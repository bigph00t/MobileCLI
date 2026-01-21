// Relay client module - Connects to remote relay server with E2E encryption
//
// Security: All messages are encrypted with XSalsa20-Poly1305 (NaCl secretbox)
// before being sent through the relay. The relay server only sees opaque blobs.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use crypto_secretbox::{
    aead::{Aead, KeyInit},
    XSalsa20Poly1305,
};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Listener};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};

// Re-use the WebSocket message types for relay
use crate::config;
use crate::db::Database;
use crate::parser::ActivityType;
use crate::ws::{
    ActivityInfo, ClientMessage, DirectoryEntry, MessageInfo, ServerMessage, SessionInfo,
};

// Default relay server URL (via Caddy reverse proxy with TLS)
const DEFAULT_RELAY_URL: &str = "wss://relay.mobilecli.app";

/// Get configured relay URLs (from config, env, or defaults)
fn get_relay_urls(app: Option<&AppHandle>) -> Vec<String> {
    // 1. Check environment variable first (highest priority, for testing/dev)
    if let Ok(custom_url) = std::env::var("MOBILECLI_RELAY_URL") {
        return vec![custom_url];
    }

    // 2. Check config if AppHandle provided
    if let Some(app) = app {
        if let Ok(config) = config::load_config(app) {
            if !config.relay_urls.is_empty() {
                return config.relay_urls;
            }
        }
    }

    // 3. Fall back to default
    vec![DEFAULT_RELAY_URL.to_string()]
}

/// Get the primary relay URL for display purposes
fn get_relay_url(app: Option<&AppHandle>) -> String {
    get_relay_urls(app)
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string())
}

/// Reconnection strategy with exponential backoff
#[derive(Debug, Clone)]
pub struct ReconnectStrategy {
    base_delay_ms: u64,
    max_delay_ms: u64,
    attempt: u32,
}

impl Default for ReconnectStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconnectStrategy {
    pub fn new() -> Self {
        Self {
            base_delay_ms: 1000, // Start at 1 second
            max_delay_ms: 30000, // Max 30 seconds
            attempt: 0,
        }
    }

    /// Get the next delay duration using exponential backoff
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.base_delay_ms * 2u64.pow(self.attempt);
        let delay = delay.min(self.max_delay_ms);
        self.attempt += 1;
        Duration::from_millis(delay)
    }

    /// Reset the backoff counter (call on successful connection)
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Get current attempt count
    pub fn attempts(&self) -> u32 {
        self.attempt
    }
}

/// Relay connection status for UI
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RelayStatus {
    Connected,
    Reconnecting,
    Disconnected,
}

/// Encryption key (32 bytes for XSalsa20-Poly1305)
pub type EncryptionKey = [u8; 32];

/// Relay connection state
pub struct RelayConnection {
    /// Encryption key for this session
    key: EncryptionKey,
    /// Room code assigned by relay
    room_code: Option<String>,
    /// Channel to send messages to relay
    sender: Option<mpsc::UnboundedSender<String>>,
    /// Whether a client (mobile) is connected
    client_connected: bool,
}

impl RelayConnection {
    fn new() -> Self {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        Self {
            key,
            room_code: None,
            sender: None,
            client_connected: false,
        }
    }

    /// Encrypt a message using NaCl secretbox
    #[allow(dead_code)]
    fn encrypt(&self, plaintext: &str) -> Result<String, String> {
        let cipher = XSalsa20Poly1305::new((&self.key).into());

        // Generate random nonce (24 bytes for XSalsa20)
        let mut nonce_bytes = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = crypto_secretbox::Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        // Prepend nonce to ciphertext and base64 encode
        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);

        Ok(BASE64.encode(combined))
    }

    /// Decrypt a message using NaCl secretbox
    #[allow(dead_code)]
    fn decrypt(&self, encrypted: &str) -> Result<String, String> {
        let cipher = XSalsa20Poly1305::new((&self.key).into());

        // Base64 decode
        let combined = BASE64
            .decode(encrypted)
            .map_err(|e| format!("Base64 decode failed: {}", e))?;

        if combined.len() < 24 {
            return Err("Ciphertext too short".to_string());
        }

        // Extract nonce and ciphertext
        let nonce = crypto_secretbox::Nonce::from_slice(&combined[..24]);
        let ciphertext = &combined[24..];

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed: {}", e))?;

        String::from_utf8(plaintext).map_err(|e| format!("UTF-8 decode failed: {}", e))
    }
}

/// Shared relay state
pub struct RelayState {
    connection: RwLock<Option<RelayConnection>>,
    status: RwLock<RelayStatus>,
    reconnect_strategy: RwLock<ReconnectStrategy>,
}

impl Default for RelayState {
    fn default() -> Self {
        Self {
            connection: RwLock::new(None),
            status: RwLock::new(RelayStatus::Disconnected),
            reconnect_strategy: RwLock::new(ReconnectStrategy::new()),
        }
    }
}

impl RelayState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update relay status and emit event
    pub async fn set_status(&self, app: &AppHandle, status: RelayStatus) {
        let mut current = self.status.write().await;
        if *current != status {
            *current = status;
            let _ = app.emit("relay-status", status);
        }
    }

    /// Get current relay status
    pub async fn get_status(&self) -> RelayStatus {
        *self.status.read().await
    }
}

/// Messages from relay server
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RelayServerMessage {
    RoomCreated { code: String },
    ClientJoined,
    ClientLeft,
    Error { message: String },
}

/// QR code data for mobile to scan
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayQrData {
    pub url: String,
    pub room_code: String,
    pub key: String, // Base64 encoded encryption key
    pub connected: bool,
}

/// Try to connect to a specific relay URL
/// Returns the WebSocket stream and the room code on success
async fn try_connect_to_relay(
    url: &str,
) -> Result<
    (
        futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        String,
    ),
    String,
> {
    let full_url = format!("{}/host", url);
    tracing::info!("Attempting to connect to relay: {}", full_url);

    let (ws_stream, _) = connect_async(&full_url)
        .await
        .map_err(|e| format!("Failed to connect to {}: {}", url, e))?;

    let (ws_sender, mut ws_receiver) = ws_stream.split();

    // Wait for room_created message with timeout
    let room_code = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match ws_receiver.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(msg) = serde_json::from_str::<RelayServerMessage>(&text) {
                        match msg {
                            RelayServerMessage::RoomCreated { code } => {
                                tracing::info!("Relay room created: {}", code);
                                return Ok(code);
                            }
                            RelayServerMessage::Error { message } => {
                                return Err(format!("Relay error: {}", message));
                            }
                            _ => {}
                        }
                    }
                }
                Some(Err(e)) => {
                    return Err(format!("WebSocket error: {}", e));
                }
                None => {
                    return Err("Connection closed before room created".to_string());
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "Timeout waiting for room creation".to_string())??;

    Ok((ws_sender, ws_receiver, room_code))
}

/// Connect to relay with failover - tries each URL in sequence
async fn connect_with_failover(
    app: Option<&AppHandle>,
) -> Result<
    (
        futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        String,
        String,
    ),
    String,
> {
    let urls = get_relay_urls(app);

    for url in &urls {
        match try_connect_to_relay(url).await {
            Ok((sender, receiver, room_code)) => {
                tracing::info!("Successfully connected to relay: {}", url);
                return Ok((sender, receiver, room_code, url.clone()));
            }
            Err(e) => {
                tracing::warn!("Failed to connect to {}: {}", url, e);
                continue;
            }
        }
    }

    Err("All relay servers unavailable".to_string())
}

/// Start relay connection and return QR code data
pub async fn start_relay(
    app: AppHandle,
    state: Arc<RelayState>,
    db: Arc<Database>,
) -> Result<RelayQrData, String> {
    // Update status to reconnecting (we're attempting to connect)
    state.set_status(&app, RelayStatus::Reconnecting).await;

    // Create new connection with fresh key
    let mut connection = RelayConnection::new();
    let key_base64 = BASE64.encode(connection.key);

    // Try to connect with failover to backup relays
    let (mut ws_sender, mut ws_receiver, room_code, connected_url) =
        match connect_with_failover(Some(&app)).await {
            Ok(result) => {
                // Reset backoff on successful connection
                state.reconnect_strategy.write().await.reset();
                result
            }
            Err(e) => {
                state.set_status(&app, RelayStatus::Disconnected).await;
                return Err(e);
            }
        };

    // Channel for sending messages to relay
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    connection.sender = Some(tx.clone());
    connection.room_code = Some(room_code.clone());

    // Mark as connected
    state.set_status(&app, RelayStatus::Connected).await;
    tracing::info!(
        "Relay connected to {} with room {}",
        connected_url,
        room_code
    );

    // Store connection state
    {
        let mut conn = state.connection.write().await;
        *conn = Some(connection);
    }

    // Clone state for the async task
    let state_clone = state.clone();
    let app_clone = app.clone();
    let db_clone = db.clone();
    let key_for_task = {
        let conn = state.connection.read().await;
        conn.as_ref().map(|c| c.key).unwrap_or([0u8; 32])
    };

    // Create channel for sending encrypted messages to relay
    let tx_for_events = tx.clone();
    let key_for_encrypt = key_for_task;

    // Helper to encrypt and send a server message
    let _encrypt_and_send = move |msg: &ServerMessage| -> Result<(), String> {
        let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        let encrypted = encrypt_message(&key_for_encrypt, &json)?;
        tx_for_events
            .send(encrypted)
            .map_err(|e| format!("Send failed: {}", e))
    };

    // Set up event listeners to forward server messages through relay
    let tx_pty = tx.clone();
    let key_pty = key_for_task;
    app.listen("pty-output", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let output = payload["raw"]
                .as_str()
                .or_else(|| payload["output"].as_str())
                .unwrap_or("");
            let msg = ServerMessage::PtyOutput {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                output: output.to_string(),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_pty, &json) {
                    let _ = tx_pty.send(encrypted);
                }
            }
        }
    });

    let tx_msg = tx.clone();
    let key_msg = key_for_task;
    app.listen("new-message", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::NewMessage {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                role: payload["role"].as_str().unwrap_or("").to_string(),
                content: payload["content"].as_str().unwrap_or("").to_string(),
                tool_name: payload["toolName"].as_str().map(String::from),
                is_complete: payload["isComplete"].as_bool(),
                client_msg_id: payload["clientMsgId"].as_str().map(String::from),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_msg, &json) {
                    let _ = tx_msg.send(encrypted);
                }
            }
        }
    });

    let tx_wait = tx.clone();
    let key_wait = key_for_task;
    app.listen("waiting-for-input", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::WaitingForInput {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                timestamp: payload["timestamp"].as_str().unwrap_or("").to_string(),
                prompt_content: payload["promptContent"].as_str().map(String::from),
                wait_type: payload["waitType"].as_str().map(String::from),
                cli_type: payload["cliType"].as_str().map(String::from),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_wait, &json) {
                    let _ = tx_wait.send(encrypted);
                }
            }
        }
    });

    let tx_session = tx.clone();
    let key_session = key_for_task;
    app.listen("session-created", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionCreated {
                session: crate::ws::SessionInfo {
                    id: payload["id"].as_str().unwrap_or("").to_string(),
                    name: payload["name"].as_str().unwrap_or("").to_string(),
                    project_path: payload["projectPath"].as_str().unwrap_or("").to_string(),
                    created_at: payload["createdAt"].as_str().unwrap_or("").to_string(),
                    last_active_at: payload["lastActiveAt"].as_str().unwrap_or("").to_string(),
                    status: payload["status"].as_str().unwrap_or("active").to_string(),
                    cli_type: payload["cliType"].as_str().unwrap_or("claude").to_string(),
                },
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_session, &json) {
                    let _ = tx_session.send(encrypted);
                }
            }
        }
    });

    let tx_closed = tx.clone();
    let key_closed = key_for_task;
    app.listen("session-closed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionClosed {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_closed, &json) {
                    let _ = tx_closed.send(encrypted);
                }
            }
        }
    });

    let tx_resumed = tx.clone();
    let key_resumed = key_for_task;
    app.listen("session-resumed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionResumed {
                session: crate::ws::SessionInfo {
                    id: payload["id"].as_str().unwrap_or("").to_string(),
                    name: payload["name"].as_str().unwrap_or("").to_string(),
                    project_path: payload["projectPath"].as_str().unwrap_or("").to_string(),
                    created_at: payload["createdAt"].as_str().unwrap_or("").to_string(),
                    last_active_at: payload["lastActiveAt"].as_str().unwrap_or("").to_string(),
                    status: payload["status"].as_str().unwrap_or("active").to_string(),
                    cli_type: payload["cliType"].as_str().unwrap_or("claude").to_string(),
                },
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_resumed, &json) {
                    let _ = tx_resumed.send(encrypted);
                }
            }
        }
    });

    let tx_renamed = tx.clone();
    let key_renamed = key_for_task;
    app.listen("session-renamed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionRenamed {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                new_name: payload["newName"].as_str().unwrap_or("").to_string(),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_renamed, &json) {
                    let _ = tx_renamed.send(encrypted);
                }
            }
        }
    });

    let tx_deleted = tx.clone();
    let key_deleted = key_for_task;
    app.listen("session-deleted", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionDeleted {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_deleted, &json) {
                    let _ = tx_deleted.send(encrypted);
                }
            }
        }
    });

    let tx_input_err = tx.clone();
    let key_input_err = key_for_task;
    app.listen("input-error", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::Error {
                code: "input_failed".to_string(),
                message: payload["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_input_err, &json) {
                    let _ = tx_input_err.send(encrypted);
                }
            }
        }
    });

    let tx_activity = tx.clone();
    let key_activity = key_for_task;
    app.listen("activity", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let activity_type_str = payload["activityType"].as_str().unwrap_or("text");
            let activity_type = match activity_type_str {
                "thinking" => ActivityType::Thinking,
                "tool_start" => ActivityType::ToolStart,
                "tool_result" => ActivityType::ToolResult,
                "file_read" => ActivityType::FileRead,
                "file_write" => ActivityType::FileWrite,
                "bash_command" => ActivityType::BashCommand,
                "code_diff" => ActivityType::CodeDiff,
                "progress" => ActivityType::Progress,
                "user_prompt" => ActivityType::UserPrompt,
                _ => ActivityType::Text,
            };

            let msg = ServerMessage::Activity {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                activity_type,
                content: payload["content"].as_str().unwrap_or("").to_string(),
                tool_name: payload["toolName"].as_str().map(String::from),
                tool_params: payload["toolParams"].as_str().map(String::from),
                file_path: payload["filePath"].as_str().map(String::from),
                is_streaming: payload["isStreaming"].as_bool().unwrap_or(false),
                timestamp: payload["timestamp"].as_str().unwrap_or("").to_string(),
                uuid: payload["uuid"].as_str().map(String::from),
                source: payload["source"].as_str().map(String::from),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Ok(encrypted) = encrypt_message(&key_activity, &json) {
                    let _ = tx_activity.send(encrypted);
                }
            }
        }
    });

    // Clone tx for use in the message handler (for direct responses)
    let tx_response = tx.clone();
    let key_response = key_for_task;

    // Spawn task to handle relay messages
    tokio::spawn(async move {
        let cipher = XSalsa20Poly1305::new((&key_for_task).into());

        loop {
            tokio::select! {
                // Messages from relay
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Try to parse as relay protocol message
                            if let Ok(relay_msg) = serde_json::from_str::<RelayServerMessage>(&text) {
                                match relay_msg {
                                    RelayServerMessage::ClientJoined => {
                                        tracing::info!("Mobile client connected to relay");
                                        if let Some(conn) = state_clone.connection.write().await.as_mut() {
                                            conn.client_connected = true;
                                        }
                                        let _ = app_clone.emit("relay-client-connected", ());
                                    }
                                    RelayServerMessage::ClientLeft => {
                                        tracing::info!("Mobile client disconnected from relay");
                                        if let Some(conn) = state_clone.connection.write().await.as_mut() {
                                            conn.client_connected = false;
                                        }
                                        let _ = app_clone.emit("relay-client-disconnected", ());
                                    }
                                    RelayServerMessage::Error { message } => {
                                        tracing::error!("Relay error: {}", message);
                                        let _ = app_clone.emit("relay-error", message);
                                    }
                                    _ => {}
                                }
                            } else {
                                // Must be encrypted data from mobile
                                // Decrypt and process as ClientMessage
                                match decrypt_message(&cipher, &text) {
                                    Ok(decrypted) => {
                                        // Note: Never log decrypted content - security risk
                                        tracing::debug!("Received encrypted message from mobile ({} bytes)", decrypted.len());
                                        // Parse as ClientMessage and forward to app
                                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&decrypted) {
                                            // Emit the same events as local WS would
                                            match &client_msg {
                                                ClientMessage::SendInput { session_id, text, raw, client_msg_id } => {
                                                    let _ = app_clone.emit(
                                                        "send-input",
                                                        serde_json::json!({
                                                            "sessionId": session_id,
                                                            "text": text,
                                                            "raw": raw,
                                                            "clientMsgId": client_msg_id,
                                                        }),
                                                    );
                                                }
                                                ClientMessage::CreateSession { project_path, name, cli_type, claude_skip_permissions, codex_approval_policy } => {
                                                    // Note: For relay, we need to handle this differently
                                                    // since create-session expects a sessionId but mobile
                                                    // doesn't have one yet. Emit relay-create-session instead.
                                                    let _ = app_clone.emit(
                                                        "relay-create-session",
                                                        serde_json::json!({
                                                            "projectPath": project_path,
                                                            "name": name,
                                                            "cliType": cli_type,
                                                            "claudeSkipPermissions": claude_skip_permissions,
                                                            "codexApprovalPolicy": codex_approval_policy,
                                                        }),
                                                    );
                                                }
                                                ClientMessage::CloseSession { session_id } => {
                                                    let _ = app_clone.emit(
                                                        "close-session",
                                                        serde_json::json!({ "sessionId": session_id }),
                                                    );
                                                }
                                                ClientMessage::ResumeSession { session_id, claude_skip_permissions } => {
                                                    // ISSUE #2: Relay doesn't pass skip_permissions (uses config default)
                                                    let _ = app_clone.emit(
                                                        "relay-resume-session",
                                                        serde_json::json!({
                                                            "sessionId": session_id,
                                                            "claudeSkipPermissions": claude_skip_permissions
                                                        }),
                                                    );
                                                }
                                                ClientMessage::Hello { .. } => {
                                                    // Send welcome message back
                                                    let msg = ServerMessage::Welcome {
                                                        server_version: "0.1.0".to_string(),
                                                        authenticated: true,
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::GetSessions => {
                                                    // Query database and send sessions back
                                                    let msg = match db_clone.get_all_sessions() {
                                                        Ok(sessions) => ServerMessage::Sessions {
                                                            sessions: sessions.into_iter().map(|s| SessionInfo {
                                                                id: s.id,
                                                                name: s.name,
                                                                project_path: s.project_path,
                                                                created_at: s.created_at,
                                                                last_active_at: s.last_active_at,
                                                                status: s.status,
                                                                cli_type: s.cli_type,
                                                            }).collect(),
                                                        },
                                                        Err(e) => ServerMessage::Error {
                                                            code: "db_error".to_string(),
                                                            message: e.to_string(),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::GetMessages { session_id, limit } => {
                                                    let msg = match db_clone.get_messages(session_id, limit.unwrap_or(100)) {
                                                        Ok(messages) => ServerMessage::Messages {
                                                            session_id: session_id.clone(),
                                                            messages: messages.into_iter().map(|m| MessageInfo {
                                                                id: m.id,
                                                                session_id: m.session_id,
                                                                role: m.role,
                                                                content: m.content,
                                                                tool_name: m.tool_name,
                                                                tool_result: m.tool_result,
                                                                timestamp: m.timestamp,
                                                            }).collect(),
                                                        },
                                                        Err(e) => ServerMessage::Error {
                                                            code: "db_error".to_string(),
                                                            message: e.to_string(),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::GetActivities { session_id, limit } => {
                                                    // Get activities with proper types from JSONL
                                                    let limit_val = limit.unwrap_or(100) as usize;
                                                    let msg = if let Ok(Some(session)) = db_clone.get_session(session_id) {
                                                        if session.cli_type == "claude" {
                                                            if let Some(ref conversation_id) = session.conversation_id {
                                                                match crate::jsonl::read_activities(&session.project_path, conversation_id) {
                                                                    Ok(activities) => {
                                                                        let activity_list: Vec<ActivityInfo> = activities
                                                                            .into_iter()
                                                                            .filter(|a| {
                                                                                // Filter extended thinking
                                                                                if a.activity_type == crate::parser::ActivityType::Thinking {
                                                                                    a.content.len() < 500
                                                                                } else {
                                                                                    true
                                                                                }
                                                                            })
                                                                            .take(limit_val)
                                                                            .map(|a| {
                                                                                // Convert ActivityType to snake_case for mobile
                                                                                let activity_type_str = match a.activity_type {
                                                                                    crate::parser::ActivityType::Thinking => "thinking",
                                                                                    crate::parser::ActivityType::ToolStart => "tool_start",
                                                                                    crate::parser::ActivityType::ToolResult => "tool_result",
                                                                                    crate::parser::ActivityType::Text => "text",
                                                                                    crate::parser::ActivityType::UserPrompt => "user_prompt",
                                                                                    crate::parser::ActivityType::FileWrite => "file_write",
                                                                                    crate::parser::ActivityType::FileRead => "file_read",
                                                                                    crate::parser::ActivityType::BashCommand => "bash_command",
                                                                                    crate::parser::ActivityType::CodeDiff => "code_diff",
                                                                                    crate::parser::ActivityType::Progress => "progress",
                                                                                    crate::parser::ActivityType::Summary => "summary",
                                                                                };
                                                                                ActivityInfo {
                                                                                    activity_type: activity_type_str.to_string(),
                                                                                    content: a.content,
                                                                                    tool_name: a.tool_name,
                                                                                    tool_params: a.tool_params,
                                                                                    file_path: a.file_path,
                                                                                    is_streaming: a.is_streaming,
                                                                                    timestamp: a.timestamp,
                                                                                    uuid: a.uuid,
                                                                                    summary: a.summary, // ISSUE #11
                                                                                }
                                                                            })
                                                                            .collect();
                                                                        ServerMessage::Activities {
                                                                            session_id: session_id.clone(),
                                                                            activities: activity_list,
                                                                        }
                                                                    }
                                                                    Err(_) => ServerMessage::Activities {
                                                                        session_id: session_id.clone(),
                                                                        activities: Vec::new(),
                                                                    }
                                                                }
                                                            } else {
                                                                ServerMessage::Activities {
                                                                    session_id: session_id.clone(),
                                                                    activities: Vec::new(),
                                                                }
                                                            }
                                                        } else {
                                                            ServerMessage::Activities {
                                                                session_id: session_id.clone(),
                                                                activities: Vec::new(),
                                                            }
                                                        }
                                                    } else {
                                                        ServerMessage::Activities {
                                                            session_id: session_id.clone(),
                                                            activities: Vec::new(),
                                                        }
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::ListDirectory { path } => {
                                                    let target_path = path.clone().unwrap_or_else(|| {
                                                        std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
                                                    });
                                                    let msg = match std::fs::read_dir(&target_path) {
                                                        Ok(entries) => {
                                                            let mut dir_entries: Vec<DirectoryEntry> = entries
                                                                .filter_map(|e| e.ok())
                                                                .filter_map(|entry| {
                                                                    let name = entry.file_name().to_string_lossy().to_string();
                                                                    if name.starts_with('.') {
                                                                        return None;
                                                                    }
                                                                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                                                    Some(DirectoryEntry { name, is_dir })
                                                                })
                                                                .collect();
                                                            dir_entries.sort_by(|a, b| {
                                                                match (a.is_dir, b.is_dir) {
                                                                    (true, false) => std::cmp::Ordering::Less,
                                                                    (false, true) => std::cmp::Ordering::Greater,
                                                                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                                                                }
                                                            });
                                                            ServerMessage::DirectoryListing {
                                                                path: target_path,
                                                                entries: dir_entries,
                                                            }
                                                        }
                                                        Err(e) => ServerMessage::Error {
                                                            code: "fs_error".to_string(),
                                                            message: e.to_string(),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::UploadFile { filename, data, mime_type: _ } => {
                                                    let msg = match base64::engine::general_purpose::STANDARD.decode(data) {
                                                        Ok(decoded) => {
                                                            let upload_dir = std::env::temp_dir().join("mobilecli_uploads");
                                                            if std::fs::create_dir_all(&upload_dir).is_err() {
                                                                ServerMessage::UploadError {
                                                                    message: "Failed to create upload directory".to_string(),
                                                                }
                                                            } else {
                                                                let timestamp = std::time::SystemTime::now()
                                                                    .duration_since(std::time::UNIX_EPOCH)
                                                                    .unwrap_or_default()
                                                                    .as_millis();
                                                                let safe_filename: String = filename
                                                                    .chars()
                                                                    .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
                                                                    .collect();
                                                                let unique_filename = format!("{}_{}", timestamp, safe_filename);
                                                                let file_path = upload_dir.join(&unique_filename);
                                                                match std::fs::write(&file_path, decoded) {
                                                                    Ok(_) => ServerMessage::FileUploaded {
                                                                        path: file_path.to_string_lossy().to_string(),
                                                                        filename: unique_filename,
                                                                    },
                                                                    Err(e) => ServerMessage::UploadError {
                                                                        message: format!("Failed to write file: {}", e),
                                                                    },
                                                                }
                                                            }
                                                        }
                                                        Err(e) => ServerMessage::UploadError {
                                                            message: format!("Failed to decode base64: {}", e),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::Subscribe { .. } | ClientMessage::Unsubscribe { .. } => {
                                                    // Subscriptions handled via broadcast, just acknowledge
                                                    let msg = ServerMessage::Welcome {
                                                        server_version: "0.1.0".to_string(),
                                                        authenticated: true,
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::RenameSession { session_id, new_name } => {
                                                    let msg = match db_clone.rename_session(session_id, new_name) {
                                                        Ok(_) => {
                                                            // Emit event for other listeners
                                                            let _ = app_clone.emit(
                                                                "session-renamed",
                                                                serde_json::json!({
                                                                    "sessionId": session_id,
                                                                    "newName": new_name.clone(),
                                                                }),
                                                            );
                                                            ServerMessage::SessionRenamed { session_id: session_id.clone(), new_name: new_name.clone() }
                                                        }
                                                        Err(e) => ServerMessage::Error {
                                                            code: "db_error".to_string(),
                                                            message: e.to_string(),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::DeleteSession { session_id } => {
                                                    let msg = match db_clone.delete_session(session_id) {
                                                        Ok(_) => {
                                                            // Emit event for other listeners
                                                            let _ = app_clone.emit(
                                                                "session-deleted",
                                                                serde_json::json!({ "sessionId": session_id }),
                                                            );
                                                            ServerMessage::SessionDeleted { session_id: session_id.clone() }
                                                        }
                                                        Err(e) => ServerMessage::Error {
                                                            code: "db_error".to_string(),
                                                            message: e.to_string(),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::SyncInputState { session_id, text, cursor_position, sender_id } => {
                                                    // Emit input state event for broadcasting to other clients
                                                    // Include sender_id and timestamp for echo prevention
                                                    let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                                                    let _ = app_clone.emit(
                                                        "input-state",
                                                        serde_json::json!({
                                                            "sessionId": session_id,
                                                            "text": text,
                                                            "cursorPosition": cursor_position,
                                                            "senderId": sender_id,
                                                            "timestamp": timestamp,
                                                        }),
                                                    );
                                                    // Send acknowledgment back
                                                    let msg = ServerMessage::InputState {
                                                        session_id: session_id.clone(),
                                                        text: text.clone(),
                                                        cursor_position: *cursor_position,
                                                        sender_id: sender_id.clone(),
                                                        timestamp: Some(timestamp),
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::CreateDirectory { path } => {
                                                    // Create directory on desktop filesystem
                                                    let msg = match std::fs::create_dir_all(path) {
                                                        Ok(_) => ServerMessage::DirectoryCreated {
                                                            path: path.clone(),
                                                            success: true,
                                                        },
                                                        Err(e) => ServerMessage::Error {
                                                            code: "fs_error".to_string(),
                                                            message: format!("Failed to create directory: {}", e),
                                                        },
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::Ping => {
                                                    // Respond to heartbeat ping from mobile
                                                    let msg = ServerMessage::Pong;
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::RegisterPushToken { token, token_type, platform } => {
                                                    tracing::info!(
                                                        "Registering push token via relay: type={}, platform={}, token={}...",
                                                        token_type,
                                                        platform,
                                                        &token[..token.len().min(20)]
                                                    );

                                                    // Store the token using the same global storage as local WS
                                                    {
                                                        let mut tokens = crate::ws::PUSH_TOKENS.write().await;
                                                        tokens.retain(|t| t.token != *token);
                                                        tokens.push(crate::ws::PushToken {
                                                            token: token.clone(),
                                                            token_type: token_type.clone(),
                                                            platform: platform.clone(),
                                                            registered_at: std::time::Instant::now(),
                                                        });
                                                        tracing::info!("Push tokens stored: {} total", tokens.len());
                                                    }

                                                    // Send acknowledgment back to mobile
                                                    let msg = ServerMessage::PushTokenRegistered {
                                                        token_type: token_type.clone(),
                                                        platform: platform.clone(),
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                                ClientMessage::PtyResize { session_id, cols, rows } => {
                                                    tracing::info!(
                                                        "PTY resize via relay: session={}, cols={}, rows={}",
                                                        session_id, cols, rows
                                                    );

                                                    // Emit event for PTY module to handle the resize
                                                    let _ = app_clone.emit(
                                                        "pty-resize",
                                                        serde_json::json!({
                                                            "sessionId": session_id,
                                                            "cols": cols,
                                                            "rows": rows,
                                                        }),
                                                    );

                                                    // Send acknowledgment back to mobile
                                                    let msg = ServerMessage::PtyResized {
                                                        session_id: session_id.clone(),
                                                        cols: *cols,
                                                        rows: *rows,
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        if let Ok(encrypted) = encrypt_message(&key_response, &json) {
                                                            let _ = tx_response.send(encrypted);
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            tracing::warn!("Failed to parse relay message as ClientMessage");
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to decrypt relay message: {}", e);
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            tracing::info!("Relay connection closed");
                            // Update status to disconnected
                            {
                                let mut status = state_clone.status.write().await;
                                *status = RelayStatus::Disconnected;
                            }
                            let _ = app_clone.emit("relay-status", RelayStatus::Disconnected);
                            let _ = app_clone.emit("relay-disconnected", ());
                            break;
                        }
                        Some(Err(e)) => {
                            tracing::error!("Relay WebSocket error: {}", e);
                            // Update status to disconnected
                            {
                                let mut status = state_clone.status.write().await;
                                *status = RelayStatus::Disconnected;
                            }
                            let _ = app_clone.emit("relay-status", RelayStatus::Disconnected);
                            let _ = app_clone.emit("relay-error", e.to_string());
                            break;
                        }
                        _ => {}
                    }
                }

                // Messages to send to relay
                msg = rx.recv() => {
                    match msg {
                        Some(encrypted) => {
                            if ws_sender.send(Message::Text(encrypted)).await.is_err() {
                                tracing::error!("Failed to send to relay");
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        // Clean up
        let mut conn = state_clone.connection.write().await;
        *conn = None;
    });

    Ok(RelayQrData {
        url: get_relay_url(Some(&app)),
        room_code,
        key: key_base64,
        connected: false,
    })
}

/// Decrypt a message using the cipher
fn decrypt_message(cipher: &XSalsa20Poly1305, encrypted: &str) -> Result<String, String> {
    // Base64 decode
    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    if combined.len() < 24 {
        return Err("Ciphertext too short".to_string());
    }

    // Extract nonce and ciphertext
    let nonce = crypto_secretbox::Nonce::from_slice(&combined[..24]);
    let ciphertext = &combined[24..];

    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 decode failed: {}", e))
}

/// Encrypt a message using the key
fn encrypt_message(key: &EncryptionKey, plaintext: &str) -> Result<String, String> {
    let cipher = XSalsa20Poly1305::new(key.into());

    // Generate random nonce (24 bytes for XSalsa20)
    let mut nonce_bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = crypto_secretbox::Nonce::from_slice(&nonce_bytes);

    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Prepend nonce to ciphertext and base64 encode
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);

    Ok(BASE64.encode(combined))
}

/// Send a message through the relay (encrypted)
#[allow(dead_code)]
pub async fn send_relay_message(state: Arc<RelayState>, message: &str) -> Result<(), String> {
    let conn = state.connection.read().await;

    if let Some(connection) = conn.as_ref() {
        if let Some(sender) = &connection.sender {
            let encrypted = connection.encrypt(message)?;
            sender
                .send(encrypted)
                .map_err(|e| format!("Failed to send: {}", e))?;
            Ok(())
        } else {
            Err("Relay not connected".to_string())
        }
    } else {
        Err("No relay connection".to_string())
    }
}

/// Get current relay status
pub async fn get_relay_status(state: Arc<RelayState>) -> Option<RelayQrData> {
    let conn = state.connection.read().await;

    conn.as_ref().map(|c| RelayQrData {
        url: get_relay_url(None), // Uses default URL since we don't have AppHandle here
        room_code: c.room_code.clone().unwrap_or_default(),
        key: BASE64.encode(c.key),
        connected: c.client_connected,
    })
}

/// Stop relay connection
pub async fn stop_relay(state: Arc<RelayState>) {
    let mut conn = state.connection.write().await;
    *conn = None;
    tracing::info!("Relay connection stopped");
}
