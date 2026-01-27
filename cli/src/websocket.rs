//! WebSocket server for mobile connections
//!
//! Single-session WebSocket server that streams PTY output to mobile clients.

use crate::protocol::{ClientMessage, ServerMessage, SessionListItem};
use crate::session;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Connected client
type ClientTx = mpsc::UnboundedSender<Message>;
type ClientMap = Arc<RwLock<HashMap<SocketAddr, ClientTx>>>;

/// Channels returned by the WebSocket server
pub struct WsChannels {
    /// Channel for receiving input from clients
    pub input_rx: mpsc::UnboundedReceiver<String>,
    /// Channel for resize events
    pub resize_rx: mpsc::UnboundedReceiver<(u16, u16)>,
}

/// WebSocket server handle
pub struct WsServer {
    /// Session ID
    session_id: String,
    /// Session name
    session_name: String,
    /// Broadcast channel for PTY output
    pty_tx: broadcast::Sender<Vec<u8>>,
    /// Port the server is listening on
    port: u16,
    /// Connected clients
    clients: ClientMap,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
}

impl WsServer {
    /// Start a new WebSocket server, returns server handle and channels
    pub async fn start(session_id: String, port: u16) -> std::io::Result<(Self, WsChannels)> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        let actual_port = listener.local_addr()?.port();

        let (pty_tx, _) = broadcast::channel::<Vec<u8>>(256);
        let (input_tx, input_rx) = mpsc::unbounded_channel::<String>();
        let (resize_tx, resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let clients: ClientMap = Arc::new(RwLock::new(HashMap::new()));

        // Get session name from session info
        let session_name = session::get_session(&session_id)
            .map(|s| s.name)
            .unwrap_or_else(|| "Terminal".to_string());

        // Clone for the accept loop
        let session_id_clone = session_id.clone();
        let session_name_clone = session_name.clone();
        let pty_tx_clone = pty_tx.clone();
        let clients_clone = clients.clone();
        let input_tx_clone = input_tx;
        let resize_tx_clone = resize_tx;
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Spawn the accept loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                let session_id = session_id_clone.clone();
                                let session_name = session_name_clone.clone();
                                let pty_rx = pty_tx_clone.subscribe();
                                let clients = clients_clone.clone();
                                let input_tx = input_tx_clone.clone();
                                let resize_tx = resize_tx_clone.clone();

                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        stream,
                                        addr,
                                        session_id,
                                        session_name,
                                        pty_rx,
                                        clients,
                                        input_tx,
                                        resize_tx,
                                    )
                                    .await
                                    {
                                        tracing::debug!("Client {} error: {}", addr, e);
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::debug!("WebSocket server shutting down");
                        break;
                    }
                }
            }
        });

        tracing::info!("WebSocket server listening on port {}", actual_port);

        let server = Self {
            session_id,
            session_name,
            pty_tx,
            port: actual_port,
            clients,
            shutdown_tx,
        };

        let channels = WsChannels { input_rx, resize_rx };

        Ok((server, channels))
    }

    /// Get the port the server is listening on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Send PTY output to all connected clients
    pub fn broadcast_pty_output(&self, data: &[u8]) {
        let _ = self.pty_tx.send(data.to_vec());
    }

    /// Shutdown the server
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Handle a single client connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    session_id: String,
    session_name: String,
    mut pty_rx: broadcast::Receiver<Vec<u8>>,
    clients: ClientMap,
    input_tx: mpsc::UnboundedSender<String>,
    resize_tx: mpsc::UnboundedSender<(u16, u16)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    tracing::info!("Mobile client connected: {}", addr);

    // Create channel for sending messages to this client
    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Message>();

    // Register client
    clients.write().await.insert(addr, client_tx);

    // Send welcome message
    // Note: authenticated=true indicates connection accepted. Security relies on
    // network access control (local network, Tailscale VPN) rather than password auth.
    let welcome = ServerMessage::Welcome {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        authenticated: true,
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&welcome)?))
        .await?;

    // Send session info - get command from registered session data
    let (command, project_path) = session::get_session(&session_id)
        .map(|s| (s.command, s.project_path))
        .unwrap_or_else(|| {
            // Fallback if session not registered yet
            let cmd = std::env::var("SHELL").unwrap_or_else(|_| "shell".to_string());
            let path = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            (cmd, path)
        });
    let session_info = ServerMessage::SessionInfo {
        session_id: session_id.clone(),
        name: session_name.clone(),
        command,
        project_path,
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&session_info)?))
        .await?;

    loop {
        tokio::select! {
            // PTY output to send to client
            result = pty_rx.recv() => {
                match result {
                    Ok(data) => {
                        let msg = ServerMessage::PtyBytes {
                            session_id: session_id.clone(),
                            data: BASE64.encode(&data),
                        };
                        if ws_sender.send(Message::Text(serde_json::to_string(&msg)?)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Client is slow, skip some data
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Messages from this client's queue
            Some(msg) = client_rx.recv() => {
                if ws_sender.send(msg).await.is_err() {
                    break;
                }
            }

            // Messages from WebSocket
            result = ws_receiver.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            match client_msg {
                                ClientMessage::SendInput { text, .. } => {
                                    let _ = input_tx.send(text);
                                }
                                ClientMessage::PtyResize { cols, rows, .. } => {
                                    let _ = resize_tx.send((cols, rows));
                                }
                                ClientMessage::Ping => {
                                    let pong = ServerMessage::Pong;
                                    let _ = ws_sender.send(Message::Text(serde_json::to_string(&pong)?)).await;
                                }
                                ClientMessage::GetSessions => {
                                    let sessions = session::list_active_sessions();
                                    let items: Vec<SessionListItem> = sessions
                                        .into_iter()
                                        .map(|s| SessionListItem {
                                            session_id: s.session_id,
                                            name: s.name,
                                            command: s.command,
                                            project_path: s.project_path,
                                            ws_port: s.ws_port,
                                            started_at: s.started_at.to_rfc3339(),
                                        })
                                        .collect();
                                    let msg = ServerMessage::Sessions { sessions: items };
                                    let _ = ws_sender.send(Message::Text(serde_json::to_string(&msg)?)).await;
                                }
                                ClientMessage::RenameSession { session_id: sid, new_name } => {
                                    if sid == session_id {
                                        let _ = session::rename_session(&sid, &new_name);
                                        let msg = ServerMessage::SessionRenamed {
                                            session_id: sid,
                                            new_name,
                                        };
                                        let _ = ws_sender.send(Message::Text(serde_json::to_string(&msg)?)).await;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Unregister client
    clients.write().await.remove(&addr);
    tracing::info!("Mobile client disconnected: {}", addr);

    Ok(())
}
