//! WebSocket protocol messages
//!
//! Compatible with the MobileCLI mobile app protocol.

use serde::{Deserialize, Serialize};

/// Messages sent from mobile client to server
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        auth_token: Option<String>,
        client_version: String,
    },
    Subscribe {
        session_id: String,
    },
    Unsubscribe {
        session_id: String,
    },
    SendInput {
        session_id: String,
        text: String,
        #[serde(default)]
        raw: bool,
        #[serde(default)]
        client_msg_id: Option<String>,
    },
    /// Resize PTY - mobile sends terminal dimensions
    PtyResize {
        session_id: String,
        cols: u16,
        rows: u16,
    },
    /// Heartbeat ping
    Ping,
    /// Request list of available sessions
    GetSessions,
    /// Rename a session
    RenameSession {
        session_id: String,
        new_name: String,
    },
}

/// Messages sent from server to mobile client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        server_version: String,
        authenticated: bool,
    },
    Error {
        code: String,
        message: String,
    },
    /// Raw PTY bytes (base64 encoded) - preserves all ANSI codes and formatting
    PtyBytes {
        session_id: String,
        data: String, // base64 encoded
    },
    /// Session info
    SessionInfo {
        session_id: String,
        name: String,
        command: String,
        project_path: String,
        started_at: String,
    },
    /// List of available sessions
    Sessions {
        sessions: Vec<SessionListItem>,
    },
    /// Session ended
    SessionEnded {
        session_id: String,
        exit_code: i32,
    },
    /// Session renamed
    SessionRenamed {
        session_id: String,
        new_name: String,
    },
    /// PTY resized confirmation
    PtyResized {
        session_id: String,
        cols: u16,
        rows: u16,
    },
    /// Heartbeat pong
    Pong,
}

/// Session list item for GetSessions response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    pub session_id: String,
    pub name: String,
    pub command: String,
    pub project_path: String,
    pub ws_port: u16,
    pub started_at: String,
    /// Explicit CLI type identifier for mobile app disambiguation
    pub cli_type: String,
}

/// Connection info for QR code / pairing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// WebSocket URL (e.g., ws://192.168.1.100:9847)
    pub ws_url: String,
    /// Session ID
    pub session_id: String,
    /// Session name (optional)
    pub session_name: Option<String>,
    /// Optional encryption key (base64)
    pub encryption_key: Option<String>,
    /// Server version
    pub version: String,
}

impl ConnectionInfo {
    /// Encode as JSON for QR code (full format)
    pub fn to_qr_data(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!("Failed to serialize ConnectionInfo: {}", e);
                String::new()
            }
        }
    }

    /// Encode as compact string for QR code (smaller QR)
    /// Format: mobilecli://host:port/session_id[/name]
    pub fn to_compact_qr(&self) -> String {
        // Extract host:port from ws_url
        let host_port = self
            .ws_url
            .strip_prefix("ws://")
            .or_else(|| self.ws_url.strip_prefix("wss://"))
            .unwrap_or(&self.ws_url);

        // Use shortened session ID (first 12 chars for better collision resistance)
        // 12 hex chars = 48 bits of entropy, birthday collision at ~16M sessions
        let short_id = if self.session_id.len() > 12 {
            &self.session_id[..12]
        } else {
            &self.session_id
        };

        // Build compact URL
        if let Some(name) = &self.session_name {
            format!("mobilecli://{}/{}/{}", host_port, short_id, urlencoding::encode(name))
        } else {
            format!("mobilecli://{}/{}", host_port, short_id)
        }
    }
}
