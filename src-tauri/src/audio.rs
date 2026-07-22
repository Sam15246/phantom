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
    // Streams are held here while recording.  cpal::Stream is !Send on some
    // platforms, so we wrap in Option and store inside a thread-local-friendly
    // Mutex<Vec<_>>.  We use a raw pointer trick: we keep the streams alive by
    // boxing them and leaking temporarily — simpler and safe because we drop
    // them explicitly in stop_recording.
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

    /// Clear all buffers and start capturing microphone + (optionally) loopback.
    pub fn start_recording(&self) -> Result<(), String> {
        // Clear previous data
        self.system_samples.lock().unwrap().clear();
        self.mic_samples.lock().unwrap().clear();

        let host = cpal::default_host();

        // ---- Microphone -------------------------------------------------------
        let mic_stream: Option<Box<dyn StreamHandle>> = {
            match host.default_input_device() {
                None => {
                    eprintln!("[audio] No default input device found");
                    None
                }
                Some(device) => {
                    match device.default_input_config() {
                        Err(e) => {
                            eprintln!("[audio] Cannot get mic config: {e}");
                            None
                        }
                        Ok(cfg) => {
                            // Store rate / channels from mic config
                            *self.sample_rate.lock().unwrap() = cfg.sample_rate().0;
                            *self.channels.lock().unwrap() = cfg.channels();

                            let mic_buf = Arc::clone(&self.mic_samples);
                            let stream = build_input_stream_f32(
                                &device,
                                &cfg.into(),
                                move |data: &[f32]| {
                                    mic_buf.lock().unwrap().extend_from_slice(data);
                                },
                            );
                            match stream {
                                Err(e) => {
                                    eprintln!("[audio] Mic stream error: {e}");
                                    None
                                }
                                Ok(s) => Some(s),
                            }
                        }
                    }
                }
            }
        };

        // ---- System loopback --------------------------------------------------
        let loopback_stream: Option<Box<dyn StreamHandle>> = {
            match host.default_output_device() {
                None => {
                    eprintln!("[audio] No default output device for loopback");
                    None
                }
                Some(device) => {
                    match device.default_output_config() {
                        Err(e) => {
                            eprintln!("[audio] Cannot get loopback config: {e}");
                            None
                        }
                        Ok(cfg) => {
                            let sys_buf = Arc::clone(&self.system_samples);
                            // On WASAPI, build_input_stream on an output device gives loopback.
                            let stream = build_input_stream_f32(
                                &device,
                                &cfg.into(),
                                move |data: &[f32]| {
                                    sys_buf.lock().unwrap().extend_from_slice(data);
                                },
                            );
                            match stream {
                                Err(e) => {
                                    eprintln!("[audio] Loopback stream error (continuing mic-only): {e}");
                                    None
                                }
                                Ok(s) => Some(s),
                            }
                        }
                    }
                }
            }
        };

        let mut streams = self.active_streams.lock().unwrap();
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
        Ok(())
    }

    /// Stop all streams, mix buffers, encode WAV and return raw bytes.
    pub fn stop_recording(&self) -> Vec<u8> {
        self.is_recording.store(false, Ordering::SeqCst);

        // Stop and drop all streams
        {
            let mut streams = self.active_streams.lock().unwrap();
            for s in streams.iter() {
                s.stop();
            }
            streams.clear();
        }

        let system = self.system_samples.lock().unwrap().clone();
        let mic = self.mic_samples.lock().unwrap().clone();

        let sample_rate = *self.sample_rate.lock().unwrap();
        let channels = *self.channels.lock().unwrap();

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
    // Wrap callback in Arc<Mutex<>> so it can be shared between the two
    // attempted stream builds (only one will actually capture it).
    let cb = Arc::new(Mutex::new(callback));

    // Try F32 first
    let cb_f32 = Arc::clone(&cb);
    let stream_f32 = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            (cb_f32.lock().unwrap())(data);
        },
        |e| eprintln!("[audio] stream error: {e}"),
        None,
    );

    if let Ok(stream) = stream_f32 {
        if let Err(e) = stream.play() {
            eprintln!("[audio] stream play error: {e}");
        }
        return Ok(Box::new(OwnedStream(stream)));
    }

    // Fall back to I16 — convert to f32 inline
    let cb_i16 = Arc::clone(&cb);
    let stream_i16 = device.build_input_stream(
        config,
        move |data: &[i16], _| {
            let floats: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
            (cb_i16.lock().unwrap())(&floats);
        },
        |e| eprintln!("[audio] stream error: {e}"),
        None,
    );

    stream_i16
        .map(|s| {
            if let Err(e) = s.play() {
                eprintln!("[audio] stream play error: {e}");
            }
            Box::new(OwnedStream(s)) as Box<dyn StreamHandle>
        })
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// mix_audio
// ---------------------------------------------------------------------------

pub fn mix_audio(system: &[f32], mic: &[f32]) -> Vec<f32> {
    let len = system.len().max(mic.len());
    if len == 0 {
        return Vec::new();
    }
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
