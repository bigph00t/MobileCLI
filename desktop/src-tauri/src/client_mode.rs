use crypto_secretbox::{aead::Aead, KeyInit, XSalsa20Poly1305};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    // Request session list from host
    GetSessions,
    // Subscribe to a session's updates
    Subscribe { session_id: String },
    // Unsubscribe from a session
    Unsubscribe { session_id: String },
    // Send input to a session
    SendInput { session_id: String, text: String },
    // Tool approval response
    ToolApproval {
        session_id: String,
        approval_id: String,
        approved: bool,
        always: bool,
    },
    // Ping for keepalive
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    // List of sessions from host
    SessionsList { sessions: Vec<SessionInfo> },
    // Activity update for a session
    ActivityUpdate {
        session_id: String,
        activity: serde_json::Value,
    },
    // Session status changed
    SessionStatus {
        session_id: String,
        status: String,
    },
    // Tool approval request
    ToolApprovalRequest {
        session_id: String,
        approval_id: String,
        tool_name: String,
        params: serde_json::Value,
    },
    // Error from host
    Error { message: String },
    // Pong response
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub project_path: String,
    pub cli_type: String,
    pub status: String,
    pub created_at: String,
    pub last_active_at: String,
}

pub struct ClientConnection {
    encryption_key: [u8; 32],
    room_code: String,
    sender: Option<mpsc::UnboundedSender<String>>,
    connected: Arc<Mutex<bool>>,
}

impl ClientConnection {
    pub fn new(key: [u8; 32], room: String) -> Self {
        Self {
            encryption_key: key,
            room_code: room,
            sender: None,
            connected: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn connect(&mut self, app: AppHandle, relay_url: &str) -> Result<(), String> {
        let url = format!("{}/join/{}", relay_url, self.room_code);
        tracing::info!("Connecting to relay as client: {}", url);

        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        let (write, read) = ws_stream.split();
        let (tx, rx) = mpsc::unbounded_channel();
        self.sender = Some(tx);

        *self.connected.lock().await = true;

        // Emit connected status
        let _ = app.emit("client-status", "connected");

        // Spawn message handlers
        let key = self.encryption_key;
        let connected = self.connected.clone();
        let app_clone = app.clone();

        tokio::spawn(Self::handle_incoming(app_clone, read, key, connected.clone()));
        tokio::spawn(Self::handle_outgoing(write, rx, connected));

        Ok(())
    }

    async fn handle_incoming<S>(
        app: AppHandle,
        mut read: S,
        key: [u8; 32],
        connected: Arc<Mutex<bool>>,
    ) where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        while let Some(result) = read.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    // Try to decrypt the message
                    match Self::decrypt(&text, &key) {
                        Ok(decrypted) => {
                            // Parse and emit the message
                            match serde_json::from_str::<ServerMessage>(&decrypted) {
                                Ok(msg) => {
                                    let _ = app.emit("client-message", &msg);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to parse server message: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to decrypt message: {}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("Client connection closed by server");
                    break;
                }
                Err(e) => {
                    tracing::error!("Client WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        *connected.lock().await = false;
        let _ = app.emit("client-status", "disconnected");
    }

    async fn handle_outgoing<S>(
        mut write: S,
        mut rx: mpsc::UnboundedReceiver<String>,
        connected: Arc<Mutex<bool>>,
    ) where
        S: SinkExt<Message> + Unpin,
        <S as futures_util::Sink<Message>>::Error: std::fmt::Debug,
    {
        while let Some(msg) = rx.recv().await {
            if !*connected.lock().await {
                break;
            }

            if let Err(e) = write.send(Message::Text(msg)).await {
                tracing::error!("Failed to send message: {:?}", e);
                break;
            }
        }
    }

    pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
        let cipher = XSalsa20Poly1305::new(key.into());

        // Generate random nonce (24 bytes for XSalsa20)
        let mut nonce_bytes = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = crypto_secretbox::Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        // Combine nonce + ciphertext and base64 encode
        let mut combined = Vec::with_capacity(24 + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);

        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &combined,
        ))
    }

    pub fn decrypt(ciphertext_b64: &str, key: &[u8; 32]) -> Result<String, String> {
        let combined = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            ciphertext_b64,
        )
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

        if combined.len() < 24 {
            return Err("Ciphertext too short".to_string());
        }

        let (nonce_bytes, ciphertext) = combined.split_at(24);
        let nonce = crypto_secretbox::Nonce::from_slice(nonce_bytes);
        let cipher = XSalsa20Poly1305::new(key.into());

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed: {}", e))?;

        String::from_utf8(plaintext).map_err(|e| format!("UTF-8 decode failed: {}", e))
    }

    pub fn send(&self, message: &ClientMessage) -> Result<(), String> {
        if let Some(tx) = &self.sender {
            let json = serde_json::to_string(message).map_err(|e| e.to_string())?;
            let encrypted = Self::encrypt(&json, &self.encryption_key)?;
            tx.send(encrypted).map_err(|e| e.to_string())
        } else {
            Err("Not connected".to_string())
        }
    }

    pub fn is_connected(&self) -> bool {
        self.sender.is_some()
    }

    pub async fn disconnect(&mut self) {
        self.sender = None;
        *self.connected.lock().await = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = [0u8; 32];
        let plaintext = "Hello, World!";

        let encrypted = ClientConnection::encrypt(plaintext, &key).unwrap();
        let decrypted = ClientConnection::decrypt(&encrypted, &key).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let key = [0u8; 32];
        let plaintext = "Hello, World!";

        let encrypted1 = ClientConnection::encrypt(plaintext, &key).unwrap();
        let encrypted2 = ClientConnection::encrypt(plaintext, &key).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = [0u8; 32];
        let mut key2 = [0u8; 32];
        key2[0] = 1;

        let plaintext = "Hello, World!";
        let encrypted = ClientConnection::encrypt(plaintext, &key1).unwrap();

        let result = ClientConnection::decrypt(&encrypted, &key2);
        assert!(result.is_err());
    }
}
