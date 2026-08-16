use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use base64::Engine as _;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PhantomConfig {
    pub openai_api_key: String,
    pub groq_api_key: String,
    pub audio_source: String,
    pub theme: String,
    pub resume_text: String,
    pub job_description: String,
    pub tts_enabled: bool,
}

impl Default for PhantomConfig {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            groq_api_key: String::new(),
            audio_source: "both".to_string(),
            theme: "normal".to_string(),
            resume_text: String::new(),
            job_description: String::new(),
            tts_enabled: false,
        }
    }
}

fn config_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("AudioDeviceManager");
    fs::create_dir_all(&dir).ok();
    dir
}

fn config_path() -> PathBuf {
    config_dir().join("config.enc")
}

fn derive_key() -> [u8; 32] {
    let machine = std::env::var("COMPUTERNAME").unwrap_or_default();
    let user = std::env::var("USERNAME").unwrap_or_default();
    let seed = format!("phantom-{}-{}-salt-v1", machine, user);
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.finalize().into()
}

/// Encrypt data using AES-256-GCM (public for session file encryption)
pub fn encrypt_data(data: &[u8]) -> Result<Vec<u8>, String> {
    encrypt(data)
}

fn encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    let key = derive_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Key error: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| format!("Encryption error: {e}"))?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 12 {
        return Err("Data too short".to_string());
    }

    let key = derive_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Key error: {e}"))?;

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {e}"))
}

/// Internal (non-command) version for use from async pipeline code.
pub fn load_config_internal() -> Result<PhantomConfig, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(PhantomConfig::default());
    }

    let encrypted = fs::read(&path).map_err(|e| format!("Read error: {e}"))?;
    let decrypted = decrypt(&encrypted)?;
    serde_json::from_slice(&decrypted).map_err(|e| format!("Parse error: {e}"))
}

#[tauri::command]
pub fn load_config() -> Result<PhantomConfig, String> {
    load_config_internal()
}

#[tauri::command]
pub fn save_config(config: PhantomConfig, cache: tauri::State<'_, ConfigCache>) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(&config).map_err(|e| format!("Serialize error: {e}"))?;
    let encrypted = encrypt(&json)?;
    let path = config_path();
    fs::write(&path, encrypted).map_err(|e| format!("Write error: {e}"))?;
    cache.invalidate(&config);
    Ok(())
}

// ---------------------------------------------------------------------------
// ConfigCache — avoids re-reading + decrypting config from disk every pipeline
// ---------------------------------------------------------------------------

pub struct ConfigCache {
    inner: Mutex<Option<PhantomConfig>>,
}

impl ConfigCache {
    pub fn new() -> Self {
        let config = load_config_internal().ok();
        Self { inner: Mutex::new(config) }
    }

    /// Get cached config, falling back to disk if cache is empty
    pub fn get(&self) -> Result<PhantomConfig, String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cfg) = *guard {
            Ok(cfg.clone())
        } else {
            drop(guard);
            let cfg = load_config_internal()?;
            *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(cfg.clone());
            Ok(cfg)
        }
    }

    /// Invalidate cache — call after save_config
    pub fn invalidate(&self, new_config: &PhantomConfig) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(new_config.clone());
    }
}

/// Extract text from a PDF file given its bytes (base64 encoded from frontend)
#[tauri::command]
pub fn parse_pdf(pdf_b64: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&pdf_b64)
        .map_err(|e| format!("Base64 decode error: {e}"))?;

    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| format!("PDF parse error: {e}"))?;

    // Clean up extracted text — remove excessive whitespace
    let cleaned: String = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(cleaned)
}
