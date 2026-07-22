use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhantomConfig {
    pub openai_api_key: String,
    pub groq_api_key: String,
    pub anthropic_api_key: String,
    pub google_api_key: String,
    pub default_mode: String,
    pub audio_source: String,
    pub overlay_position: String,
    pub theme: String,
    pub resume_text: String,
    pub job_description: String,
    pub activation_mode: String,
    pub silence_duration_secs: u32,
    pub auto_start_on_meeting: bool,
}

impl Default for PhantomConfig {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            groq_api_key: String::new(),
            anthropic_api_key: String::new(),
            google_api_key: String::new(),
            default_mode: "general".to_string(),
            audio_source: "both".to_string(),
            overlay_position: "top-center".to_string(),
            theme: "normal".to_string(),
            resume_text: String::new(),
            job_description: String::new(),
            activation_mode: "manual".to_string(),
            silence_duration_secs: 3,
            auto_start_on_meeting: false,
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
pub fn save_config(config: PhantomConfig) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(&config).map_err(|e| format!("Serialize error: {e}"))?;
    let encrypted = encrypt(&json)?;
    let path = config_path();
    fs::write(&path, encrypted).map_err(|e| format!("Write error: {e}"))?;
    Ok(())
}
