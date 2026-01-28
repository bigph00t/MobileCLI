//! Background daemon for MobileCLI
//!
//! Single WebSocket server that all terminal sessions stream to.
//! Mobile connects once and sees all active sessions.

use crate::protocol::{ClientMessage, ServerMessage, SessionListItem};
use crate::session::{self, SessionInfo};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Default WebSocket port
pub const DEFAULT_PORT: u16 = 9847;

/// PID file path
fn pid_file() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".mobilecli").join("daemon.pid")
}

/// Port file path
fn port_file() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".mobilecli").join("daemon.port")
}

/// Get the running daemon's port (reads from port file)
pub fn get_port() -> Option<u16> {
    std::fs::read_to_string(port_file())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Check if daemon is running
pub fn is_running() -> bool {
    let pid_path = pid_file();
    if !pid_path.exists() {
        return false;
    }

    if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return is_process_alive(pid);
        }
    }
    false
}

/// Check if a process is alive (portable Unix implementation)
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    // kill with signal 0 checks if process exists without sending a signal
    kill(Pid::from_raw(pid as i32), None::<Signal>).is_ok()
}

#[cfg(not(unix))]
fn is_process_alive(_pid: u32) -> bool {
    // Conservative default on non-Unix platforms
    true
}

/// Get daemon PID
pub fn get_pid() -> Option<u32> {
    std::fs::read_to_string(pid_file())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Waiting state for a session
#[derive(Debug, Clone)]
pub struct WaitingState {
    pub wait_type: String,      // "tool_approval" | "plan_approval" | "question" | "input"
    pub prompt_content: String,
    pub timestamp: chrono::DateTime<Utc>,
}

/// Push notification token
#[derive(Debug, Clone)]
pub struct PushToken {
    pub token: String,
    pub token_type: String,  // "expo" | "apns" | "fcm"
    pub platform: String,    // "ios" | "android"
}

/// Active PTY session
pub struct PtySession {
    pub session_id: String,
    pub name: String,
    pub command: String,
    pub project_path: String,
    pub started_at: chrono::DateTime<Utc>,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub resize_tx: mpsc::UnboundedSender<(u16, u16)>,
    pub waiting_state: Option<WaitingState>,
}

/// Daemon shared state
pub struct DaemonState {
    pub sessions: HashMap<String, PtySession>,
    pub mobile_clients: HashMap<SocketAddr, mpsc::UnboundedSender<Message>>,
    pub pty_broadcast: broadcast::Sender<(String, Vec<u8>)>,
    pub port: u16, // The actual port the daemon is running on
    pub push_tokens: Vec<PushToken>,
}

impl DaemonState {
    pub fn new(port: u16) -> Self {
        let (pty_broadcast, _) = broadcast::channel(256);
        Self {
            sessions: HashMap::new(),
            mobile_clients: HashMap::new(),
            pty_broadcast,
            port,
            push_tokens: Vec::new(),
        }
    }
}

pub type SharedState = Arc<RwLock<DaemonState>>;

/// Start the daemon (blocking - run in background)
pub async fn run(port: u16) -> std::io::Result<()> {
    // Write PID file
    let pid_path = pid_file();
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Write port file so wrapper/QR can find the actual port
    let port_path = port_file();
    std::fs::write(&port_path, port.to_string())?;

    let state: SharedState = Arc::new(RwLock::new(DaemonState::new(port)));

    // Start WebSocket server on all interfaces (0.0.0.0)
    // This is intentional - mobile clients need network access to connect.
    // Security model: Access is controlled at the network level via:
    // - Local network: Only devices on same WiFi can connect
    // - Tailscale: Only authenticated Tailscale network members can connect
    // Users explicitly choose their connection mode in setup wizard.
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Daemon WebSocket server on port {}", port);

    // Run the main loop with platform-specific signal handling
    #[cfg(unix)]
    run_server_loop_unix(listener, state).await;

    #[cfg(not(unix))]
    run_server_loop_ctrlc_only(listener, state).await;

    // Cleanup
    let _ = std::fs::remove_file(&pid_path);
    let _ = std::fs::remove_file(&port_path);
    Ok(())
}

/// Server loop with Unix signal handling (SIGTERM + Ctrl+C)
#[cfg(unix)]
async fn run_server_loop_unix(listener: TcpListener, state: SharedState) {
    use tokio::signal::unix::{signal, SignalKind};

    // Try to set up SIGTERM handler, fall back to Ctrl+C only if it fails
    let sigterm_result = signal(SignalKind::terminate());
    if sigterm_result.is_err() {
        tracing::warn!(
            "Failed to set up SIGTERM handler: {:?}. Only Ctrl+C will work for shutdown.",
            sigterm_result.err()
        );
        // Fall back to generic loop with just Ctrl+C
        run_server_loop_ctrlc_only(listener, state).await;
        return;
    }
    let mut sigterm = sigterm_result.unwrap();

    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((stream, addr)) = result {
                    let state = state.clone();
                    tokio::spawn(handle_connection(stream, addr, state));
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Daemon shutting down (Ctrl+C)");
                break;
            }
            _ = sigterm.recv() => {
                tracing::info!("Daemon shutting down (SIGTERM)");
                break;
            }
        }
    }
}

/// Server loop with Ctrl+C only (fallback or non-Unix)
async fn run_server_loop_ctrlc_only(listener: TcpListener, state: SharedState) {
    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((stream, addr)) = result {
                    let state = state.clone();
                    tokio::spawn(handle_connection(stream, addr, state));
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Daemon shutting down (Ctrl+C)");
                break;
            }
        }
    }
}

/// Handle WebSocket connection (could be mobile client or PTY session)
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: SharedState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws = accept_async(stream).await?;
    let (tx, mut rx) = ws.split();

    // Wait for first message to determine client type
    let first_msg = rx.next().await;

    match first_msg {
        Some(Ok(Message::Text(text))) => {
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&text) {
                if msg.get("type").and_then(|v| v.as_str()) == Some("register_pty") {
                    // This is a PTY session registering
                    return handle_pty_session(msg, tx, rx, addr, state).await;
                }
            }
            // Assume it's a mobile client
            handle_mobile_client(Some(text), tx, rx, addr, state).await
        }
        _ => Ok(()),
    }
}

/// Handle mobile client connection
async fn handle_mobile_client(
    first_msg: Option<String>,
    mut tx: futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>,
    mut rx: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    addr: SocketAddr,
    state: SharedState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Mobile client connected: {}", addr);

    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Message>();

    // Register client and get broadcast receiver
    let mut pty_rx = {
        let mut st = state.write().await;
        st.mobile_clients.insert(addr, client_tx);
        st.pty_broadcast.subscribe()
    };

    // Send welcome
    let welcome = ServerMessage::Welcome {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        authenticated: true,
    };
    tx.send(Message::Text(serde_json::to_string(&welcome)?)).await?;

    // Send sessions list
    send_sessions_list(&state, &mut tx).await?;

    // Process first message if it was a client message
    if let Some(text) = first_msg {
        if let Ok(msg) = serde_json::from_str::<ClientMessage>(&text) {
            process_client_msg(msg, &state, &mut tx).await?;
        }
    }

    loop {
        tokio::select! {
            // PTY output
            result = pty_rx.recv() => {
                match result {
                    Ok((session_id, data)) => {
                        let msg = ServerMessage::PtyBytes {
                            session_id,
                            data: BASE64.encode(&data),
                        };
                        if tx.send(Message::Text(serde_json::to_string(&msg)?)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }

            // Queued messages
            Some(msg) = client_rx.recv() => {
                if tx.send(msg).await.is_err() {
                    break;
                }
            }

            // Client messages
            result = rx.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(msg) = serde_json::from_str::<ClientMessage>(&text) {
                            process_client_msg(msg, &state, &mut tx).await?;
                        }
                    }
                    Some(Ok(Message::Ping(d))) => { let _ = tx.send(Message::Pong(d)).await; }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    // Unregister
    state.write().await.mobile_clients.remove(&addr);
    tracing::info!("Mobile client disconnected: {}", addr);
    Ok(())
}

/// Handle PTY session registration
async fn handle_pty_session(
    reg_msg: serde_json::Value,
    mut tx: futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>,
    mut rx: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    _addr: SocketAddr,
    state: SharedState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_id = reg_msg["session_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Missing or empty session_id in registration")?
        .to_string();
    let name = reg_msg["name"].as_str().unwrap_or("Terminal").to_string();
    let command = reg_msg["command"].as_str().unwrap_or("shell").to_string();
    let project_path = reg_msg["project_path"].as_str().unwrap_or("").to_string();

    tracing::info!("PTY session registered: {} ({})", name, session_id);

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();

    // Register session
    let pty_broadcast = {
        let mut st = state.write().await;
        st.sessions.insert(session_id.clone(), PtySession {
            session_id: session_id.clone(),
            name: name.clone(),
            command,
            project_path,
            started_at: Utc::now(),
            input_tx,
            resize_tx,
            waiting_state: None,
        });
        st.pty_broadcast.clone()
    };

    // Notify mobile clients and persist to file
    broadcast_sessions_update(&state).await;
    persist_sessions_to_file(&state).await;

    // Send ACK
    tx.send(Message::Text(r#"{"type":"registered"}"#.to_string())).await?;

    // Buffer for detecting waiting state patterns
    let mut output_buffer = String::new();
    const BUFFER_MAX: usize = 2000; // Keep last 2KB for pattern matching

    loop {
        tokio::select! {
            // PTY output from terminal wrapper
            result = rx.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&text) {
                            if msg["type"].as_str() == Some("pty_output") {
                                if let Some(data) = msg["data"].as_str() {
                                    if let Ok(bytes) = BASE64.decode(data) {
                                        let _ = pty_broadcast.send((session_id.clone(), bytes.clone()));

                                        // Append to buffer for pattern detection
                                        let text = String::from_utf8_lossy(&bytes);
                                        output_buffer.push_str(&text);
                                        if output_buffer.len() > BUFFER_MAX {
                                            output_buffer = output_buffer[output_buffer.len() - BUFFER_MAX..].to_string();
                                        }

                                        // Check for waiting state patterns
                                        if let Some((wait_type, prompt_content)) = detect_waiting_state(&output_buffer) {
                                            let should_notify = {
                                                let mut st = state.write().await;
                                                if let Some(session) = st.sessions.get_mut(&session_id) {
                                                    // Only notify if not already waiting
                                                    let was_waiting = session.waiting_state.is_some();
                                                    session.waiting_state = Some(WaitingState {
                                                        wait_type: wait_type.to_string(),
                                                        prompt_content: prompt_content.clone(),
                                                        timestamp: Utc::now(),
                                                    });
                                                    !was_waiting
                                                } else {
                                                    false
                                                }
                                            };

                                            if should_notify {
                                                // Broadcast to mobile clients
                                                broadcast_waiting_for_input(&state, &session_id, wait_type, &prompt_content).await;

                                                // Send push notifications
                                                let tokens = {
                                                    let st = state.read().await;
                                                    st.push_tokens.clone()
                                                };
                                                send_push_notifications(
                                                    &tokens,
                                                    "Action Required",
                                                    &format!("{}: {}", name, wait_type.replace('_', " ")),
                                                    &session_id,
                                                ).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        let _ = pty_broadcast.send((session_id.clone(), data));
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            // Input from mobile to send to PTY
            Some(input) = input_rx.recv() => {
                let msg = serde_json::json!({
                    "type": "input",
                    "data": BASE64.encode(&input),
                });
                if tx.send(Message::Text(msg.to_string())).await.is_err() {
                    break;
                }

                // Clear waiting state when user sends input
                {
                    let mut st = state.write().await;
                    if let Some(session) = st.sessions.get_mut(&session_id) {
                        if session.waiting_state.is_some() {
                            session.waiting_state = None;
                            drop(st);
                            broadcast_waiting_cleared(&state, &session_id).await;
                        }
                    }
                }

                // Clear output buffer on input
                output_buffer.clear();
            }

            // Resize from mobile
            Some((cols, rows)) = resize_rx.recv() => {
                let msg = serde_json::json!({
                    "type": "resize",
                    "cols": cols,
                    "rows": rows,
                });
                if tx.send(Message::Text(msg.to_string())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Unregister session
    {
        let mut st = state.write().await;
        st.sessions.remove(&session_id);

        // Notify about session end
        let msg = ServerMessage::SessionEnded {
            session_id: session_id.clone(),
            exit_code: 0,
        };
        let msg_str = serde_json::to_string(&msg)?;
        for client in st.mobile_clients.values() {
            let _ = client.send(Message::Text(msg_str.clone()));
        }
    }

    // Broadcast updated sessions list to all clients
    broadcast_sessions_update(&state).await;

    // Update persisted sessions
    persist_sessions_to_file(&state).await;

    tracing::info!("PTY session ended: {}", session_id);
    Ok(())
}

/// Process a message from mobile client
async fn process_client_msg(
    msg: ClientMessage,
    state: &SharedState,
    tx: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match msg {
        ClientMessage::Hello { client_version, .. } => {
            // Already sent Welcome on connect, but log the client version
            tracing::debug!("Client hello, version: {}", client_version);
        }
        ClientMessage::Subscribe { session_id } => {
            // Currently all clients receive all session output via broadcast
            // This is logged for future per-session subscription support
            tracing::debug!("Client subscribed to session: {}", session_id);
        }
        ClientMessage::Unsubscribe { session_id } => {
            // Currently all clients receive all session output via broadcast
            tracing::debug!("Client unsubscribed from session: {}", session_id);
        }
        ClientMessage::SendInput { session_id, text, .. } => {
            let st = state.read().await;
            if let Some(session) = st.sessions.get(&session_id) {
                let _ = session.input_tx.send(text.into_bytes());
            }
        }
        ClientMessage::PtyResize { session_id, cols, rows } => {
            let st = state.read().await;
            if let Some(session) = st.sessions.get(&session_id) {
                let _ = session.resize_tx.send((cols, rows));
            }
        }
        ClientMessage::Ping => {
            tx.send(Message::Text(serde_json::to_string(&ServerMessage::Pong)?)).await?;
        }
        ClientMessage::GetSessions => {
            send_sessions_list(state, tx).await?;
        }
        ClientMessage::RenameSession { session_id, new_name } => {
            let renamed = {
                let mut st = state.write().await;
                if let Some(session) = st.sessions.get_mut(&session_id) {
                    session.name = new_name.clone();
                    true
                } else {
                    false
                }
            };

            if renamed {
                // Send confirmation
                let msg = ServerMessage::SessionRenamed {
                    session_id: session_id.clone(),
                    new_name: new_name.clone(),
                };
                tx.send(Message::Text(serde_json::to_string(&msg)?)).await?;

                // Broadcast updated sessions list to all clients
                broadcast_sessions_update(state).await;

                // Update persisted sessions file
                persist_sessions_to_file(state).await;

                tracing::info!("Session {} renamed to '{}'", session_id, new_name);
            } else {
                let msg = ServerMessage::Error {
                    code: "session_not_found".to_string(),
                    message: format!("Session {} not found", session_id),
                };
                tx.send(Message::Text(serde_json::to_string(&msg)?)).await?;
            }
        }
        ClientMessage::RegisterPushToken { token, token_type, platform } => {
            let mut st = state.write().await;
            // Remove existing token with same value to avoid duplicates
            st.push_tokens.retain(|t| t.token != token);
            st.push_tokens.push(PushToken {
                token: token.clone(),
                token_type: token_type.clone(),
                platform: platform.clone(),
            });
            tracing::info!("Registered push token ({}/{})", token_type, platform);
        }
        ClientMessage::ToolApproval { session_id, response } => {
            // Map approval response to the appropriate input for the CLI
            let input = match response.as_str() {
                "yes" => "y\n",
                "yes_always" => "a\n",  // 'a' for always in many CLIs
                "no" => "n\n",
                _ => "n\n",  // Default to no for safety
            };

            let cleared = {
                let mut st = state.write().await;
                if let Some(session) = st.sessions.get_mut(&session_id) {
                    let _ = session.input_tx.send(input.as_bytes().to_vec());
                    // Clear the waiting state
                    session.waiting_state = None;
                    true
                } else {
                    false
                }
            };

            if cleared {
                // Broadcast waiting_cleared to all clients
                broadcast_waiting_cleared(state, &session_id).await;
            }
        }
    }
    Ok(())
}

/// Send sessions list to a client
async fn send_sessions_list(
    state: &SharedState,
    tx: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let st = state.read().await;
    let port = st.port;
    let items: Vec<SessionListItem> = st.sessions.values().map(|s| SessionListItem {
        session_id: s.session_id.clone(),
        name: s.name.clone(),
        command: s.command.clone(),
        project_path: s.project_path.clone(),
        ws_port: port,
        started_at: s.started_at.to_rfc3339(),
        cli_type: "terminal".to_string(),
    }).collect();
    let msg = ServerMessage::Sessions { sessions: items };
    tx.send(Message::Text(serde_json::to_string(&msg)?)).await?;
    Ok(())
}

/// Broadcast sessions update to all mobile clients
async fn broadcast_sessions_update(state: &SharedState) {
    let st = state.read().await;
    let port = st.port;
    let items: Vec<SessionListItem> = st.sessions.values().map(|s| SessionListItem {
        session_id: s.session_id.clone(),
        name: s.name.clone(),
        command: s.command.clone(),
        project_path: s.project_path.clone(),
        ws_port: port,
        started_at: s.started_at.to_rfc3339(),
        cli_type: "terminal".to_string(),
    }).collect();
    let msg = ServerMessage::Sessions { sessions: items };
    if let Ok(msg_str) = serde_json::to_string(&msg) {
        for client in st.mobile_clients.values() {
            let _ = client.send(Message::Text(msg_str.clone()));
        }
    }
}

/// Persist daemon sessions to file for status command
async fn persist_sessions_to_file(state: &SharedState) {
    let st = state.read().await;
    let port = st.port;
    let sessions: Vec<SessionInfo> = st
        .sessions
        .values()
        .map(|s| SessionInfo {
            session_id: s.session_id.clone(),
            name: s.name.clone(),
            command: s.command.clone(),
            args: vec![],
            project_path: s.project_path.clone(),
            ws_port: port,
            pid: std::process::id(), // daemon PID since we manage all sessions
            started_at: s.started_at,
        })
        .collect();
    if let Err(e) = session::save_sessions(&sessions) {
        tracing::warn!("Failed to persist sessions: {}", e);
    }
}

/// Broadcast waiting_for_input to all mobile clients
async fn broadcast_waiting_for_input(
    state: &SharedState,
    session_id: &str,
    wait_type: &str,
    prompt_content: &str,
) {
    let st = state.read().await;
    let msg = ServerMessage::WaitingForInput {
        session_id: session_id.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        prompt_content: prompt_content.to_string(),
        wait_type: wait_type.to_string(),
        cli_type: "terminal".to_string(),
    };
    if let Ok(msg_str) = serde_json::to_string(&msg) {
        for client in st.mobile_clients.values() {
            let _ = client.send(Message::Text(msg_str.clone()));
        }
    }
}

/// Broadcast waiting_cleared to all mobile clients
async fn broadcast_waiting_cleared(state: &SharedState, session_id: &str) {
    let st = state.read().await;
    let msg = ServerMessage::WaitingCleared {
        session_id: session_id.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };
    if let Ok(msg_str) = serde_json::to_string(&msg) {
        for client in st.mobile_clients.values() {
            let _ = client.send(Message::Text(msg_str.clone()));
        }
    }
}

/// Detect waiting state patterns in PTY output
/// Returns (wait_type, prompt_content) if a waiting pattern is detected
fn detect_waiting_state(output: &str) -> Option<(&'static str, String)> {
    // Common approval/confirmation patterns
    let approval_patterns = [
        "Do you want to proceed?",
        "Allow tool to run?",
        "Do you want to continue?",
        "Proceed?",
        "[Y/n]",
        "[y/N]",
        "Allow | Deny",
        "Press Enter to continue",
        "(y/n)",
        "(Y/n)",
        "(yes/no)",
    ];

    // Question patterns (ends with ?)
    let is_question = output.trim().ends_with('?')
        && !approval_patterns.iter().any(|p| output.contains(p));

    for pattern in &approval_patterns {
        if output.contains(pattern) {
            // Extract a reasonable prompt snippet (last 200 chars or so)
            let prompt = if output.len() > 200 {
                &output[output.len() - 200..]
            } else {
                output
            };
            return Some(("tool_approval", prompt.to_string()));
        }
    }

    if is_question {
        let prompt = if output.len() > 200 {
            &output[output.len() - 200..]
        } else {
            output
        };
        return Some(("question", prompt.to_string()));
    }

    None
}

/// Send push notifications to all registered tokens
async fn send_push_notifications(
    tokens: &[PushToken],
    title: &str,
    body: &str,
    session_id: &str,
) {
    if tokens.is_empty() {
        return;
    }

    // Build Expo push messages
    let messages: Vec<serde_json::Value> = tokens
        .iter()
        .filter(|t| t.token_type == "expo")
        .map(|t| {
            serde_json::json!({
                "to": t.token,
                "title": title,
                "body": body,
                "data": {
                    "session_id": session_id,
                    "type": "waiting_for_input"
                },
                "sound": "default",
                "priority": "high"
            })
        })
        .collect();

    if messages.is_empty() {
        return;
    }

    // Send to Expo Push API
    let client = reqwest::Client::new();
    match client
        .post("https://exp.host/--/api/v2/push/send")
        .header("Content-Type", "application/json")
        .json(&messages)
        .send()
        .await
    {
        Ok(resp) => {
            if !resp.status().is_success() {
                tracing::warn!("Push notification failed: {}", resp.status());
            } else {
                tracing::debug!("Push notification sent to {} devices", messages.len());
            }
        }
        Err(e) => {
            tracing::warn!("Failed to send push notification: {}", e);
        }
    }
}
