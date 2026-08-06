use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::State;

// ---------------------------------------------------------------------------
// AudioEngine
// ---------------------------------------------------------------------------

pub struct AudioEngine {
    pub is_recording: Arc<AtomicBool>,
    system_samples: Arc<Mutex<Vec<f32>>>,
    mic_samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: Arc<Mutex<u32>>,
    channels: Arc<Mutex<u16>>,
    active_streams: Mutex<Vec<Box<dyn StreamHandle>>>,
}

// A type-erased wrapper so we can store heterogeneous Stream types.
trait StreamHandle: Send {
    fn stop(&self);
}

struct OwnedStream(cpal::Stream);

// SAFETY: cpal streams on Windows are fine to send across threads.
unsafe impl Send for OwnedStream {}

impl StreamHandle for OwnedStream {
    fn stop(&self) {
        let _ = self.0.pause();
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            system_samples: Arc::new(Mutex::new(Vec::new())),
            mic_samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(44100)),
            channels: Arc::new(Mutex::new(1)),
            active_streams: Mutex::new(Vec::new()),
        }
    }

    /// Clear all buffers and start capturing based on audio_source setting.
    /// audio_source: "both", "system", or "mic"
    /// Returns Ok(None) on full success, Ok(Some(warning)) on partial, Err on total failure.
    pub fn start_recording(&self, audio_source: &str) -> Result<Option<String>, String> {
        // Clear previous data
        self.system_samples.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.mic_samples.lock().unwrap_or_else(|e| e.into_inner()).clear();

        let host = cpal::default_host();

        let capture_mic = audio_source != "system";
        let capture_system = audio_source != "mic";

        let mut mic_rate: Option<u32> = None;
        let mut sys_rate: Option<u32> = None;

        // ---- Microphone -------------------------------------------------------
        let mic_stream: Option<Box<dyn StreamHandle>> = if !capture_mic { None } else {
            match host.default_input_device() {
                None => None,
                Some(device) => {
                    match device.default_input_config() {
                        Err(_) => None,
                        Ok(cfg) => {
                            let rate = cfg.sample_rate().0;
                            let ch = cfg.channels();
                            mic_rate = Some(rate);

                            let mic_buf = Arc::clone(&self.mic_samples);
                            let stream = build_input_stream_f32(
                                &device,
                                &cfg.into(),
                                move |data: &[f32]| {
                                    // Downmix to mono if multi-channel
                                    if ch > 1 {
                                        let mono: Vec<f32> = data.chunks(ch as usize)
                                            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                                            .collect();
                                        mic_buf.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&mono);
                                    } else {
                                        mic_buf.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(data);
                                    }
                                },
                            );
                            match stream {
                                Err(_) => None,
                                Ok(s) => Some(s),
                            }
                        }
                    }
                }
            }
        };

        // ---- System loopback --------------------------------------------------
        let loopback_stream: Option<Box<dyn StreamHandle>> = if !capture_system { None } else {
            match host.default_output_device() {
                None => None,
                Some(device) => {
                    match device.default_output_config() {
                        Err(_) => None,
                        Ok(cfg) => {
                            let rate = cfg.sample_rate().0;
                            let ch = cfg.channels();
                            sys_rate = Some(rate);

                            let sys_buf = Arc::clone(&self.system_samples);
                            let stream = build_input_stream_f32(
                                &device,
                                &cfg.into(),
                                move |data: &[f32]| {
                                    // Downmix to mono if multi-channel (system is usually stereo)
                                    if ch > 1 {
                                        let mono: Vec<f32> = data.chunks(ch as usize)
                                            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                                            .collect();
                                        sys_buf.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&mono);
                                    } else {
                                        sys_buf.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(data);
                                    }
                                },
                            );
                            match stream {
                                Err(_) => None,
                                Ok(s) => Some(s),
                            }
                        }
                    }
                }
            }
        };

        let mic_ok = mic_stream.is_some();
        let loopback_ok = loopback_stream.is_some();

        // Set sample rate from the best available source; always mono output
        let final_rate = mic_rate.or(sys_rate).unwrap_or(48000);
        *self.sample_rate.lock().unwrap_or_else(|e| e.into_inner()) = final_rate;
        *self.channels.lock().unwrap_or_else(|e| e.into_inner()) = 1;

        let mut streams = self.active_streams.lock().unwrap_or_else(|e| e.into_inner());
        streams.clear();
        if let Some(s) = mic_stream {
            streams.push(s);
        }
        if let Some(s) = loopback_stream {
            streams.push(s);
        }

        if streams.is_empty() {
            return Err("No audio streams could be started".into());
        }

        self.is_recording.store(true, Ordering::SeqCst);

        // Warn if a requested source failed
        let warning = if capture_mic && capture_system {
            match (mic_ok, loopback_ok) {
                (false, true) => Some("Mic unavailable — recording system audio only".into()),
                (true, false) => Some("System audio unavailable — recording mic only".into()),
                _ => None,
            }
        } else {
            None
        };

        Ok(warning)
    }

    /// Stop all streams, mix buffers, encode WAV and return raw bytes.
    pub fn stop_recording(&self) -> Vec<u8> {
        self.is_recording.store(false, Ordering::SeqCst);

        // Stop and drop all streams
        {
            let mut streams = self.active_streams.lock().unwrap_or_else(|e| e.into_inner());
            for s in streams.iter() {
                s.stop();
            }
            streams.clear();
        }

        let system = self.system_samples.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mic = self.mic_samples.lock().unwrap_or_else(|e| e.into_inner()).clone();

        let sample_rate = *self.sample_rate.lock().unwrap_or_else(|e| e.into_inner());
        let channels = *self.channels.lock().unwrap_or_else(|e| e.into_inner());

        let mixed = mix_audio(&system, &mic);
        encode_wav(&mixed, sample_rate, channels).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// RecordingStore
// ---------------------------------------------------------------------------

pub struct RecordingStore {
    pub data: Mutex<Option<Vec<u8>>>,
}

impl RecordingStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_recording_data(store: State<'_, RecordingStore>) -> Result<String, String> {
    let guard = store.data.lock().map_err(|e| e.to_string())?;
    match guard.as_ref() {
        None => Err("No recording available".into()),
        Some(bytes) => Ok(base64::engine::general_purpose::STANDARD.encode(bytes)),
    }
}

// ---------------------------------------------------------------------------
// Helper: build an input stream that normalises to f32
// ---------------------------------------------------------------------------

fn build_input_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    callback: impl FnMut(&[f32]) + Send + 'static,
) -> Result<Box<dyn StreamHandle>, String> {
    let cb = Arc::new(Mutex::new(callback));

    // Try F32 first
    let cb_f32 = Arc::clone(&cb);
    let stream_f32 = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            (cb_f32.lock().unwrap_or_else(|e| e.into_inner()))(data);
        },
        |_e| { /* stream error — silent in release */ },
        None,
    );

    if let Ok(stream) = stream_f32 {
        let _ = stream.play();
        return Ok(Box::new(OwnedStream(stream)));
    }

    // Fall back to I16 — convert to f32 inline
    let cb_i16 = Arc::clone(&cb);
    let stream_i16 = device.build_input_stream(
        config,
        move |data: &[i16], _| {
            let floats: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
            (cb_i16.lock().unwrap_or_else(|e| e.into_inner()))(&floats);
        },
        |_e| { /* stream error — silent in release */ },
        None,
    );

    stream_i16
        .map(|s| {
            let _ = s.play();
            Box::new(OwnedStream(s)) as Box<dyn StreamHandle>
        })
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// mix_audio
// ---------------------------------------------------------------------------

pub fn mix_audio(system: &[f32], mic: &[f32]) -> Vec<f32> {
    if system.is_empty() {
        return mic.to_vec();
    }
    if mic.is_empty() {
        return system.to_vec();
    }

    let len = system.len().max(mic.len());
    (0..len)
        .map(|i| {
            let s = system.get(i).copied().unwrap_or(0.0);
            let m = mic.get(i).copied().unwrap_or(0.0);
            (s + m) * 0.5
        })
        .collect()
}

// ---------------------------------------------------------------------------
// encode_wav
// ---------------------------------------------------------------------------

pub fn encode_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut buf, spec).map_err(|e| e.to_string())?;
        for &sample in samples {
            let s = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(s).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    }

    Ok(buf.into_inner())
}
