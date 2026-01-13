//! MobileCLI Relay Server
//!
//! A lightweight WebSocket relay that connects desktop and mobile clients.
//! All message contents are end-to-end encrypted - the relay only sees opaque blobs.
//!
//! Security features:
//! - E2E encryption (relay cannot read messages)
//! - Room expiration (10 min if no client joins)
//! - Rate limiting (10 rooms/IP/minute)
//! - No message logging
//! - Memory-only storage

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{info, debug};
use uuid::Uuid;

/// Room state
struct Room {
    host_tx: mpsc::UnboundedSender<Message>,
    client_tx: Option<mpsc::UnboundedSender<Message>>,
    created_at: Instant,
    client_joined: bool,
}

/// Rate limiting state per IP
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// Shared state between all connections
struct RelayState {
    rooms: DashMap<String, Room>,
    rate_limits: DashMap<IpAddr, RateLimitEntry>,
    total_rooms_created: AtomicU64,
    total_connections: AtomicU64,
}

// Security constants
const ROOM_EXPIRY_NO_CLIENT: Duration = Duration::from_secs(600); // 10 minutes
const ROOM_EXPIRY_IDLE: Duration = Duration::from_secs(3600); // 1 hour idle
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT_MAX_ROOMS: u32 = 10;
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

impl RelayState {
    fn new() -> Self {
        Self {
            rooms: DashMap::new(),
            rate_limits: DashMap::new(),
            total_rooms_created: AtomicU64::new(0),
            total_connections: AtomicU64::new(0),
        }
    }

    /// Check rate limit for IP, returns true if allowed
    fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let now = Instant::now();

        let mut entry = self.rate_limits.entry(ip).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now.duration_since(entry.window_start) > RATE_LIMIT_WINDOW {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count >= RATE_LIMIT_MAX_ROOMS {
            return false;
        }

        entry.count += 1;
        true
    }

    /// Generate a secure room code (16 chars = ~82 bits of entropy)
    fn generate_code() -> String {
        // Use UUID v4 and encode as base32-like (no confusing chars)
        // 16 characters from 32-char alphabet = 32^16 = 1.2e24 combinations
        let chars: Vec<char> = "ABCDEFGHJKMNPQRSTUVWXYZ23456789".chars().collect();
        let uuid = Uuid::new_v4();
        let bytes = uuid.as_bytes();

        let mut code = String::with_capacity(16);
        for i in 0..16 {
            // Use each byte to select a character
            let idx = (bytes[i] as usize) % chars.len();
            code.push(chars[idx]);
        }
        code
    }

    /// Clean up expired rooms and stale rate limit entries
    fn cleanup(&self) {
        let now = Instant::now();
        let mut expired_rooms = Vec::new();

        // Find expired rooms
        for entry in self.rooms.iter() {
            let room = entry.value();
            let age = now.duration_since(room.created_at);

            // Room expires if no client joined within timeout, or if idle too long
            if (!room.client_joined && age > ROOM_EXPIRY_NO_CLIENT) || age > ROOM_EXPIRY_IDLE {
                expired_rooms.push(entry.key().clone());
            }
        }

        // Remove expired rooms
        for code in expired_rooms {
            if let Some((_, room)) = self.rooms.remove(&code) {
                info!("Room expired: {}", code);
                // Notify connected clients
                if let Some(client_tx) = room.client_tx {
                    let _ = client_tx.send(Message::Close(None));
                }
                let _ = room.host_tx.send(Message::Close(None));
            }
        }

        // Clean up old rate limit entries
        self.rate_limits.retain(|_, entry| {
            now.duration_since(entry.window_start) < RATE_LIMIT_WINDOW * 2
        });
    }
}

/// Messages sent to clients (relay protocol, not user data)
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum RelayMessage {
    #[serde(rename = "room_created")]
    RoomCreated { code: String },
    #[serde(rename = "client_joined")]
    ClientJoined,
    #[serde(rename = "client_left")]
    ClientLeft,
    #[serde(rename = "host_left")]
    HostLeft,
    #[serde(rename = "error")]
    Error { message: String },
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mobilecli_relay=info".parse().unwrap())
        )
        .init();

    let state = Arc::new(RelayState::new());
    let addr = "0.0.0.0:8080";

    // Start cleanup task
    let cleanup_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            cleanup_state.cleanup();
        }
    });

    let listener = TcpListener::bind(addr).await.expect("Failed to bind");

    info!("═══════════════════════════════════════════════════════════");
    info!("  MobileCLI Relay Server v0.1.0");
    info!("  Listening on {}", addr);
    info!("═══════════════════════════════════════════════════════════");
    info!("  Security: End-to-end encrypted (relay sees only blobs)");
    info!("  Room expiry: {} min (no client) / {} min (idle)",
          ROOM_EXPIRY_NO_CLIENT.as_secs() / 60,
          ROOM_EXPIRY_IDLE.as_secs() / 60);
    info!("  Rate limit: {} rooms/IP/minute", RATE_LIMIT_MAX_ROOMS);
    info!("═══════════════════════════════════════════════════════════");
    info!("  Endpoints:");
    info!("    /host       - Desktop creates encrypted room");
    info!("    /join/CODE  - Mobile joins with room code");
    info!("    /health     - Health check");
    info!("    /stats      - Connection statistics");
    info!("═══════════════════════════════════════════════════════════");

    while let Ok((stream, addr)) = listener.accept().await {
        let state = Arc::clone(&state);
        state.total_connections.fetch_add(1, Ordering::Relaxed);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, state, addr.ip()).await {
                debug!("Connection ended from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    state: Arc<RelayState>,
    client_ip: IpAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Peek at the HTTP request to determine the path
    let mut buf = [0u8; 1024];
    let n = stream.peek(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the path from the HTTP request
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    debug!("Request from {}: {}", client_ip, path);

    // Handle non-WebSocket endpoints
    if path == "/health" {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nok";
        use tokio::io::AsyncWriteExt;
        let mut stream = stream;
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    if path == "/stats" {
        let stats = serde_json::json!({
            "version": "0.1.0",
            "security": "e2e_encrypted",
            "active_rooms": state.rooms.len(),
            "total_rooms_created": state.total_rooms_created.load(Ordering::Relaxed),
            "total_connections": state.total_connections.load(Ordering::Relaxed),
        });
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
            stats
        );
        use tokio::io::AsyncWriteExt;
        let mut stream = stream;
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    // Upgrade to WebSocket
    let ws_stream = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    if path == "/host" {
        // Check rate limit
        if !state.check_rate_limit(client_ip) {
            let msg = serde_json::to_string(&RelayMessage::Error {
                message: "Rate limit exceeded. Try again later.".to_string(),
            })?;
            ws_sender.send(Message::Text(msg)).await?;
            return Ok(());
        }

        // Desktop client - create a new room
        let code = RelayState::generate_code();
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Store the room
        state.rooms.insert(code.clone(), Room {
            host_tx: tx,
            client_tx: None,
            created_at: Instant::now(),
            client_joined: false,
        });

        state.total_rooms_created.fetch_add(1, Ordering::Relaxed);
        info!("Room created: {} (from {})", code, client_ip);

        // Send room code to host
        let msg = serde_json::to_string(&RelayMessage::RoomCreated { code: code.clone() })?;
        ws_sender.send(Message::Text(msg)).await?;

        // Handle messages (all encrypted - we just forward blobs)
        loop {
            tokio::select! {
                // Message from host's WebSocket
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Forward encrypted blob to client if connected
                            if let Some(room) = state.rooms.get(&code) {
                                if let Some(client_tx) = &room.client_tx {
                                    let _ = client_tx.send(Message::Text(text));
                                }
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            // Binary messages (encrypted data)
                            if let Some(room) = state.rooms.get(&code) {
                                if let Some(client_tx) = &room.client_tx {
                                    let _ = client_tx.send(Message::Binary(data));
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            ws_sender.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            info!("Host disconnected, closing room: {}", code);
                            // Notify client if connected
                            if let Some(room) = state.rooms.get(&code) {
                                if let Some(client_tx) = &room.client_tx {
                                    let msg = serde_json::to_string(&RelayMessage::HostLeft)?;
                                    let _ = client_tx.send(Message::Text(msg));
                                }
                            }
                            state.rooms.remove(&code);
                            break;
                        }
                        _ => {}
                    }
                }
                // Message to send to host (from client via channel)
                msg = rx.recv() => {
                    match msg {
                        Some(msg) => {
                            if ws_sender.send(msg).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    } else if path.starts_with("/join/") {
        // Mobile client - join existing room
        let code = path.trim_start_matches("/join/").to_uppercase();

        if code.len() != 6 {
            let msg = serde_json::to_string(&RelayMessage::Error {
                message: "Invalid room code".to_string(),
            })?;
            ws_sender.send(Message::Text(msg)).await?;
            return Ok(());
        }

        // Check if room exists
        if !state.rooms.contains_key(&code) {
            let msg = serde_json::to_string(&RelayMessage::Error {
                message: "Room not found or expired".to_string(),
            })?;
            ws_sender.send(Message::Text(msg)).await?;
            return Ok(());
        }

        let (tx, mut rx) = mpsc::unbounded_channel();

        // Set client_tx in the room
        {
            if let Some(mut room) = state.rooms.get_mut(&code) {
                if room.client_tx.is_some() {
                    let msg = serde_json::to_string(&RelayMessage::Error {
                        message: "Room already has a connected device".to_string(),
                    })?;
                    ws_sender.send(Message::Text(msg)).await?;
                    return Ok(());
                }
                room.client_tx = Some(tx);
                room.client_joined = true;
                // Notify host that client joined
                let msg = serde_json::to_string(&RelayMessage::ClientJoined)?;
                let _ = room.host_tx.send(Message::Text(msg));
            }
        }

        info!("Client joined room: {} (from {})", code, client_ip);

        // Handle messages (all encrypted - we just forward blobs)
        loop {
            tokio::select! {
                // Message from client's WebSocket
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Forward encrypted blob to host
                            if let Some(room) = state.rooms.get(&code) {
                                let _ = room.host_tx.send(Message::Text(text));
                            } else {
                                break; // Room gone
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            // Binary messages (encrypted data)
                            if let Some(room) = state.rooms.get(&code) {
                                let _ = room.host_tx.send(Message::Binary(data));
                            } else {
                                break;
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            ws_sender.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            info!("Client disconnected from room: {}", code);
                            // Notify host
                            if let Some(mut room) = state.rooms.get_mut(&code) {
                                room.client_tx = None;
                                let msg = serde_json::to_string(&RelayMessage::ClientLeft)?;
                                let _ = room.host_tx.send(Message::Text(msg));
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                // Message to send to client (from host via channel)
                msg = rx.recv() => {
                    match msg {
                        Some(msg) => {
                            if ws_sender.send(msg).await.is_err() {
                                // Notify host
                                if let Some(mut room) = state.rooms.get_mut(&code) {
                                    room.client_tx = None;
                                    let msg = serde_json::to_string(&RelayMessage::ClientLeft).unwrap();
                                    let _ = room.host_tx.send(Message::Text(msg));
                                }
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    } else {
        // Unknown path
        let msg = serde_json::to_string(&RelayMessage::Error {
            message: format!("Unknown endpoint: {}. Use /host or /join/CODE", path),
        })?;
        ws_sender.send(Message::Text(msg)).await?;
    }

    Ok(())
}
