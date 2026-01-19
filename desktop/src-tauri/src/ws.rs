// WebSocket server module - Handles mobile client connections

use crate::codex;
use crate::db::{CliType, Database};
use crate::gemini;
use crate::jsonl;
use crate::parser::ActivityType;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

pub const WS_PORT: u16 = 9847;

// Connection security limits
const MAX_CONNECTIONS_PER_IP: usize = 5;
const MAX_TOTAL_CONNECTIONS: usize = 50;

type Tx = mpsc::UnboundedSender<Message>;
type PeerMap = Arc<RwLock<HashMap<SocketAddr, Tx>>>;

/// Push notification token storage
#[derive(Debug, Clone)]
pub struct PushToken {
    pub token: String,
    pub token_type: String, // "expo", "apns", or "fcm"
    pub platform: String,   // "ios" or "android"
    pub registered_at: std::time::Instant,
}

/// Global push token storage - stores tokens from all connected mobile clients
pub static PUSH_TOKENS: std::sync::LazyLock<RwLock<Vec<PushToken>>> =
    std::sync::LazyLock::new(|| RwLock::new(Vec::new()));

/// Send push notifications to all registered mobile clients
/// Uses Expo Push Service for expo tokens
pub async fn send_push_notifications(
    title: &str,
    body: &str,
    session_id: &str,
    notification_type: &str,
) {
    let tokens = PUSH_TOKENS.read().await;
    if tokens.is_empty() {
        tracing::debug!("No push tokens registered, skipping push notification");
        return;
    }

    tracing::info!(
        "Sending push notification to {} devices: {} - {}",
        tokens.len(),
        title,
        body
    );

    // Build notification payloads for Expo Push Service
    let mut expo_messages: Vec<serde_json::Value> = Vec::new();

    for token in tokens.iter() {
        if token.token_type == "expo" {
            // Expo Push Token format
            expo_messages.push(serde_json::json!({
                "to": token.token,
                "title": title,
                "body": body,
                "sound": "default",
                "badge": 1,
                "data": {
                    "sessionId": session_id,
                    "type": notification_type,
                },
                // iOS-specific
                "priority": "high",
                "_contentAvailable": true,
            }));
        }
        // TODO: Add native APNs support if needed
    }

    if expo_messages.is_empty() {
        tracing::debug!("No expo tokens found, skipping Expo Push Service");
        return;
    }

    // Send to Expo Push Service
    let client = reqwest::Client::new();
    match client
        .post("https://exp.host/--/api/v2/push/send")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&expo_messages)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                tracing::info!("Push notifications sent successfully");
                if let Ok(text) = response.text().await {
                    tracing::debug!("Expo response: {}", text);
                }
            } else {
                tracing::error!(
                    "Failed to send push notifications: HTTP {}",
                    response.status()
                );
                if let Ok(text) = response.text().await {
                    tracing::error!("Expo error response: {}", text);
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to send push notifications: {}", e);
        }
    }
}

/// Recent session events queue - replays important events to new subscribers
/// Solves the timing issue where session events fire before mobile connects
#[derive(Clone)]
struct RecentSessionEvent {
    message: ServerMessage,
    timestamp: std::time::Instant,
}

type RecentEventsQueue = Arc<RwLock<Vec<RecentSessionEvent>>>;

/// Queue lifetime - events older than this are cleaned up
const EVENT_QUEUE_TTL_SECS: u64 = 5;

/// Check if a new connection should be accepted based on rate limits
fn check_connection_limits(
    peers: &HashMap<SocketAddr, Tx>,
    new_addr: &SocketAddr,
) -> Result<(), String> {
    // Check total connections
    if peers.len() >= MAX_TOTAL_CONNECTIONS {
        return Err(format!(
            "Max connections ({}) reached",
            MAX_TOTAL_CONNECTIONS
        ));
    }

    // Check connections per IP
    let ip = new_addr.ip();
    let connections_from_ip = peers.keys().filter(|addr| addr.ip() == ip).count();
    if connections_from_ip >= MAX_CONNECTIONS_PER_IP {
        return Err(format!(
            "Max connections per IP ({}) reached for {}",
            MAX_CONNECTIONS_PER_IP, ip
        ));
    }

    Ok(())
}

/// Validate a path to prevent directory traversal attacks.
/// Returns the canonicalized path if safe, or an error message.
fn validate_path(requested_path: &str) -> Result<std::path::PathBuf, String> {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let home = std::path::PathBuf::from(&home_dir);

    // Convert to PathBuf and canonicalize
    let requested = std::path::PathBuf::from(requested_path);

    // Try to canonicalize (resolves .. and symlinks)
    let canonical = match requested.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Path doesn't exist yet, check if parent is valid
            if let Some(parent) = requested.parent() {
                if let Ok(canonical_parent) = parent.canonicalize() {
                    // For new paths, construct from canonical parent
                    if canonical_parent.starts_with(&home) {
                        return Ok(requested);
                    }
                }
            }
            return Err(format!("Invalid path: {}", requested_path));
        }
    };

    // Ensure path is within home directory (or common safe locations)
    let allowed_prefixes = [&home, &std::path::PathBuf::from("/tmp")];

    for prefix in allowed_prefixes.iter() {
        if canonical.starts_with(prefix) {
            return Ok(canonical);
        }
    }

    Err(format!("Access denied: path outside allowed directories"))
}

/// File upload security constants
const ALLOWED_UPLOAD_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", // Images
    "pdf", "txt", "md", "json", // Documents
    "log",  // Logs
];
const MAX_UPLOAD_SIZE: usize = 10 * 1024 * 1024; // 10MB

/// Validate file uploads to prevent malicious file types and excessive sizes
fn validate_upload(filename: &str, size: usize) -> Result<(), String> {
    // Check file size
    if size > MAX_UPLOAD_SIZE {
        return Err(format!(
            "File too large: {} bytes (max {} bytes)",
            size, MAX_UPLOAD_SIZE
        ));
    }

    // Extract and validate extension
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext.is_empty() {
        return Err("File must have an extension".to_string());
    }

    if !ALLOWED_UPLOAD_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "File extension '{}' not allowed. Allowed: {:?}",
            ext, ALLOWED_UPLOAD_EXTENSIONS
        ));
    }

    // Prevent path traversal in filename
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err("Invalid characters in filename".to_string());
    }

    Ok(())
}

/// Try to bind to an address with retry logic.
/// This helps when the app is restarted quickly and the OS hasn't released the port yet.
async fn bind_with_retry(
    addr: &str,
    max_retries: u32,
    initial_delay_ms: u64,
) -> Result<TcpListener, Box<dyn std::error::Error + Send + Sync>> {
    let mut delay_ms = initial_delay_ms;

    for attempt in 0..max_retries {
        // Use socket2 to set SO_REUSEADDR before binding
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;

        // Set SO_REUSEADDR to allow binding even if port is in TIME_WAIT state
        socket.set_reuse_address(true)?;

        // On Unix, also try SO_REUSEPORT for even faster rebinding
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // set_reuse_port is available on Linux - ignore errors
            let _ = socket.set_reuse_port(true);
        }

        // Set non-blocking for tokio
        socket.set_nonblocking(true)?;

        // Parse and bind the address
        let sock_addr: std::net::SocketAddr = addr.parse()?;
        match socket.bind(&sock_addr.into()) {
            Ok(_) => {
                socket.listen(128)?;
                let std_listener: std::net::TcpListener = socket.into();
                return Ok(TcpListener::from_std(std_listener)?);
            }
            Err(e) if attempt < max_retries - 1 => {
                tracing::warn!(
                    "Failed to bind to {} (attempt {}/{}): {}. Retrying in {}ms...",
                    addr,
                    attempt + 1,
                    max_retries,
                    e,
                    delay_ms
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                delay_ms *= 2; // Exponential backoff
            }
            Err(e) => {
                tracing::error!(
                    "Failed to bind to {} after {} attempts: {}",
                    addr,
                    max_retries,
                    e
                );
                return Err(e.into());
            }
        }
    }

    Err("Failed to bind after all retries".into())
}

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
    CreateSession {
        project_path: String,
        name: Option<String>,
        cli_type: Option<String>,
        /// Claude: Open with --dangerously-skip-permissions flag
        #[serde(default)]
        claude_skip_permissions: Option<bool>,
        /// Codex: Approval policy (untrusted, on-failure, on-request, never)
        codex_approval_policy: Option<String>,
    },
    CloseSession {
        session_id: String,
    },
    ResumeSession {
        session_id: String,
        /// Optional: skip permission prompts on resume (mobile setting)
        claude_skip_permissions: Option<bool>,
    },
    GetSessions,
    GetMessages {
        session_id: String,
        limit: Option<i64>,
    },
    /// Get activities (including tool calls like Bash, Read, etc.) for a session
    GetActivities {
        session_id: String,
        limit: Option<i64>,
    },
    ListDirectory {
        path: Option<String>,
    },
    CreateDirectory {
        path: String,
    },
    UploadFile {
        filename: String,
        data: String,
        mime_type: String,
    },
    RenameSession {
        session_id: String,
        new_name: String,
    },
    DeleteSession {
        session_id: String,
    },
    /// Sync input state - when user types on mobile, sync to other clients
    SyncInputState {
        session_id: String,
        text: String,
        cursor_position: Option<usize>,
        /// Sender ID to identify the source device (for echo prevention)
        #[serde(default)]
        sender_id: Option<String>,
    },
    /// Heartbeat ping - client sends to check connection health
    Ping,
    /// Register push notification token from mobile client
    RegisterPushToken {
        /// The push notification token (Expo or native APNs/FCM)
        token: String,
        /// Token type: "expo" for Expo Push Token, "apns" for native iOS, "fcm" for native Android
        token_type: String,
        /// Platform: "ios" or "android"
        platform: String,
    },
}

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
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    SessionCreated {
        session: SessionInfo,
    },
    SessionResumed {
        session: SessionInfo,
    },
    SessionClosed {
        session_id: String,
    },
    SessionRenamed {
        session_id: String,
        new_name: String,
    },
    SessionDeleted {
        session_id: String,
    },
    Messages {
        session_id: String,
        messages: Vec<MessageInfo>,
    },
    /// Activities list for session history (includes tool calls like Bash, Read, etc.)
    Activities {
        session_id: String,
        activities: Vec<ActivityInfo>,
    },
    NewMessage {
        session_id: String,
        role: String,
        content: String,
        tool_name: Option<String>,
        is_complete: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        client_msg_id: Option<String>,
    },
    PtyOutput {
        session_id: String,
        output: String,
    },
    /// Raw PTY bytes (base64 encoded) for xterm.js rendering
    /// This is the raw terminal output without ANSI stripping - used by mobile xterm.js WebView
    PtyBytes {
        session_id: String,
        /// Base64 encoded raw bytes from PTY
        data: String,
    },
    WaitingForInput {
        session_id: String,
        timestamp: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait_type: Option<String>,
    },
    /// Tool approval was accepted/rejected - dismiss mobile modal
    WaitingCleared {
        session_id: String,
        timestamp: String,
        /// The response that was sent (e.g., "1", "2", "3", "y", "n")
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<String>,
    },
    DirectoryListing {
        path: String,
        entries: Vec<DirectoryEntry>,
    },
    /// Directory creation response
    DirectoryCreated {
        path: String,
        success: bool,
    },
    /// Activity stream for showing full CLI flow
    Activity {
        session_id: String,
        activity_type: ActivityType,
        content: String,
        tool_name: Option<String>,
        tool_params: Option<String>,
        file_path: Option<String>,
        is_streaming: bool,
        timestamp: String,
        /// UUID from Claude's JSONL (tool_use_id) - used for matching streaming to final
        #[serde(skip_serializing_if = "Option::is_none")]
        uuid: Option<String>,
        /// Source of this activity: "pty" or "jsonl"
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// File upload response
    FileUploaded {
        path: String,
        filename: String,
    },
    /// Upload error
    UploadError {
        message: String,
    },
    /// Input state sync - broadcast current input field state to mobile clients
    InputState {
        session_id: String,
        /// Current text in the input field (not yet sent)
        text: String,
        /// Cursor position in the input field
        cursor_position: Option<usize>,
        /// Sender ID to identify the source device (for echo prevention)
        #[serde(skip_serializing_if = "Option::is_none")]
        sender_id: Option<String>,
        /// Timestamp when the input was typed
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// Heartbeat pong - server responds to ping to confirm connection is alive
    Pong,
    /// Push token registered successfully
    PushTokenRegistered {
        token_type: String,
        platform: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DirectoryEntry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub project_path: String,
    pub created_at: String,
    pub last_active_at: String,
    pub status: String,
    pub cli_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageInfo {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_result: Option<String>,
    pub timestamp: String,
}

/// Activity info for GetActivities response - includes tool calls, results, etc.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityInfo {
    pub activity_type: String, // "tool_start", "tool_result", "text", "user_prompt", "thinking"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub is_streaming: bool,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// ISSUE #11: Clean tool summary for display in tool approval modal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

pub async fn start_server(
    app: AppHandle,
    db: Arc<Database>,
    ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("0.0.0.0:{}", WS_PORT);

    // Try to bind with retry logic - helps when restarting quickly
    let listener = bind_with_retry(&addr, 5, 500).await?;
    tracing::info!("WebSocket server listening on {}", addr);

    // Signal that the server is ready
    if let Some(tx) = ready_tx {
        let _ = tx.send(());
    }

    // Emit event so frontend knows WS is ready
    let _ = app.emit("ws-server-ready", serde_json::json!({ "port": WS_PORT }));

    let peers: PeerMap = Arc::new(RwLock::new(HashMap::new()));

    // Recent events queue for session events - replays to new subscribers
    let recent_events: RecentEventsQueue = Arc::new(RwLock::new(Vec::new()));

    // Channel for broadcasting events to all clients
    let (broadcast_tx, _) = broadcast::channel::<ServerMessage>(100);

    // Listen for Tauri events and broadcast to WebSocket clients
    let _peers_clone = peers.clone();
    let broadcast_tx_clone = broadcast_tx.clone();
    app.listen("pty-output", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            // Send raw output (with ANSI codes) for terminal rendering
            // Fall back to cleaned output if raw not available
            let output = payload["raw"]
                .as_str()
                .or_else(|| payload["output"].as_str())
                .unwrap_or("");
            let msg = ServerMessage::PtyOutput {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                output: output.to_string(),
            };
            let _ = broadcast_tx_clone.send(msg);
        }
    });

    // Listen for raw PTY bytes (base64 encoded) for xterm.js rendering on mobile
    let broadcast_tx_pty_bytes = broadcast_tx.clone();
    app.listen("pty-bytes", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::PtyBytes {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                data: payload["data"].as_str().unwrap_or("").to_string(),
            };
            let _ = broadcast_tx_pty_bytes.send(msg);
        }
    });

    let _peers_clone2 = peers.clone();
    let broadcast_tx_clone2 = broadcast_tx.clone();
    app.listen("new-message", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
            let role = payload["role"].as_str().unwrap_or("").to_string();
            let content = payload["content"].as_str().unwrap_or("").to_string();
            tracing::info!(
                "[ws.rs] Broadcasting new-message: session={}, role={}, content={}",
                session_id,
                role,
                content
            );
            let msg = ServerMessage::NewMessage {
                session_id,
                role,
                content,
                tool_name: payload["toolName"].as_str().map(String::from),
                is_complete: payload["isComplete"].as_bool(),
                client_msg_id: payload["clientMsgId"].as_str().map(String::from),
            };
            let _ = broadcast_tx_clone2.send(msg);
        }
    });

    // Listen for waiting-for-input events (for mobile push notifications)
    let broadcast_tx_clone3 = broadcast_tx.clone();
    app.listen("waiting-for-input", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
            let timestamp = payload["timestamp"].as_str().unwrap_or("").to_string();
            let prompt_content = payload["promptContent"].as_str().map(|s| s.to_string());

            let wait_type = payload["waitType"].as_str().map(|s| s.to_string());
            let msg = ServerMessage::WaitingForInput {
                session_id: session_id.clone(),
                timestamp,
                prompt_content: prompt_content.clone(),
                wait_type: wait_type.clone(),
            };
            tracing::debug!("Broadcasting waiting-for-input event with prompt content");
            let _ = broadcast_tx_clone3.send(msg);

            // Send push notification to mobile devices
            // Determine notification content based on prompt
            let (title, body) = if let Some(ref wait_type) = wait_type {
                match wait_type.as_str() {
                    "tool_approval" => (
                        "Tool Approval Needed".to_string(),
                        "Claude needs permission to proceed".to_string(),
                    ),
                    "plan_approval" => (
                        "Plan Approval Needed".to_string(),
                        "Claude has a plan ready for review".to_string(),
                    ),
                    "clarifying_question" => (
                        "Claude has a question".to_string(),
                        prompt_content
                            .as_ref()
                            .map(|content| content.chars().take(100).collect::<String>())
                            .unwrap_or_else(|| "Tap to respond".to_string()),
                    ),
                    _ => (
                        "Claude is ready".to_string(),
                        "Waiting for your input".to_string(),
                    ),
                }
            } else if let Some(ref content) = prompt_content {
                if content.contains("approval") || content.contains("permission") || content.contains("Allow") {
                    ("Tool Approval Needed".to_string(), "Claude needs permission to proceed".to_string())
                } else if content.contains("?") {
                    ("Claude has a question".to_string(), content.chars().take(100).collect::<String>())
                } else {
                    ("Claude is ready".to_string(), "Waiting for your input".to_string())
                }
            } else {
                ("Claude is ready".to_string(), "Waiting for your input".to_string())
            };

            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                send_push_notifications(&title, &body, &session_id_clone, "waiting_for_input").await;
            });
        }
    });

    // Listen for waiting-cleared events (tool approval accepted/rejected - dismiss mobile modal)
    let broadcast_tx_clone3a = broadcast_tx.clone();
    app.listen("waiting-cleared", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::WaitingCleared {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                timestamp: payload["timestamp"].as_str().unwrap_or("").to_string(),
                response: payload["response"].as_str().map(|s| s.to_string()),
            };
            tracing::info!("Broadcasting waiting-cleared event to mobile clients");
            let _ = broadcast_tx_clone3a.send(msg);
        }
    });

    // Listen for session-created events (to sync with mobile)
    // Also queue for replay to late-connecting subscribers
    let broadcast_tx_clone4 = broadcast_tx.clone();
    let recent_events_clone4 = recent_events.clone();
    app.listen("session-created", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionCreated {
                session: SessionInfo {
                    id: payload["id"].as_str().unwrap_or("").to_string(),
                    name: payload["name"].as_str().unwrap_or("").to_string(),
                    project_path: payload["projectPath"].as_str().unwrap_or("").to_string(),
                    created_at: payload["createdAt"].as_str().unwrap_or("").to_string(),
                    last_active_at: payload["lastActiveAt"].as_str().unwrap_or("").to_string(),
                    status: payload["status"].as_str().unwrap_or("active").to_string(),
                    cli_type: payload["cliType"].as_str().unwrap_or("claude").to_string(),
                },
            };
            tracing::info!("Broadcasting session-created event to mobile clients");
            let _ = broadcast_tx_clone4.send(msg.clone());

            // Queue for replay to late-connecting subscribers
            if let Ok(mut queue) = recent_events_clone4.try_write() {
                // Clean up old events
                let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(EVENT_QUEUE_TTL_SECS);
                queue.retain(|e| e.timestamp > cutoff);
                // Add new event
                queue.push(RecentSessionEvent {
                    message: msg,
                    timestamp: std::time::Instant::now(),
                });
            }
        }
    });

    // Listen for session-resumed events (to sync desktop when mobile resumes)
    // Also queue for replay to late-connecting subscribers
    let broadcast_tx_clone5 = broadcast_tx.clone();
    let recent_events_clone5 = recent_events.clone();
    app.listen("session-resumed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionResumed {
                session: SessionInfo {
                    id: payload["id"].as_str().unwrap_or("").to_string(),
                    name: payload["name"].as_str().unwrap_or("").to_string(),
                    project_path: payload["projectPath"].as_str().unwrap_or("").to_string(),
                    created_at: payload["createdAt"].as_str().unwrap_or("").to_string(),
                    last_active_at: payload["lastActiveAt"].as_str().unwrap_or("").to_string(),
                    status: payload["status"].as_str().unwrap_or("active").to_string(),
                    cli_type: payload["cliType"].as_str().unwrap_or("claude").to_string(),
                },
            };
            tracing::info!("Broadcasting session-resumed event to all clients");
            let _ = broadcast_tx_clone5.send(msg.clone());

            // Queue for replay to late-connecting subscribers
            if let Ok(mut queue) = recent_events_clone5.try_write() {
                let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(EVENT_QUEUE_TTL_SECS);
                queue.retain(|e| e.timestamp > cutoff);
                queue.push(RecentSessionEvent {
                    message: msg,
                    timestamp: std::time::Instant::now(),
                });
            }
        }
    });

    // Listen for session-closed events (to sync all clients when session is closed)
    // Also queue for replay to late-connecting subscribers
    let broadcast_tx_clone6 = broadcast_tx.clone();
    let recent_events_clone6 = recent_events.clone();
    app.listen("session-closed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionClosed {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
            };
            tracing::info!("Broadcasting session-closed event to all clients");
            let _ = broadcast_tx_clone6.send(msg.clone());

            // Queue for replay to late-connecting subscribers
            if let Ok(mut queue) = recent_events_clone6.try_write() {
                let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(EVENT_QUEUE_TTL_SECS);
                queue.retain(|e| e.timestamp > cutoff);
                queue.push(RecentSessionEvent {
                    message: msg,
                    timestamp: std::time::Instant::now(),
                });
            }
        }
    });

    // Listen for session-renamed events (to sync all clients when session is renamed)
    let broadcast_tx_clone6a = broadcast_tx.clone();
    app.listen("session-renamed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionRenamed {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                new_name: payload["newName"].as_str().unwrap_or("").to_string(),
            };
            tracing::info!("Broadcasting session-renamed event to all clients");
            let _ = broadcast_tx_clone6a.send(msg);
        }
    });

    // Listen for session-deleted events (to sync all clients when session is deleted)
    let broadcast_tx_clone6b = broadcast_tx.clone();
    app.listen("session-deleted", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::SessionDeleted {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
            };
            tracing::info!("Broadcasting session-deleted event to all clients");
            let _ = broadcast_tx_clone6b.send(msg);
        }
    });

    // Listen for input-error events (when input fails to send to PTY)
    let broadcast_tx_clone7 = broadcast_tx.clone();
    app.listen("input-error", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let msg = ServerMessage::Error {
                code: "input_failed".to_string(),
                message: payload["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            };
            tracing::info!("Broadcasting input-error event to all clients");
            let _ = broadcast_tx_clone7.send(msg);
        }
    });

    // Listen for PTY activity events (streaming, may be noisy)
    // NOTE: After JSONL redesign, PTY activities are mostly for streaming visibility.
    // JSONL activities are the authoritative source for Claude sessions.
    let broadcast_tx_clone8 = broadcast_tx.clone();
    app.listen("activity", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            // Parse activity type from string
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

            // DEBUG: Log thinking activities
            if activity_type_str == "thinking" {
                tracing::info!(
                    "[WS] Broadcasting THINKING activity: {:?}",
                    payload["content"].as_str().unwrap_or("")
                );
            }

            let msg = ServerMessage::Activity {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                activity_type,
                content: payload["content"].as_str().unwrap_or("").to_string(),
                tool_name: payload["toolName"].as_str().map(String::from),
                tool_params: payload["toolParams"].as_str().map(String::from),
                file_path: payload["filePath"].as_str().map(String::from),
                is_streaming: payload["isStreaming"].as_bool().unwrap_or(false),
                timestamp: payload["timestamp"].as_str().unwrap_or("").to_string(),
                uuid: None, // PTY activities don't have UUIDs
                source: Some("pty".to_string()),
            };
            let _ = broadcast_tx_clone8.send(msg);
        }
    });

    // Listen for JSONL activity events (authoritative, from Claude's native JSONL logs)
    // These are clean, structured activities that should replace PTY-based activities
    let broadcast_tx_clone8b = broadcast_tx.clone();
    app.listen("jsonl-activity", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            // Parse activity type from string
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

            tracing::debug!(
                "Broadcasting JSONL activity: {} ({} chars)",
                activity_type_str,
                payload["content"].as_str().map(|s| s.len()).unwrap_or(0)
            );

            let msg = ServerMessage::Activity {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                activity_type,
                content: payload["content"].as_str().unwrap_or("").to_string(),
                tool_name: payload["toolName"].as_str().map(String::from),
                tool_params: payload["toolParams"].as_str().map(String::from),
                file_path: payload["filePath"].as_str().map(String::from),
                is_streaming: false, // JSONL activities are always complete
                timestamp: payload["timestamp"].as_str().unwrap_or("").to_string(),
                uuid: payload["uuid"].as_str().map(String::from),
                source: Some("jsonl".to_string()),
            };
            let _ = broadcast_tx_clone8b.send(msg);
        }
    });

    // Listen for input-state events (for real-time input field sync)
    let broadcast_tx_clone9 = broadcast_tx.clone();
    app.listen("input-state", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let text = payload["text"].as_str().unwrap_or("").to_string();
            tracing::debug!("Broadcasting input-state: {} chars, sender: {:?}", text.len(), payload["senderId"].as_str());
            let msg = ServerMessage::InputState {
                session_id: payload["sessionId"].as_str().unwrap_or("").to_string(),
                text,
                cursor_position: payload["cursorPosition"].as_u64().map(|v| v as usize),
                sender_id: payload["senderId"].as_str().map(String::from),
                timestamp: payload["timestamp"].as_u64(),
            };
            let _ = broadcast_tx_clone9.send(msg);
        }
    });

    // Accept connections
    while let Ok((stream, addr)) = listener.accept().await {
        // Check connection limits before processing
        {
            let current_peers = peers.read().await;
            if let Err(e) = check_connection_limits(&current_peers, &addr) {
                tracing::warn!("Connection rejected from {}: {}", addr, e);
                drop(stream); // Close the connection
                continue;
            }
        }

        let peers = peers.clone();
        let db = db.clone();
        let app = app.clone();
        let broadcast_rx = broadcast_tx.subscribe();
        let recent_events = recent_events.clone();

        tokio::spawn(async move {
            if let Err(e) =
                handle_connection(stream, addr, peers.clone(), db, app, broadcast_rx, recent_events).await
            {
                tracing::error!("Connection error for {}: {}", addr, e);
            }
            // Remove peer on disconnect
            peers.write().await.remove(&addr);
            tracing::info!("Client disconnected: {}", addr);
        });
    }

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    peers: PeerMap,
    db: Arc<Database>,
    app: AppHandle,
    mut broadcast_rx: broadcast::Receiver<ServerMessage>,
    recent_events: RecentEventsQueue,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = accept_async(stream).await?;
    tracing::info!("New WebSocket connection: {}", addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Channel for sending messages to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    peers.write().await.insert(addr, tx.clone());

    // Send welcome message
    let welcome = ServerMessage::Welcome {
        server_version: "0.1.0".to_string(),
        authenticated: true, // For now, no auth
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&welcome)?))
        .await?;

    // Replay recent session events to this new connection
    // This ensures mobile clients see sessions created just before they connected
    {
        let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(EVENT_QUEUE_TTL_SECS);
        let queue = recent_events.read().await;
        let recent_count = queue.iter().filter(|e| e.timestamp > cutoff).count();
        if recent_count > 0 {
            tracing::info!(
                "Replaying {} recent session events to new client {}",
                recent_count,
                addr
            );
            for event in queue.iter().filter(|e| e.timestamp > cutoff) {
                if let Ok(json) = serde_json::to_string(&event.message) {
                    let _ = ws_sender.send(Message::Text(json)).await;
                }
            }
        }
    }

    // Spawn task to forward messages from channel to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Spawn task to forward broadcast messages
    let tx_clone = tx.clone();
    let broadcast_task = tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                tracing::info!(
                    "[ws.rs] Forwarding broadcast to client: {} chars",
                    json.len()
                );
                let _ = tx_clone.send(Message::Text(json));
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                    let response = handle_client_message(client_msg, &db, &app).await;
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = tx.send(Message::Text(json));
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(data)) => {
                let _ = tx.send(Message::Pong(data));
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Clean up tasks
    send_task.abort();
    broadcast_task.abort();

    Ok(())
}

async fn handle_client_message(
    msg: ClientMessage,
    db: &Database,
    app: &AppHandle,
) -> ServerMessage {
    match msg {
        ClientMessage::Hello { .. } => ServerMessage::Welcome {
            server_version: "0.1.0".to_string(),
            authenticated: true,
        },

        ClientMessage::GetSessions => match db.get_all_sessions() {
            Ok(sessions) => ServerMessage::Sessions {
                sessions: sessions
                    .into_iter()
                    .map(|s| SessionInfo {
                        id: s.id,
                        name: s.name,
                        project_path: s.project_path,
                        created_at: s.created_at,
                        last_active_at: s.last_active_at,
                        status: s.status,
                        cli_type: s.cli_type,
                    })
                    .collect(),
            },
            Err(e) => ServerMessage::Error {
                code: "db_error".to_string(),
                message: e.to_string(),
            },
        },

        ClientMessage::GetMessages { session_id, limit } => {
            // Read from CLI-native session files first (JSONL/JSON)
            // Fall back to DB if CLI files fail

            let limit_val = limit.unwrap_or(100) as usize;

            // Try to get session info for file lookup
            if let Ok(Some(session)) = db.get_session(&session_id) {
                let conversation_id = session.conversation_id.as_deref().unwrap_or(&session_id);

                // Helper to convert activities to MessageInfo
                let convert_activities =
                    |activities: Vec<crate::jsonl::Activity>, sid: &str| -> Vec<MessageInfo> {
                        activities
                            .into_iter()
                            .filter(|a| a.activity_type != ActivityType::Thinking)
                            .take(limit_val)
                            .map(|a| {
                                let role = match a.activity_type {
                                    ActivityType::UserPrompt => "user".to_string(),
                                    _ => "assistant".to_string(),
                                };
                                MessageInfo {
                                    id: a.uuid.unwrap_or_else(|| format!("act_{}", a.timestamp)),
                                    session_id: sid.to_string(),
                                    role,
                                    content: a.content,
                                    tool_name: a.tool_name,
                                    tool_result: None,
                                    timestamp: a.timestamp,
                                }
                            })
                            .collect()
                    };

                match session.cli_type.as_str() {
                    "claude" => {
                        // Claude: JSONL at ~/.claude/projects/{hash}/{session}.jsonl
                        let jsonl_path =
                            jsonl::get_jsonl_path(&session.project_path, conversation_id);
                        if jsonl_path.exists() {
                            match jsonl::read_activities(&session.project_path, conversation_id) {
                                Ok(activities) => {
                                    let messages = convert_activities(activities, &session_id);
                                    return ServerMessage::Messages {
                                        session_id,
                                        messages,
                                    };
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to read Claude JSONL for session {}: {}",
                                        session_id,
                                        e
                                    );
                                }
                            }
                        }
                    }
                    "codex" => {
                        // Codex: JSONL at ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl
                        if let Some(codex_path) = codex::find_session_file(conversation_id)
                            .or_else(|| codex::get_latest_session_file())
                        {
                            match codex::read_codex_file(&codex_path) {
                                Ok(records) => {
                                    let activities: Vec<_> = records
                                        .iter()
                                        .flat_map(codex::record_to_activities)
                                        .map(|a| crate::jsonl::Activity {
                                            activity_type: a.activity_type,
                                            content: a.content,
                                            tool_name: a.tool_name,
                                            tool_params: a.tool_params,
                                            file_path: a.file_path,
                                            is_streaming: a.is_streaming,
                                            timestamp: a.timestamp,
                                            uuid: a.uuid,
                                            summary: None, // Codex doesn't have summary entries
                                        })
                                        .collect();
                                    let messages = convert_activities(activities, &session_id);
                                    return ServerMessage::Messages {
                                        session_id,
                                        messages,
                                    };
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to read Codex JSONL for session {}: {}",
                                        session_id,
                                        e
                                    );
                                }
                            }
                        }
                    }
                    "gemini" => {
                        // Gemini: JSON at ~/.gemini/tmp/{hash}/chats/session-*.json
                        if let Some(gemini_path) =
                            gemini::find_session_file(&session.project_path, conversation_id)
                                .or_else(|| gemini::get_latest_session_file(&session.project_path))
                        {
                            match gemini::read_session_file(&gemini_path) {
                                Ok(session_data) => {
                                    let activities: Vec<_> = session_data
                                        .messages
                                        .iter()
                                        .flat_map(gemini::message_to_activities)
                                        .map(|a| crate::jsonl::Activity {
                                            activity_type: a.activity_type,
                                            content: a.content,
                                            tool_name: a.tool_name,
                                            tool_params: a.tool_params,
                                            file_path: a.file_path,
                                            is_streaming: a.is_streaming,
                                            timestamp: a.timestamp,
                                            uuid: a.uuid,
                                            summary: None, // Gemini doesn't have summary entries
                                        })
                                        .collect();
                                    let messages = convert_activities(activities, &session_id);
                                    return ServerMessage::Messages {
                                        session_id,
                                        messages,
                                    };
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to read Gemini JSON for session {}: {}",
                                        session_id,
                                        e
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        // OpenCode or unknown - fall through to DB
                        tracing::debug!("No native file parser for CLI type: {}", session.cli_type);
                    }
                }
            }

            // Fallback: read from database
            match db.get_messages(&session_id, limit_val as i64) {
                Ok(messages) => ServerMessage::Messages {
                    session_id,
                    messages: messages
                        .into_iter()
                        .map(|m| MessageInfo {
                            id: m.id,
                            session_id: m.session_id,
                            role: m.role,
                            content: m.content,
                            tool_name: m.tool_name,
                            tool_result: m.tool_result,
                            timestamp: m.timestamp,
                        })
                        .collect(),
                },
                Err(e) => ServerMessage::Error {
                    code: "db_error".to_string(),
                    message: e.to_string(),
                },
            }
        }

        ClientMessage::GetActivities { session_id, limit } => {
            // Get activities with proper types (tool_start, tool_result, etc.)
            // This preserves Bash commands, file operations, etc. for display

            let limit_val = limit.unwrap_or(100) as usize;

            // Try to get session info for JSONL lookup
            if let Ok(Some(session)) = db.get_session(&session_id) {
                if session.cli_type == "claude" {
                    if let Some(ref conversation_id) = session.conversation_id {
                        match jsonl::read_activities(&session.project_path, conversation_id) {
                            Ok(activities) => {
                                // Convert JSONL activities to ActivityInfo, preserving types
                                // Filter out extended thinking content but keep streaming indicators
                                let activity_list: Vec<ActivityInfo> = activities
                                    .into_iter()
                                    .filter(|a| {
                                        // Keep all activity types - let mobile decide what to show
                                        // Only filter extended thinking blocks (>500 chars)
                                        if a.activity_type == crate::parser::ActivityType::Thinking
                                        {
                                            a.content.len() < 500
                                        } else {
                                            true
                                        }
                                    })
                                    .take(limit_val)
                                    .map(|a| {
                                        // Convert ActivityType to snake_case string for mobile
                                        let activity_type_str = match a.activity_type {
                                            crate::parser::ActivityType::Thinking => "thinking",
                                            crate::parser::ActivityType::ToolStart => "tool_start",
                                            crate::parser::ActivityType::ToolResult => {
                                                "tool_result"
                                            }
                                            crate::parser::ActivityType::Text => "text",
                                            crate::parser::ActivityType::UserPrompt => {
                                                "user_prompt"
                                            }
                                            crate::parser::ActivityType::FileWrite => "file_write",
                                            crate::parser::ActivityType::FileRead => "file_read",
                                            crate::parser::ActivityType::BashCommand => {
                                                "bash_command"
                                            }
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

                                return ServerMessage::Activities {
                                    session_id,
                                    activities: activity_list,
                                };
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to read JSONL activities for session {}: {}",
                                    session_id,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            // Fallback: return empty activities (non-Claude CLIs or JSONL not found)
            ServerMessage::Activities {
                session_id,
                activities: Vec::new(),
            }
        }

        ClientMessage::SendInput {
            session_id,
            text,
            raw,
            client_msg_id,
        } => {
            let text_bytes: Vec<u8> = text.bytes().collect();
            tracing::info!(
                "WS SendInput: session={}, text={:?}, text_hex={:02x?}, raw={}",
                session_id,
                text,
                text_bytes,
                raw
            );

            // Emit event for PTY module to handle
            let _ = app.emit(
                "send-input",
                serde_json::json!({
                    "sessionId": session_id,
                    "text": text,
                    "raw": raw,
                    "clientMsgId": client_msg_id,
                }),
            );

            // JSONL Redesign: User messages are written to JSONL by Claude when sent to PTY
            // No need to store in DB anymore - JSONL is the source of truth
            if !raw {
                // Broadcast user message to all connected clients (including relay)
                // This ensures mobile sees prompts sent from desktop and vice versa
                let _ = app.emit(
                    "new-message",
                    serde_json::json!({
                        "sessionId": session_id,
                        "role": "user",
                        "content": text,
                        "isComplete": true,
                        "clientMsgId": client_msg_id,
                    }),
                );
            }

            ServerMessage::NewMessage {
                session_id,
                role: "user".to_string(),
                content: text,
                tool_name: None,
                is_complete: Some(true),
                client_msg_id,
            }
        }

        ClientMessage::CreateSession {
            project_path,
            name,
            cli_type,
            claude_skip_permissions,
            codex_approval_policy,
        } => {
            // Parse CLI type (default to Claude)
            let parsed_cli_type = cli_type
                .as_deref()
                .and_then(CliType::from_str)
                .unwrap_or(CliType::ClaudeCode);

            // Generate session name from project path if not provided
            let session_name = name.unwrap_or_else(|| {
                project_path
                    .split('/')
                    .next_back()
                    .unwrap_or("New Session")
                    .to_string()
            });

            match db.create_session(&session_name, &project_path, parsed_cli_type) {
                Ok(session) => {
                    let session_info = SessionInfo {
                        id: session.id.clone(),
                        name: session.name.clone(),
                        project_path: session.project_path.clone(),
                        created_at: session.created_at.clone(),
                        last_active_at: session.last_active_at.clone(),
                        status: session.status.clone(),
                        cli_type: session.cli_type.clone(),
                    };

                    // Emit event to start PTY session with CLI-specific settings
                    let _ = app.emit(
                        "create-session",
                        serde_json::json!({
                            "sessionId": session.id,
                            "projectPath": project_path,
                            "cliType": session.cli_type,
                            "claudeSkipPermissions": claude_skip_permissions.unwrap_or(false),
                            "codexApprovalPolicy": codex_approval_policy,
                        }),
                    );

                    // Emit session-created to broadcast to all clients (including desktop)
                    // The requesting mobile client may receive this twice, but mobile
                    // handles deduplication via content-based checks in addSession
                    let _ = app.emit(
                        "session-created",
                        serde_json::json!({
                            "id": session.id,
                            "name": session.name,
                            "projectPath": session.project_path,
                            "createdAt": session.created_at,
                            "lastActiveAt": session.last_active_at,
                            "status": session.status,
                            "cliType": session.cli_type,
                        }),
                    );

                    ServerMessage::SessionCreated {
                        session: session_info,
                    }
                }
                Err(e) => ServerMessage::Error {
                    code: "db_error".to_string(),
                    message: e.to_string(),
                },
            }
        }

        ClientMessage::CloseSession { session_id } => {
            match db.update_session_status(&session_id, "closed") {
                Ok(_) => {
                    // Emit event to stop the PTY session
                    let _ = app.emit(
                        "close-session",
                        serde_json::json!({ "sessionId": session_id }),
                    );
                    // Broadcast to all clients that this session is closed
                    let _ = app.emit(
                        "session-closed",
                        serde_json::json!({ "sessionId": session_id }),
                    );
                    ServerMessage::SessionClosed {
                        session_id: session_id.clone(),
                    }
                }
                Err(e) => ServerMessage::Error {
                    code: "db_error".to_string(),
                    message: e.to_string(),
                },
            }
        }

        ClientMessage::ResumeSession { session_id, claude_skip_permissions } => {
            // Get session from database to check if it can be resumed
            match db.get_session(&session_id) {
                Ok(Some(session)) => {
                    // Check if session has a conversation_id
                    if session.conversation_id.is_none() {
                        return ServerMessage::Error {
                            code: "no_conversation_id".to_string(),
                            message: "Session has no conversation ID to resume".to_string(),
                        };
                    }

                    // Check if CLI supports resume
                    let cli_type =
                        CliType::from_str(&session.cli_type).unwrap_or(CliType::ClaudeCode);
                    if !cli_type.supports_resume() {
                        return ServerMessage::Error {
                            code: "resume_not_supported".to_string(),
                            message: format!(
                                "{} does not support session resume",
                                cli_type.display_name()
                            ),
                        };
                    }

                    // Update status to active
                    if let Err(e) = db.update_session_status(&session_id, "active") {
                        return ServerMessage::Error {
                            code: "db_error".to_string(),
                            message: e.to_string(),
                        };
                    }

                    // Emit event to start PTY with resume flag
                    // ISSUE #2: Include claude_skip_permissions from mobile
                    let _ = app.emit(
                        "resume-session",
                        serde_json::json!({
                            "sessionId": session.id,
                            "projectPath": session.project_path,
                            "conversationId": session.conversation_id,
                            "cliType": session.cli_type,
                            "claudeSkipPermissions": claude_skip_permissions,
                        }),
                    );

                    let session_info = SessionInfo {
                        id: session.id.clone(),
                        name: session.name.clone(),
                        project_path: session.project_path.clone(),
                        created_at: session.created_at.clone(),
                        last_active_at: session.last_active_at.clone(),
                        status: "active".to_string(),
                        cli_type: session.cli_type.clone(),
                    };

                    // Emit session-resumed to notify desktop UI and broadcast to all WS clients
                    let _ = app.emit(
                        "session-resumed",
                        serde_json::json!({
                            "id": session_info.id,
                            "name": session_info.name,
                            "projectPath": session_info.project_path,
                            "createdAt": session_info.created_at,
                            "lastActiveAt": session_info.last_active_at,
                            "status": session_info.status,
                            "cliType": session_info.cli_type,
                        }),
                    );

                    ServerMessage::SessionResumed {
                        session: session_info,
                    }
                }
                Ok(None) => ServerMessage::Error {
                    code: "session_not_found".to_string(),
                    message: "Session not found".to_string(),
                },
                Err(e) => ServerMessage::Error {
                    code: "db_error".to_string(),
                    message: e.to_string(),
                },
            }
        }

        ClientMessage::Subscribe { session_id } => {
            // CRITICAL FIX: When mobile subscribes, request the current input state from desktop
            // This ensures mobile sees any pending input the desktop user has typed
            let _ = app.emit(
                "request-input-state",
                serde_json::json!({
                    "sessionId": session_id,
                }),
            );

            // FIX FOR ISSUE 1 & 6: Also request the current waiting state
            // This ensures mobile sees the correct status (awaiting_response vs working)
            // when subscribing to a session that's already waiting for input
            let _ = app.emit(
                "request-waiting-state",
                serde_json::json!({
                    "sessionId": session_id,
                }),
            );

            // New: Send recent activities immediately so tool calls appear on mobile
            let activities = if let Ok(Some(session)) = db.get_session(&session_id) {
                let limit_val = 120;
                if session.cli_type == "claude" {
                    if let Some(ref conversation_id) = session.conversation_id {
                        match jsonl::read_activities(&session.project_path, conversation_id) {
                            Ok(acts) => {
                                acts.into_iter()
                                    .filter(|a| {
                                        if a.activity_type == crate::parser::ActivityType::Thinking {
                                            a.content.len() < 500
                                        } else {
                                            true
                                        }
                                    })
                                    .take(limit_val)
                                    .map(|a| ActivityInfo {
                                        activity_type: match a.activity_type {
                                            crate::parser::ActivityType::Thinking => "thinking",
                                            crate::parser::ActivityType::ToolStart => "tool_start",
                                            crate::parser::ActivityType::ToolResult => "tool_result",
                                            crate::parser::ActivityType::FileRead => "file_read",
                                            crate::parser::ActivityType::FileWrite => "file_write",
                                            crate::parser::ActivityType::BashCommand => "bash_command",
                                            crate::parser::ActivityType::CodeDiff => "code_diff",
                                            crate::parser::ActivityType::Progress => "progress",
                                            crate::parser::ActivityType::UserPrompt => "user_prompt",
                                            crate::parser::ActivityType::Summary => "summary",
                                            _ => "text",
                                        }
                                        .to_string(),
                                        content: a.content,
                                        tool_name: a.tool_name,
                                        tool_params: a.tool_params,
                                        file_path: a.file_path,
                                        is_streaming: a.is_streaming,
                                        timestamp: a.timestamp,
                                        uuid: a.uuid,
                                        summary: a.summary,
                                    })
                                    .collect::<Vec<_>>()
                            }
                            Err(_) => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            if !activities.is_empty() {
                let _ = app.emit(
                    "activities",
                    serde_json::json!({
                        "sessionId": session_id,
                        "activities": activities,
                    }),
                );
            }

            tracing::info!(
                "Mobile subscribed to session {}, requesting current input and waiting state",
                session_id
            );

            ServerMessage::Welcome {
                server_version: "0.1.0".to_string(),
                authenticated: true,
            }
        }

        ClientMessage::Unsubscribe { .. } => {
            // Unsubscription doesn't need special handling
            ServerMessage::Welcome {
                server_version: "0.1.0".to_string(),
                authenticated: true,
            }
        }

        ClientMessage::ListDirectory { path } => {
            // List directory contents for remote file browser
            let target_path =
                path.unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".to_string()));

            // Validate path to prevent directory traversal
            match validate_path(&target_path) {
                Err(e) => ServerMessage::Error {
                    code: "access_denied".to_string(),
                    message: e,
                },
                Ok(validated_path) => {
                    let path_str = validated_path.to_string_lossy().to_string();
                    match std::fs::read_dir(&validated_path) {
                        Ok(entries) => {
                            let mut dir_entries: Vec<DirectoryEntry> = entries
                                .filter_map(|e| e.ok())
                                .filter_map(|entry| {
                                    let name = entry.file_name().to_string_lossy().to_string();
                                    // Skip hidden files
                                    if name.starts_with('.') {
                                        return None;
                                    }
                                    let is_dir =
                                        entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                    Some(DirectoryEntry { name, is_dir })
                                })
                                .collect();

                            // Sort: directories first, then alphabetically
                            dir_entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                                (true, false) => std::cmp::Ordering::Less,
                                (false, true) => std::cmp::Ordering::Greater,
                                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                            });

                            ServerMessage::DirectoryListing {
                                path: path_str,
                                entries: dir_entries,
                            }
                        }
                        Err(e) => ServerMessage::Error {
                            code: "fs_error".to_string(),
                            message: e.to_string(),
                        },
                    }
                }
            }
        }

        ClientMessage::CreateDirectory { path } => {
            // Validate path to prevent directory traversal
            match validate_path(&path) {
                Err(e) => ServerMessage::Error {
                    code: "access_denied".to_string(),
                    message: e,
                },
                Ok(validated_path) => {
                    let path_str = validated_path.to_string_lossy().to_string();
                    match std::fs::create_dir_all(&validated_path) {
                        Ok(()) => {
                            tracing::info!("Created directory: {}", path_str);
                            ServerMessage::DirectoryCreated {
                                path: path_str,
                                success: true,
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to create directory {}: {}", path_str, e);
                            ServerMessage::Error {
                                code: "fs_error".to_string(),
                                message: e.to_string(),
                            }
                        }
                    }
                }
            }
        }

        ClientMessage::UploadFile {
            filename,
            data,
            mime_type,
        } => {
            use base64::{engine::general_purpose::STANDARD, Engine as _};

            // Decode base64 data
            let decoded = match STANDARD.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return ServerMessage::UploadError {
                        message: format!("Failed to decode base64 data: {}", e),
                    };
                }
            };

            // Security: Validate file before processing
            if let Err(e) = validate_upload(&filename, decoded.len()) {
                tracing::warn!("Upload rejected: {} (file: {})", e, filename);
                return ServerMessage::UploadError { message: e };
            }

            // Create uploads directory in temp
            let upload_dir = std::env::temp_dir().join("mobilecli_uploads");
            if let Err(e) = std::fs::create_dir_all(&upload_dir) {
                return ServerMessage::UploadError {
                    message: format!("Failed to create upload directory: {}", e),
                };
            }

            // Generate unique filename to avoid collisions
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let safe_filename = filename
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
                .collect::<String>();
            let unique_filename = format!("{}_{}", timestamp, safe_filename);
            let file_path = upload_dir.join(&unique_filename);

            // Write file
            match std::fs::write(&file_path, &decoded) {
                Ok(_) => {
                    let path_str = file_path.to_string_lossy().to_string();
                    tracing::info!(
                        "File uploaded: {} ({} bytes, {})",
                        path_str,
                        decoded.len(),
                        mime_type
                    );
                    ServerMessage::FileUploaded {
                        path: path_str,
                        filename: unique_filename,
                    }
                }
                Err(e) => ServerMessage::UploadError {
                    message: format!("Failed to write file: {}", e),
                },
            }
        }

        ClientMessage::RenameSession {
            session_id,
            new_name,
        } => {
            match db.rename_session(&session_id, &new_name) {
                Ok(_) => {
                    // Emit event to notify other clients
                    let _ = app.emit(
                        "session-renamed",
                        serde_json::json!({
                            "sessionId": session_id,
                            "newName": new_name,
                        }),
                    );
                    ServerMessage::SessionRenamed {
                        session_id,
                        new_name,
                    }
                }
                Err(e) => ServerMessage::Error {
                    code: "db_error".to_string(),
                    message: e.to_string(),
                },
            }
        }

        ClientMessage::DeleteSession { session_id } => {
            // First close any active PTY session
            let _ = app.emit(
                "close-session",
                serde_json::json!({ "sessionId": session_id }),
            );

            match db.delete_session(&session_id) {
                Ok(_) => {
                    // Emit event to notify other clients
                    let _ = app.emit(
                        "session-deleted",
                        serde_json::json!({ "sessionId": session_id }),
                    );
                    ServerMessage::SessionDeleted { session_id }
                }
                Err(e) => ServerMessage::Error {
                    code: "db_error".to_string(),
                    message: e.to_string(),
                },
            }
        }
        ClientMessage::SyncInputState {
            session_id,
            text,
            cursor_position,
            sender_id,
        } => {
            // Broadcast input state to all other clients (for real-time input sync)
            // Include sender_id and timestamp so receivers can filter their own echoes
            let timestamp = chrono::Utc::now().timestamp_millis() as u64;
            let _ = app.emit(
                "input-state",
                serde_json::json!({
                    "sessionId": session_id,
                    "text": text,
                    "cursorPosition": cursor_position,
                    "senderId": sender_id,
                    "timestamp": timestamp,
                }),
            );
            // Return the same state as acknowledgment
            ServerMessage::InputState {
                session_id,
                text,
                cursor_position,
                sender_id,
                timestamp: Some(timestamp),
            }
        }
        ClientMessage::Ping => {
            // Respond immediately to heartbeat ping
            ServerMessage::Pong
        }

        ClientMessage::RegisterPushToken {
            token,
            token_type,
            platform,
        } => {
            tracing::info!(
                "Registering push token: type={}, platform={}, token={}...",
                token_type,
                platform,
                &token[..token.len().min(20)]
            );

            // Store the token (replace existing token with same value to avoid duplicates)
            {
                let mut tokens = PUSH_TOKENS.write().await;
                // Remove any existing token with the same value (device re-registration)
                tokens.retain(|t| t.token != token);
                tokens.push(PushToken {
                    token: token.clone(),
                    token_type: token_type.clone(),
                    platform: platform.clone(),
                    registered_at: std::time::Instant::now(),
                });
                tracing::info!("Push tokens stored: {} total", tokens.len());
            }

            ServerMessage::PushTokenRegistered {
                token_type,
                platform,
            }
        }
    }
}
