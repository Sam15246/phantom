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
        let mut mic_fail_reason: Option<String> = None;
        let mut sys_fail_reason: Option<String> = None;

        // ---- Microphone -------------------------------------------------------
        let mic_stream: Option<Box<dyn StreamHandle>> = if !capture_mic { None } else {
            match host.default_input_device() {
                None => { mic_fail_reason = Some("no default input device".into()); None }
                Some(device) => {
                    match device.default_input_config() {
                        Err(e) => { mic_fail_reason = Some(format!("config error: {e}")); None }
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
                                Err(e) => { mic_fail_reason = Some(format!("stream build error: {e}")); mic_rate = None; None }
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
                None => { sys_fail_reason = Some("no default output device".into()); None }
                Some(device) => {
                    match device.default_output_config() {
                        Err(e) => { sys_fail_reason = Some(format!("config error: {e}")); None }
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
                                Err(e) => { sys_fail_reason = Some(format!("stream build error: {e}")); sys_rate = None; None }
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
            let mut err = "No audio streams could be started".to_string();
            if let Some(ref r) = mic_fail_reason {
                err.push_str(&format!(" (mic: {r})"));
            }
            if let Some(ref r) = sys_fail_reason {
                err.push_str(&format!(" (system: {r})"));
            }
            return Err(err);
        }

        self.is_recording.store(true, Ordering::SeqCst);

        // Warn if a requested source failed, including the reason
        let warning = if capture_mic && capture_system {
            match (mic_ok, loopback_ok) {
                (false, true) => {
                    let reason = mic_fail_reason.as_deref().unwrap_or("unknown");
                    Some(format!("Mic unavailable ({reason}) — recording system audio only"))
                }
                (true, false) => {
                    let reason = sys_fail_reason.as_deref().unwrap_or("unknown");
                    Some(format!("System audio unavailable ({reason}) — recording mic only"))
                }
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

        // Trim leading/trailing silence — saves transcription time
        let trimmed = trim_silence(&mixed, sample_rate);

        encode_wav(trimmed, sample_rate, channels).unwrap_or_default()
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
    // Poison-tolerant: data is a self-contained Option<Vec<u8>>, safe to recover
    let guard = store.data.lock().unwrap_or_else(|e| e.into_inner());
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
// downsample — linear interpolation resampler
// ---------------------------------------------------------------------------

fn downsample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;
        let a = samples[idx];
        let b = samples.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac as f32);
    }
    out
}

// ---------------------------------------------------------------------------
// trim_silence — remove leading/trailing dead air
// ---------------------------------------------------------------------------

fn trim_silence(samples: &[f32], sample_rate: u32) -> &[f32] {
    if samples.is_empty() {
        return samples;
    }
    // -40dB threshold ≈ amplitude 0.01 — only trims actual dead silence
    let threshold: f32 = 0.01;
    // 300ms safety buffer on each side to avoid clipping soft speech
    let buffer = (sample_rate as usize * 300) / 1000;

    let first = samples.iter().position(|&s| s.abs() > threshold);
    let last = samples.iter().rposition(|&s| s.abs() > threshold);

    match (first, last) {
        (Some(f), Some(l)) => {
            let start = f.saturating_sub(buffer);
            let end = (l + buffer + 1).min(samples.len());
            &samples[start..end]
        }
        // Entire audio is below threshold — return as-is (don't trim to nothing)
        _ => samples,
    }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- mix_audio --

    #[test]
    fn mix_audio_empty_passthrough() {
        assert_eq!(mix_audio(&[], &[1.0, 2.0]), vec![1.0, 2.0]);
        assert_eq!(mix_audio(&[1.0, 2.0], &[]), vec![1.0, 2.0]);
    }

    #[test]
    fn mix_audio_averages() {
        let out = mix_audio(&[1.0, 0.0], &[0.0, 1.0]);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn mix_audio_unequal_length() {
        let out = mix_audio(&[1.0, 1.0, 1.0], &[1.0]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], 1.0); // (1+1)/2
        assert_eq!(out[2], 0.5); // (1+0)/2
    }

    // -- trim_silence --

    #[test]
    fn trim_silence_empty() {
        let out = trim_silence(&[], 44100);
        assert!(out.is_empty());
    }

    #[test]
    fn trim_silence_all_quiet() {
        let quiet = vec![0.001; 44100];
        let out = trim_silence(&quiet, 44100);
        assert_eq!(out.len(), quiet.len(), "all-quiet should return as-is");
    }

    #[test]
    fn trim_silence_preserves_loud() {
        let mut samples = vec![0.0; 44100]; // 1s silence
        samples.push(0.5); // loud sample
        samples.extend(vec![0.0; 44100]); // 1s silence
        let out = trim_silence(&samples, 44100);
        // Should include the loud sample + safety buffer on each side
        assert!(out.len() < samples.len());
        assert!(out.iter().any(|&s| s == 0.5));
    }

    // -- downsample --

    #[test]
    fn downsample_identity() {
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let out = downsample(&input, 44100, 44100);
        assert_eq!(out, input);
    }

    #[test]
    fn downsample_2x() {
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let out = downsample(&input, 44100, 22050);
        assert_eq!(out.len(), 50);
    }

    // -- encode_wav --

    #[test]
    fn encode_wav_roundtrip() {
        let samples = vec![0.0, 0.5, -0.5, 1.0, -1.0];
        let wav = encode_wav(&samples, 16000, 1).expect("encode should succeed");
        // WAV header is 44 bytes, then 2 bytes per sample
        assert!(wav.len() >= 44 + samples.len() * 2);
        // Verify it's a valid WAV by reading it back
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).expect("valid WAV");
        assert_eq!(reader.spec().sample_rate, 16000);
        assert_eq!(reader.spec().channels, 1);
    }

    #[test]
    fn encode_wav_clamps_out_of_range() {
        // Values beyond [-1, 1] should be clamped, not overflow
        let samples = vec![2.0, -2.0];
        let wav = encode_wav(&samples, 16000, 1).expect("encode should succeed");
        let mut reader = hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap();
        let decoded: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(decoded[0], i16::MAX);
        assert_eq!(decoded[1], i16::MIN + 1); // -1.0 * 32767 = -32767
    }
}
