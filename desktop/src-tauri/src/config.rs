// Config module - Persistent configuration and secure key storage

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

/// Application operating mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    /// Host mode: runs PTY sessions, acts as server
    Host,
    /// Client mode: connects to a host, no local PTY
    Client,
}

impl Default for AppMode {
    fn default() -> Self {
        AppMode::Host
    }
}

/// Codex approval policy for tool execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CodexApprovalPolicy {
    /// Only trusted commands run without approval
    Untrusted,
    /// Ask after failures
    OnFailure,
    /// Model decides when to ask
    OnRequest,
    /// Fully autonomous (dangerous)
    Never,
}

impl Default for CodexApprovalPolicy {
    fn default() -> Self {
        CodexApprovalPolicy::Untrusted
    }
}

impl CodexApprovalPolicy {
    /// Get the CLI flag value for this policy
    pub fn as_flag(&self) -> &'static str {
        match self {
            CodexApprovalPolicy::Untrusted => "untrusted",
            CodexApprovalPolicy::OnFailure => "on-failure",
            CodexApprovalPolicy::OnRequest => "on-request",
            CodexApprovalPolicy::Never => "never",
        }
    }

    /// Parse a string to CodexApprovalPolicy
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "untrusted" => Some(CodexApprovalPolicy::Untrusted),
            "on-failure" => Some(CodexApprovalPolicy::OnFailure),
            "on-request" => Some(CodexApprovalPolicy::OnRequest),
            "never" => Some(CodexApprovalPolicy::Never),
            _ => None,
        }
    }
}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Operating mode (host or client)
    pub mode: AppMode,
    /// App version (for migration compatibility)
    pub version: String,
    /// Whether this is the first time running the app
    pub first_run: bool,
    /// List of relay server URLs (primary + fallbacks)
    pub relay_urls: Vec<String>,
    /// Last successfully connected host URL (for client mode)
    pub last_host_url: Option<String>,
    /// Last successfully connected room code (for client mode)
    pub last_room_code: Option<String>,
    /// WebSocket server port for host mode
    pub ws_port: u16,
    /// Claude: Open sessions with --dangerously-skip-permissions flag
    #[serde(default)]
    pub claude_skip_permissions: bool,
    /// Codex: Approval policy for tool execution
    #[serde(default)]
    pub codex_approval_policy: CodexApprovalPolicy,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: AppMode::Host,
            version: env!("CARGO_PKG_VERSION").to_string(),
            first_run: true,
            relay_urls: vec![
                "wss://relay.mobilecli.app".to_string(),  // Primary (with SSL via Caddy)
            ],
            last_host_url: None,
            last_room_code: None,
            ws_port: 9847,
            claude_skip_permissions: false,
            codex_approval_policy: CodexApprovalPolicy::default(),
        }
    }
}

const CONFIG_STORE: &str = "config.json";
const SECRETS_STORE: &str = "secrets.json";

/// Load application configuration from persistent storage
pub fn load_config(app: &AppHandle) -> Result<AppConfig, String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    // Check if we have saved config
    if let Some(value) = store.get("config") {
        serde_json::from_value(value.clone())
            .map_err(|e| format!("Failed to parse config: {}", e))
    } else {
        // Return default config for new installations
        Ok(AppConfig::default())
    }
}

/// Save application configuration to persistent storage
pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let value = serde_json::to_value(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    store.set("config", value);
    store
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

/// Store encryption key securely (for relay E2E encryption)
pub fn store_encryption_key(app: &AppHandle, key: &[u8; 32]) -> Result<(), String> {
    let store = app
        .store(SECRETS_STORE)
        .map_err(|e| format!("Failed to open secrets store: {}", e))?;

    // Encode key as base64 for JSON storage
    let key_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key);
    store.set("encryption_key", serde_json::json!(key_b64));
    store
        .save()
        .map_err(|e| format!("Failed to save encryption key: {}", e))?;

    Ok(())
}

/// Load encryption key from secure storage
pub fn load_encryption_key(app: &AppHandle) -> Result<Option<[u8; 32]>, String> {
    let store = app
        .store(SECRETS_STORE)
        .map_err(|e| format!("Failed to open secrets store: {}", e))?;

    if let Some(value) = store.get("encryption_key") {
        let key_b64 = value
            .as_str()
            .ok_or("Encryption key is not a string")?;

        let key_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, key_b64)
            .map_err(|e| format!("Failed to decode encryption key: {}", e))?;

        if key_bytes.len() != 32 {
            return Err(format!(
                "Invalid encryption key length: {} (expected 32)",
                key_bytes.len()
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        Ok(Some(key))
    } else {
        Ok(None)
    }
}

/// Delete stored encryption key
pub fn delete_encryption_key(app: &AppHandle) -> Result<(), String> {
    let store = app
        .store(SECRETS_STORE)
        .map_err(|e| format!("Failed to open secrets store: {}", e))?;

    store.delete("encryption_key");
    store
        .save()
        .map_err(|e| format!("Failed to save after delete: {}", e))?;

    Ok(())
}

/// Get the config directory path
pub fn get_config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.mode, AppMode::Host);
        assert!(config.first_run);
        assert_eq!(config.ws_port, 9847);
        assert!(!config.relay_urls.is_empty());
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.mode, loaded.mode);
        assert_eq!(config.version, loaded.version);
        assert_eq!(config.first_run, loaded.first_run);
    }

    #[test]
    fn test_app_mode_serialization() {
        let host = serde_json::to_string(&AppMode::Host).unwrap();
        assert_eq!(host, "\"host\"");

        let client = serde_json::to_string(&AppMode::Client).unwrap();
        assert_eq!(client, "\"client\"");

        let parsed: AppMode = serde_json::from_str("\"host\"").unwrap();
        assert_eq!(parsed, AppMode::Host);
    }
}
