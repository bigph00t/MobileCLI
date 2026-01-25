// MobileCLI Desktop - Tauri Library Entry Point

mod claude_history;
mod client_mode;
mod codex;
mod codex_watcher;
mod config;
mod db;
mod gemini;
mod gemini_watcher;
mod input_coordinator;
mod jsonl;
mod jsonl_watcher;
mod opencode_watcher;
mod parser;
mod pty;
mod relay;
mod ws;

use base64::Engine;
use client_mode::ClientConnection;
use db::Database;
use input_coordinator::InputCoordinator;
use pty::SessionManager;
use relay::RelayState;
use std::sync::Arc;
use tauri::{Emitter, Listener, Manager};
use tokio::sync::{Mutex, RwLock};

// Application state shared across commands
pub struct AppState {
    pub db: Arc<Database>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub relay_state: Arc<RelayState>,
    pub ws_ready: Arc<std::sync::atomic::AtomicBool>,
    pub client_connection: Arc<Mutex<Option<ClientConnection>>>,
    pub input_coordinator: Arc<InputCoordinator>,
}

/// Extract a user-friendly session name from a project path.
/// Returns the last component of the path (e.g., "/home/user/Desktop" → "Desktop")
/// Falls back to a timestamp-based name if the path is empty or invalid.
fn derive_session_name(project_path: &str) -> String {
    use std::path::Path;

    if project_path.is_empty() {
        return format!("Session {}", chrono::Utc::now().format("%H:%M:%S"));
    }

    // Get the last path component
    Path::new(project_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| format!("Session {}", chrono::Utc::now().format("%H:%M:%S")))
}

fn resolve_home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| {
            let drive = std::env::var("HOMEDRIVE").ok();
            let path = std::env::var("HOMEPATH").ok();
            match (drive, path) {
                (Some(drive), Some(path)) => Ok(format!("{}{}", drive, path)),
                _ => Err(std::env::VarError::NotPresent),
            }
        })
        .unwrap_or_else(|_| ".".to_string())
}

// Tauri commands exposed to frontend
mod commands {
    use super::*;
    use crate::db::{CliType, SessionRecord};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SessionInfo {
        pub id: String,
        pub name: String,
        pub project_path: String,
        pub created_at: String,
        pub last_active_at: String,
        pub status: String,
        pub conversation_id: Option<String>,
        pub cli_type: String,
    }

    impl From<SessionRecord> for SessionInfo {
        fn from(r: SessionRecord) -> Self {
            Self {
                id: r.id,
                name: r.name,
                project_path: r.project_path,
                created_at: r.created_at,
                last_active_at: r.last_active_at,
                status: r.status,
                conversation_id: r.conversation_id,
                cli_type: r.cli_type,
            }
        }
    }

    /// Available CLI types for the frontend
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CliInfo {
        pub id: String,
        pub name: String,
        pub installed: bool,
        pub supports_resume: bool,
    }

    // MessageInfo has been replaced by crate::jsonl::Activity
    // The Activity struct provides a cleaner format read directly from Claude's JSONL logs

    #[derive(Deserialize)]
    pub struct CreateSessionRequest {
        pub project_path: String,
        pub name: Option<String>,
        pub cli_type: Option<String>, // "claude" or "gemini"
    }

    #[tauri::command]
    pub async fn get_sessions(
        state: tauri::State<'_, AppState>,
    ) -> Result<Vec<SessionInfo>, String> {
        state
            .db
            .get_all_sessions()
            .map(|sessions| sessions.into_iter().map(SessionInfo::from).collect())
            .map_err(|e| e.to_string())
    }

    #[tauri::command]
    pub async fn get_session(
        state: tauri::State<'_, AppState>,
        session_id: String,
    ) -> Result<Option<SessionInfo>, String> {
        state
            .db
            .get_session(&session_id)
            .map(|opt| opt.map(SessionInfo::from))
            .map_err(|e| e.to_string())
    }

    #[tauri::command]
    pub async fn create_session(
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
        request: CreateSessionRequest,
    ) -> Result<SessionInfo, String> {
        let name = request
            .name
            .unwrap_or_else(|| derive_session_name(&request.project_path));

        // Parse CLI type (default to Claude)
        let cli_type = request
            .cli_type
            .as_deref()
            .and_then(CliType::from_str)
            .unwrap_or(CliType::ClaudeCode);

        // Create session in database
        let session = state
            .db
            .create_session(&name, &request.project_path, cli_type)
            .map_err(|e| e.to_string())?;

        let session_id = session.id.clone();
        let session_info = SessionInfo::from(session);

        // Emit session-created event to notify WS clients
        let _ = app.emit(
            "session-created",
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

        // Start PTY process
        let mut manager = state.session_manager.write().await;
        manager
            .start_session(
                session_id.clone(),
                request.project_path,
                cli_type,
                state.db.clone(),
                app,
            )
            .await
            .map_err(|e| e.to_string())?;

        Ok(session_info)
    }

    /// Get available CLI types and their installation status
    #[tauri::command]
    pub async fn get_available_clis() -> Result<Vec<CliInfo>, String> {
        use std::process::Command;
        use std::path::{Path, PathBuf};

        // Check if command is installed using multiple methods for cross-platform support
        let check_installed = |cmd: &str| -> bool {
            let home = resolve_home_dir();

            if cfg!(windows) {
                let mut search_dirs: Vec<PathBuf> = Vec::new();
                if !home.is_empty() {
                    search_dirs.push(Path::new(&home).join("AppData").join("Roaming").join("npm"));
                    search_dirs.push(Path::new(&home).join(".npm-global").join("bin"));
                    search_dirs.push(Path::new(&home).join(".yarn").join("bin"));
                    search_dirs.push(Path::new(&home).join(".bun").join("bin"));
                    search_dirs.push(Path::new(&home).join("scoop").join("shims"));
                }

                if let Ok(path_env) = std::env::var("PATH") {
                    for entry in path_env.split(';') {
                        if !entry.trim().is_empty() {
                            search_dirs.push(PathBuf::from(entry));
                        }
                    }
                }

                let extensions = ["", ".exe", ".cmd", ".bat"];
                for dir in search_dirs {
                    for ext in extensions {
                        let candidate = dir.join(format!("{}{}", cmd, ext));
                        if candidate.exists() {
                            tracing::debug!("Found {} at path: {}", cmd, candidate.display());
                            return true;
                        }
                    }
                }

                tracing::debug!("CLI {} not found on PATH (Windows check)", cmd);
                return false;
            }

            // Method 1: Check common installation paths directly (fastest, most reliable)
            let common_paths = [
                format!("{home}/.nvm/versions/node/*/bin/{cmd}"),
                format!("{home}/.local/bin/{cmd}"),
                format!("{home}/.npm-global/bin/{cmd}"),
                format!("{home}/.yarn/bin/{cmd}"),
                format!("{home}/.bun/bin/{cmd}"),
                format!("/usr/local/bin/{cmd}"),
                format!("/usr/bin/{cmd}"),
                format!("/opt/homebrew/bin/{cmd}"),
            ];

            for pattern in &common_paths {
                if let Ok(mut paths) = glob::glob(pattern) {
                    if paths.next().is_some() {
                        tracing::debug!("Found {} via glob: {}", cmd, pattern);
                        return true;
                    }
                }
            }

            // Method 2: Check if it's a direct path (non-glob patterns)
            let direct_paths = [
                format!("{home}/.local/bin/{cmd}"),
                format!("{home}/.npm-global/bin/{cmd}"),
                format!("/usr/local/bin/{cmd}"),
                format!("/usr/bin/{cmd}"),
            ];

            for path in &direct_paths {
                if Path::new(path).exists() {
                    tracing::debug!("Found {} at path: {}", cmd, path);
                    return true;
                }
            }

            // Method 3: Try interactive bash shell (sources .bashrc which sets up nvm)
            let bash_check = Command::new("bash")
                .args(["-ic", &format!("which {} >/dev/null 2>&1", cmd)])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if bash_check {
                tracing::debug!("Found {} via bash -ic", cmd);
                return true;
            }

            // Method 4: Try zsh interactive shell (macOS default)
            let zsh_check = Command::new("zsh")
                .args(["-ic", &format!("which {} >/dev/null 2>&1", cmd)])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if zsh_check {
                tracing::debug!("Found {} via zsh -ic", cmd);
                return true;
            }

            // Method 5: Try login shells as last resort
            let bash_login = Command::new("bash")
                .args(["-lc", &format!("which {} >/dev/null 2>&1", cmd)])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if bash_login {
                tracing::debug!("Found {} via bash -lc", cmd);
                return true;
            }

            tracing::debug!("CLI {} not found by any method", cmd);
            false
        };

        Ok(vec![
            CliInfo {
                id: "claude".to_string(),
                name: "Claude Code".to_string(),
                installed: check_installed("claude"),
                supports_resume: true,
            },
            CliInfo {
                id: "gemini".to_string(),
                name: "Gemini CLI".to_string(),
                installed: check_installed("gemini"),
                supports_resume: true,
            },
            CliInfo {
                id: "opencode".to_string(),
                name: "OpenCode".to_string(),
                installed: check_installed("opencode"),
                supports_resume: true,
            },
            CliInfo {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                installed: check_installed("codex"),
                supports_resume: true,
            },
        ])
    }

    #[tauri::command]
    pub async fn send_input(
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
        session_id: String,
        input: String,
    ) -> Result<(), String> {
        // JSONL Redesign: User messages are written to JSONL by Claude when sent to PTY
        // No need to store in DB anymore - JSONL is the source of truth

        // Broadcast to WS clients (mobile)
        tracing::info!(
            "[lib.rs] Emitting new-message for user input: session={}, content={}",
            session_id,
            &input
        );
        let _ = app.emit(
            "new-message",
            serde_json::json!({
                "sessionId": session_id,
                "role": "user",
                "content": input,
                "isComplete": true,
            }),
        );

        // Send to PTY
        let manager = state.session_manager.read().await;
        manager
            .send_input(&session_id, &input)
            .await
            .map_err(|e| e.to_string())
    }

    #[tauri::command]
    pub async fn send_raw_input(
        app: tauri::AppHandle,
        state: tauri::State<'_, AppState>,
        session_id: String,
        input: String,
    ) -> Result<(), String> {
        let manager = state.session_manager.read().await;
        manager
            .send_raw_input(&session_id, &input)
            .await
            .map_err(|e| e.to_string())?;

        // CRITICAL FIX: If input looks like a tool approval response (1, 2, 3, y, n),
        // emit waiting-cleared so mobile dismisses its modal immediately.
        // This catches when desktop user presses a single digit/key to approve a tool.
        let trimmed = input.trim();
        if trimmed == "1"
            || trimmed == "2"
            || trimmed == "3"
            || trimmed.eq_ignore_ascii_case("y")
            || trimmed.eq_ignore_ascii_case("n")
        {
            tracing::info!(
                "Tool approval response detected: {:?} - emitting waiting-cleared",
                trimmed
            );
            let _ = app.emit(
                "waiting-cleared",
                serde_json::json!({
                    "sessionId": session_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "response": trimmed,
                }),
            );
        }

        Ok(())
    }

    /// Send a tool approval response to a session
    /// This handles CLI-specific approval input (numbered options, y/n, arrow keys)
    #[tauri::command]
    pub async fn send_tool_approval(
        app: tauri::AppHandle,
        state: tauri::State<'_, AppState>,
        session_id: String,
        response: crate::db::ApprovalResponse,
    ) -> Result<(), String> {
        // Get session to determine CLI type
        let session = state
            .db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session not found: {}", session_id))?;

        // Parse CLI type
        let cli_type = crate::db::CliType::from_str(&session.cli_type)
            .ok_or_else(|| format!("Unknown CLI type: {}", session.cli_type))?;

        // Get the appropriate input string for this CLI's approval model
        let input = response.get_input_for_cli(cli_type);

        tracing::info!(
            "Sending tool approval to session {}: {:?} -> {:?}",
            session_id,
            response,
            input.as_bytes()
        );

        // Send to PTY as raw input
        let manager = state.session_manager.read().await;
        manager
            .send_raw_input(&session_id, input)
            .await
            .map_err(|e| e.to_string())?;

        // CRITICAL FIX: Emit waiting-cleared event so mobile dismisses its modal
        let _ = app.emit(
            "waiting-cleared",
            serde_json::json!({
                "sessionId": session_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "response": input,
            }),
        );
        tracing::info!("Emitted waiting-cleared for session {}", session_id);

        Ok(())
    }

    #[tauri::command]
    pub async fn resize_pty(
        state: tauri::State<'_, AppState>,
        session_id: String,
        rows: u16,
        cols: u16,
    ) -> Result<(), String> {
        let manager = state.session_manager.read().await;
        manager
            .resize(&session_id, rows, cols)
            .map_err(|e| e.to_string())
    }

    #[tauri::command]
    pub async fn close_session(
        state: tauri::State<'_, AppState>,
        session_id: String,
    ) -> Result<(), String> {
        // Update database
        state
            .db
            .update_session_status(&session_id, "closed")
            .map_err(|e| e.to_string())?;

        // Stop PTY process
        let mut manager = state.session_manager.write().await;
        manager.stop_session(&session_id).await;

        Ok(())
    }

    #[tauri::command]
    pub async fn rename_session(
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
        session_id: String,
        new_name: String,
    ) -> Result<(), String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Session name cannot be empty".to_string());
        }
        let name = trimmed.to_string();

        state
            .db
            .rename_session(&session_id, &name)
            .map_err(|e| e.to_string())?;

        let _ = app.emit(
            "session-renamed",
            serde_json::json!({ "sessionId": session_id, "newName": name }),
        );

        Ok(())
    }

    /// Delete a session from the database
    ///
    /// This removes the session and its messages from the local database.
    /// Note: This does NOT delete Claude's JSONL files in ~/.claude/projects/.
    #[tauri::command]
    pub async fn delete_session(
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
        session_id: String,
    ) -> Result<(), String> {
        // Make sure the session is not active
        let session = state
            .db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Session not found".to_string())?;

        if session.status == "active" {
            return Err("Cannot delete an active session. Close it first.".to_string());
        }

        // Delete from database
        state
            .db
            .delete_session(&session_id)
            .map_err(|e| e.to_string())?;

        // Emit event for UI updates
        let _ = app.emit("session-deleted", serde_json::json!({ "sessionId": session_id }));

        tracing::info!("Deleted session: {}", session_id);
        Ok(())
    }

    /// ISSUE #1: Create a new directory at the specified path
    /// Used by the desktop frontend to create folders during new session setup
    #[tauri::command]
    pub async fn create_directory(path: String) -> Result<(), String> {
        use std::path::Path;

        let dir_path = Path::new(&path);

        // Validate: path must be absolute
        if !dir_path.is_absolute() {
            return Err("Path must be absolute".to_string());
        }

        // Validate: parent must exist
        if let Some(parent) = dir_path.parent() {
            if !parent.exists() {
                return Err(format!("Parent directory does not exist: {}", parent.display()));
            }
        }

        // Validate: path must not already exist
        if dir_path.exists() {
            return Err(format!("Directory already exists: {}", path));
        }

        // Create the directory
        std::fs::create_dir(&path).map_err(|e| format!("Failed to create directory: {}", e))?;

        tracing::info!("Created directory: {}", path);
        Ok(())
    }

    /// Get messages/activities for a session
    ///
    /// For Claude sessions, reads from Claude's JSONL logs for clean, structured data.
    /// Falls back to database for other CLI types or if JSONL is not available.
    #[tauri::command]
    pub async fn get_messages(
        state: tauri::State<'_, AppState>,
        session_id: String,
        limit: Option<i64>,
    ) -> Result<Vec<crate::jsonl::Activity>, String> {
        // Get session info to get conversation_id and project_path
        let session = state
            .db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session not found: {}", session_id))?;

        // Check if this is a Claude session with JSONL available
        if session.cli_type == "claude" {
            if let Some(ref conversation_id) = session.conversation_id {
                // Try to read from Claude's JSONL file
                let jsonl_path =
                    crate::jsonl::get_jsonl_path(&session.project_path, conversation_id);

                if jsonl_path.exists() {
                    tracing::info!(
                        "Reading messages from JSONL for session {}: {:?}",
                        session_id,
                        jsonl_path
                    );

                    match crate::jsonl::read_activities(&session.project_path, conversation_id) {
                        Ok(activities) => {
                            // Apply limit if specified
                            let limited = if let Some(lim) = limit {
                                activities.into_iter().take(lim as usize).collect()
                            } else {
                                activities
                            };
                            return Ok(limited);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to read JSONL for session {}, falling back to DB: {}",
                                session_id,
                                e
                            );
                            // Fall through to database fallback
                        }
                    }
                } else {
                    tracing::info!(
                        "JSONL file not found for session {}, conversation_id: {}",
                        session_id,
                        conversation_id
                    );
                }
            }
        }

        // Fallback: read from database and convert to Activity format
        // This handles non-Claude CLIs and sessions without JSONL
        let messages = state
            .db
            .get_messages(&session_id, limit.unwrap_or(100))
            .map_err(|e| e.to_string())?;

        // Convert MessageRecord to Activity
        let activities: Vec<crate::jsonl::Activity> = messages
            .into_iter()
            .map(|m| {
                let activity_type = match m.role.as_str() {
                    "user" => crate::parser::ActivityType::UserPrompt,
                    "assistant" => crate::parser::ActivityType::Text,
                    _ => crate::parser::ActivityType::Text,
                };

                crate::jsonl::Activity {
                    activity_type,
                    content: m.content,
                    tool_name: m.tool_name,
                    tool_params: None,
                    file_path: None,
                    is_streaming: false,
                    timestamp: m.timestamp,
                    uuid: None,
                    summary: None,
                }
            })
            .collect();

        Ok(activities)
    }

    #[tauri::command]
    pub async fn get_ws_port() -> Result<u16, String> {
        // Return the WebSocket server port
        Ok(ws::WS_PORT)
    }

    #[tauri::command]
    pub async fn is_ws_ready(state: tauri::State<'_, AppState>) -> Result<bool, String> {
        Ok(state.ws_ready.load(std::sync::atomic::Ordering::SeqCst))
    }

    #[tauri::command]
    pub async fn is_session_active(
        state: tauri::State<'_, AppState>,
        session_id: String,
    ) -> Result<bool, String> {
        let manager = state.session_manager.read().await;
        Ok(manager.is_session_active(&session_id))
    }

    #[tauri::command]
    pub async fn update_conversation_id(
        state: tauri::State<'_, AppState>,
        session_id: String,
        conversation_id: String,
    ) -> Result<(), String> {
        state
            .db
            .update_conversation_id(&session_id, &conversation_id)
            .map_err(|e| e.to_string())
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ClaudeMessage {
        pub role: String,
        pub content: String,
        pub timestamp: Option<String>,
    }

    #[tauri::command]
    pub async fn get_claude_history(
        project_path: String,
        conversation_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<ClaudeMessage>, String> {
        let messages = crate::claude_history::read_conversation_history(
            &project_path,
            &conversation_id,
            limit.unwrap_or(50),
        )?;

        Ok(messages
            .into_iter()
            .map(|m| ClaudeMessage {
                role: m.role,
                content: m.content,
                timestamp: m.timestamp,
            })
            .collect())
    }

    #[tauri::command]
    pub async fn resume_session(
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
        session_id: String,
    ) -> Result<SessionInfo, String> {
        // Get session from database
        let session = state
            .db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Session not found".to_string())?;

        // Parse CLI type
        let cli_type = CliType::from_str(&session.cli_type)
            .ok_or_else(|| format!("Unknown CLI type: {}", session.cli_type))?;

        // Check if this CLI supports resume
        if !cli_type.supports_resume() {
            return Err(format!(
                "{} does not support session resume",
                cli_type.display_name()
            ));
        }

        // Check if session has a conversation_id to resume
        let conversation_id = session
            .conversation_id
            .clone()
            .ok_or_else(|| "Session has no conversation ID to resume".to_string())?;

        // Update session status to active
        state
            .db
            .update_session_status(&session_id, "active")
            .map_err(|e| e.to_string())?;

        // Start PTY process with resume flag
        // Desktop resume uses config setting (None means use config default)
        let mut manager = state.session_manager.write().await;
        manager
            .resume_session(
                session_id.clone(),
                session.project_path.clone(),
                conversation_id,
                cli_type,
                state.db.clone(),
                app,
                None, // Use config default for desktop resume
            )
            .await
            .map_err(|e| e.to_string())?;

        // Get updated session
        let updated_session = state
            .db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Session not found after resume".to_string())?;

        Ok(SessionInfo::from(updated_session))
    }

    #[tauri::command]
    pub fn get_local_ip() -> Result<String, String> {
        // Try to get the local IP address
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            // Connect to a public IP (doesn't actually send anything)
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    return Ok(addr.ip().to_string());
                }
            }
        }

        // Fallback: try to get from network interfaces
        if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
            for (name, ip) in interfaces {
                // Skip loopback and docker interfaces
                if !name.starts_with("lo")
                    && !name.starts_with("docker")
                    && !name.starts_with("br-")
                    && !name.starts_with("veth")
                {
                    if let std::net::IpAddr::V4(ipv4) = ip {
                        if !ipv4.is_loopback() {
                            return Ok(ipv4.to_string());
                        }
                    }
                }
            }
        }

        Ok("localhost".to_string())
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct TailscaleStatus {
        pub installed: bool,
        pub running: bool,
        pub tailscale_ip: Option<String>,
        pub hostname: Option<String>,
        pub ws_url: Option<String>,
    }

    #[derive(Deserialize)]
    struct TailscaleStatusJson {
        #[serde(rename = "Self")]
        self_info: Option<TailscaleSelf>,
    }

    #[derive(Deserialize)]
    struct TailscaleSelf {
        #[serde(rename = "HostName")]
        hostname: Option<String>,
    }

    #[tauri::command]
    pub async fn get_tailscale_status() -> Result<TailscaleStatus, String> {
        use std::process::Command;

        let ws_port = ws::WS_PORT;

        let ip_output = Command::new("tailscale").args(["ip", "-4"]).output();
        let ip_output = match ip_output {
            Ok(output) if output.status.success() => output,
            Ok(_) => {
                return Ok(TailscaleStatus {
                    installed: true,
                    running: false,
                    tailscale_ip: None,
                    hostname: None,
                    ws_url: None,
                });
            }
            Err(_) => {
                return Ok(TailscaleStatus {
                    installed: false,
                    running: false,
                    tailscale_ip: None,
                    hostname: None,
                    ws_url: None,
                });
            }
        };

        let ip_output = String::from_utf8_lossy(&ip_output.stdout);
        let tailscale_ip = ip_output
            .lines()
            .next()
            .map(|line| line.trim().to_string())
            .filter(|ip| !ip.is_empty());

        let hostname = Command::new("tailscale")
            .args(["status", "--json"])
            .output()
            .ok()
            .and_then(|output| serde_json::from_slice::<TailscaleStatusJson>(&output.stdout).ok())
            .and_then(|status| status.self_info.and_then(|info| info.hostname));

        let ws_url = tailscale_ip
            .as_ref()
            .map(|ip| format!("ws://{}:{}", ip, ws_port));

        Ok(TailscaleStatus {
            installed: true,
            running: tailscale_ip.is_some(),
            tailscale_ip,
            hostname,
            ws_url,
        })
    }

    // ========== RELAY COMMANDS (Remote Access with E2E Encryption) ==========

    /// Start relay connection and get QR code data
    #[tauri::command]
    pub async fn start_relay(
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
    ) -> Result<crate::relay::RelayQrData, String> {
        crate::relay::start_relay(app, state.relay_state.clone(), state.db.clone()).await
    }

    /// Get current relay status
    #[tauri::command]
    pub async fn get_relay_status(
        state: tauri::State<'_, AppState>,
    ) -> Result<Option<crate::relay::RelayQrData>, String> {
        Ok(crate::relay::get_relay_status(state.relay_state.clone()).await)
    }

    /// Stop relay connection
    #[tauri::command]
    pub async fn stop_relay(state: tauri::State<'_, AppState>) -> Result<(), String> {
        crate::relay::stop_relay(state.relay_state.clone()).await;
        Ok(())
    }

    // ========== APP INFO COMMANDS ==========

    /// Get application version
    #[tauri::command]
    pub fn get_version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    // ========== CONFIG COMMANDS (Persistent Configuration) ==========

    /// Get current application configuration
    #[tauri::command]
    pub fn get_config(app: tauri::AppHandle) -> Result<crate::config::AppConfig, String> {
        crate::config::load_config(&app)
    }

    /// Save application configuration
    #[tauri::command]
    pub fn set_config(
        app: tauri::AppHandle,
        config: crate::config::AppConfig,
    ) -> Result<(), String> {
        crate::config::save_config(&app, &config)
    }

    /// Check if this is the first run of the application
    #[tauri::command]
    pub fn is_first_run(app: tauri::AppHandle) -> Result<bool, String> {
        let config = crate::config::load_config(&app)?;
        Ok(config.first_run)
    }

    /// Mark first run as complete
    #[tauri::command]
    pub fn set_first_run_complete(app: tauri::AppHandle) -> Result<(), String> {
        let mut config = crate::config::load_config(&app)?;
        config.first_run = false;
        crate::config::save_config(&app, &config)
    }

    /// Get current app mode (host or client)
    #[tauri::command]
    pub fn get_app_mode(app: tauri::AppHandle) -> Result<crate::config::AppMode, String> {
        let config = crate::config::load_config(&app)?;
        Ok(config.mode)
    }

    /// Set app mode (host or client)
    #[tauri::command]
    pub fn set_app_mode(app: tauri::AppHandle, mode: crate::config::AppMode) -> Result<(), String> {
        let mut config = crate::config::load_config(&app)?;
        config.mode = mode;
        crate::config::save_config(&app, &config)
    }

    // ============================================
    // Client Mode Commands (Desktop as client)
    // ============================================

    /// Connect to a host as a client via relay
    #[tauri::command]
    pub async fn connect_as_client(
        app: tauri::AppHandle,
        state: tauri::State<'_, AppState>,
        relay_url: String,
        room_code: String,
        key: String, // base64 encoded encryption key
    ) -> Result<(), String> {
        use base64::Engine;

        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&key)
            .map_err(|e| format!("Invalid key: {}", e))?;

        let key: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| "Key must be 32 bytes".to_string())?;

        let mut client = crate::client_mode::ClientConnection::new(key, room_code);
        client.connect(app, &relay_url).await?;

        let mut conn = state.client_connection.lock().await;
        *conn = Some(client);

        Ok(())
    }

    /// Disconnect from host
    #[tauri::command]
    pub async fn disconnect_client(state: tauri::State<'_, AppState>) -> Result<(), String> {
        let mut conn = state.client_connection.lock().await;
        if let Some(ref mut client) = *conn {
            client.disconnect().await;
        }
        *conn = None;
        Ok(())
    }

    /// Check if connected as client
    #[tauri::command]
    pub async fn is_client_connected(state: tauri::State<'_, AppState>) -> Result<bool, String> {
        let conn = state.client_connection.lock().await;
        Ok(conn.as_ref().map(|c| c.is_connected()).unwrap_or(false))
    }

    /// Send a message to the host
    #[tauri::command]
    pub async fn send_client_message(
        state: tauri::State<'_, AppState>,
        message: crate::client_mode::ClientMessage,
    ) -> Result<(), String> {
        let conn = state.client_connection.lock().await;
        if let Some(client) = conn.as_ref() {
            client.send(&message)
        } else {
            Err("Not connected".to_string())
        }
    }

    /// Request sessions list from host
    #[tauri::command]
    pub async fn request_sessions_from_host(
        state: tauri::State<'_, AppState>,
    ) -> Result<(), String> {
        let conn = state.client_connection.lock().await;
        if let Some(client) = conn.as_ref() {
            client.send(&crate::client_mode::ClientMessage::GetSessions)
        } else {
            Err("Not connected".to_string())
        }
    }

    /// Subscribe to a session's updates
    #[tauri::command]
    pub async fn subscribe_to_session(
        state: tauri::State<'_, AppState>,
        session_id: String,
    ) -> Result<(), String> {
        let conn = state.client_connection.lock().await;
        if let Some(client) = conn.as_ref() {
            client.send(&crate::client_mode::ClientMessage::Subscribe { session_id })
        } else {
            Err("Not connected".to_string())
        }
    }

    /// Send input to a session on the host
    #[tauri::command]
    pub async fn send_input_to_host(
        state: tauri::State<'_, AppState>,
        session_id: String,
        text: String,
    ) -> Result<(), String> {
        let conn = state.client_connection.lock().await;
        if let Some(client) = conn.as_ref() {
            client.send(&crate::client_mode::ClientMessage::SendInput { session_id, text })
        } else {
            Err("Not connected".to_string())
        }
    }

    /// Send tool approval to the host
    #[tauri::command]
    pub async fn send_tool_approval_to_host(
        state: tauri::State<'_, AppState>,
        session_id: String,
        approval_id: String,
        approved: bool,
        always: bool,
    ) -> Result<(), String> {
        let conn = state.client_connection.lock().await;
        if let Some(client) = conn.as_ref() {
            client.send(&crate::client_mode::ClientMessage::ToolApproval {
                session_id,
                approval_id,
                approved,
                always,
            })
        } else {
            Err("Not connected".to_string())
        }
    }
}

/// Setup global panic handler for better error reporting
fn setup_panic_handler() {
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        tracing::error!("PANIC at {}: {}", location, msg);

        // Log backtrace if available
        let backtrace = std::backtrace::Backtrace::capture();
        if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            tracing::error!("Backtrace:\n{}", backtrace);
        }
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing first
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mobilecli=debug".parse().unwrap()),
        )
        .init();

    // Setup panic handler immediately after tracing
    setup_panic_handler();

    tracing::info!("Starting MobileCLI desktop app");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            // Initialize database
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data dir");

            let db_path = app_data_dir.join("mobilecli.db");
            let db = Arc::new(Database::new(&db_path).expect("Failed to initialize database"));

            // Mark all "active" sessions as "closed" since their PTY died when app closed
            // This prevents showing black/empty terminals for orphaned sessions
            if let Err(e) = db.close_all_active_sessions() {
                tracing::warn!("Failed to close orphaned sessions: {}", e);
            } else {
                tracing::info!("Closed orphaned sessions from previous run");
            }

            // Initialize session manager
            let session_manager = Arc::new(RwLock::new(SessionManager::new()));

            // Initialize relay state
            let relay_state = Arc::new(RelayState::new());

            // Initialize WS ready flag
            let ws_ready = Arc::new(std::sync::atomic::AtomicBool::new(false));

            // Clone session_manager before moving into AppState
            let session_manager_for_input = session_manager.clone();
            let session_manager_for_create = session_manager.clone();
            let session_manager_for_resume = session_manager.clone();
            let session_manager_for_close = session_manager.clone();
            // Additional clones for relay-specific handlers
            let session_manager_for_relay_create = session_manager.clone();
            let session_manager_for_relay_resume = session_manager.clone();
            let session_manager_for_resize = session_manager.clone();
            let session_manager_for_history = session_manager.clone();

            // Store state
            // 500ms debounce between different input senders to prevent race conditions
            let input_coordinator = Arc::new(InputCoordinator::new(500));
            let input_coordinator_for_handler = input_coordinator.clone();
            app.manage(AppState {
                db: db.clone(),
                session_manager,
                relay_state,
                ws_ready: ws_ready.clone(),
                client_connection: Arc::new(Mutex::new(None)),
                input_coordinator: input_coordinator.clone(),
            });

            // Start WebSocket server with ready signal
            let app_handle = app.handle().clone();
            let db_clone = db.clone();
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

            tauri::async_runtime::spawn(async move {
                if let Err(e) = ws::start_server(app_handle, db_clone, Some(ready_tx)).await {
                    tracing::error!("WebSocket server error: {}", e);
                }
            });

            // Wait for WebSocket server to be ready (with timeout)
            let app_handle_ready = app.handle().clone();
            let ws_ready_clone = ws_ready.clone();
            tauri::async_runtime::spawn(async move {
                match tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx).await {
                    Ok(Ok(())) => {
                        tracing::info!("WebSocket server is ready on port {}", ws::WS_PORT);
                        ws_ready_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    Ok(Err(_)) => {
                        tracing::error!("WebSocket server ready channel closed unexpectedly");
                        let _ = app_handle_ready.emit("ws-server-error", serde_json::json!({
                            "error": "Server startup failed"
                        }));
                    }
                    Err(_) => {
                        tracing::error!("Timeout waiting for WebSocket server to start");
                        let _ = app_handle_ready.emit("ws-server-error", serde_json::json!({
                            "error": "Server startup timeout"
                        }));
                    }
                }
            });

            // Background task to process queued inputs (from debounce)
            let input_coordinator_for_queue = input_coordinator.clone();
            let session_manager_for_queue = session_manager_for_input.clone();
            let app_for_queue = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    // Check queue every 500ms (matching debounce time)
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                    let ready = input_coordinator_for_queue.process_queue().await;
                    if !ready.is_empty() {
                        tracing::info!("Processing {} queued inputs", ready.len());
                    }

                    for input in ready {
                        let mgr = session_manager_for_queue.read().await;

                        // NOTE: Removed is_session_active pre-check for consistency with
                        // the main send-input handler. Let the PTY send fail naturally
                        // with an appropriate error message if the session doesn't exist.

                        // Send the queued input
                        tracing::info!(
                            "Executing queued input from {} for session {}",
                            input.sender_id,
                            input.session_id
                        );

                        if let Err(e) = mgr.send_input(&input.session_id, &input.text).await {
                            tracing::error!(
                                "Failed to send queued input to session {}: {}",
                                input.session_id,
                                e
                            );
                            let _ = app_for_queue.emit(
                                "input-error",
                                serde_json::json!({
                                    "sessionId": input.session_id,
                                    "error": e.to_string(),
                                }),
                            );
                        }
                    }
                }
            });

            // Listen for send-input events from WebSocket (mobile client)
            let app_for_input = app.handle().clone();
            app.listen("send-input", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
                    let text = payload["text"].as_str().unwrap_or("").to_string();
                    let raw = payload["raw"].as_bool().unwrap_or(false);
                    let sender_id = payload["senderId"].as_str().unwrap_or("local").to_string();

                    let text_bytes: Vec<u8> = text.bytes().collect();
                    tracing::info!(
                        "EVENT send-input: session={}, sender={}, text={:?}, text_hex={:02x?}, raw={}",
                        session_id, sender_id, text, text_bytes, raw
                    );

                    // Allow empty text for raw mode (used for Enter key / auto-accept trust prompts)
                    // Regular (non-raw) mode still requires non-empty text
                    if !session_id.is_empty() && (raw || !text.is_empty()) {
                        let manager = session_manager_for_input.clone();
                        let coordinator = input_coordinator_for_handler.clone();
                        let app = app_for_input.clone();
                        let sid = session_id.clone();
                        let txt = text.clone();
                        let sender = sender_id.clone();
                        tauri::async_runtime::spawn(async move {
                            let mgr = manager.read().await;

                            // NOTE: We removed the is_session_active pre-check here.
                            // Previously, mobile input was blocked by this check while desktop
                            // (using send_raw_input command) bypassed it. This caused mobile
                            // input to fail for valid sessions. Now mobile is consistent with
                            // desktop - we attempt the send and handle errors downstream.
                            // The PTY send methods return appropriate errors if session doesn't exist.

                            // Emit typing indicator for all connected clients
                            let _ = app.emit(
                                "input-state",
                                serde_json::json!({
                                    "sessionId": sid,
                                    "typing": true,
                                    "senderId": sender,
                                }),
                            );

                            // Submit to coordinator for debounce handling
                            let pending = input_coordinator::PendingInput {
                                session_id: sid.clone(),
                                text: txt.clone(),
                                sender_id: sender.clone(),
                                timestamp: std::time::Instant::now(),
                            };


                            let can_execute = coordinator.submit_input(pending).await.unwrap_or(false);

                            if can_execute {
                                // Execute input immediately
                                let result = if raw {
                                    mgr.send_raw_input(&sid, &txt).await
                                } else {
                                    mgr.send_input(&sid, &txt).await
                                };
                                if let Err(e) = result {
                                    tracing::error!("Failed to send input to session {}: {}", sid, e);
                                    let _ = app.emit(
                                        "input-error",
                                        serde_json::json!({
                                            "sessionId": sid,
                                            "error": e.to_string(),
                                        }),
                                    );
                                } else {

                                    // CRITICAL FIX: For non-raw sends (mobile complete messages),
                                    // emit an event to clear the desktop frontend's inputBuffer.
                                    // The PTY already sent Ctrl+U to clear the terminal line,
                                    // but we need to sync the frontend's tracking state too.
                                    if !raw {
                                        let _ = app.emit(
                                            "input-state",
                                            serde_json::json!({
                                                "sessionId": sid,
                                                "text": "",
                                                "cursorPosition": 0,
                                                "senderId": "mobile-clear",
                                            }),
                                        );
                                    }
                                }
                            }

                            // Clear typing indicator after delay
                            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                            let _ = app.emit(
                                "input-state",
                                serde_json::json!({
                                    "sessionId": sid,
                                    "typing": false,
                                    "senderId": sender,
                                }),
                            );
                        });
                    } else {
                        tracing::warn!("send-input event with empty session_id or (non-raw) empty text");
                    }
                }
            });

            // Listen for pty-resize events from WebSocket (mobile sends terminal dimensions)
            app.listen("pty-resize", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
                    let cols = payload["cols"].as_u64().unwrap_or(80) as u16;
                    let rows = payload["rows"].as_u64().unwrap_or(24) as u16;

                    if !session_id.is_empty() && cols > 0 && rows > 0 {
                        tracing::info!(
                            "EVENT pty-resize: session={}, cols={}, rows={}",
                            session_id, cols, rows
                        );
                        let manager = session_manager_for_resize.clone();
                        tauri::async_runtime::spawn(async move {
                            let mgr = manager.read().await;
                            if let Err(e) = mgr.resize(&session_id, rows, cols) {
                                tracing::error!("Failed to resize PTY {}: {}", session_id, e);
                            } else {
                                tracing::info!("Successfully resized PTY {} to {}x{}", session_id, cols, rows);
                            }
                        });
                    } else {
                        tracing::warn!("pty-resize event with invalid parameters: session={}, cols={}, rows={}",
                            session_id, cols, rows);
                    }
                }
            });

            // Listen for request-pty-history events from WebSocket (mobile client subscribing)
            // This sends the PTY output history so new subscribers can see recent terminal output
            let app_handle_history = app.handle().clone();
            app.listen("request-pty-history", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();

                    if !session_id.is_empty() {
                        tracing::info!("EVENT request-pty-history: session={}", session_id);
                        let manager = session_manager_for_history.clone();
                        let app = app_handle_history.clone();
                        tauri::async_runtime::spawn(async move {
                            let mgr = manager.read().await;
                            if let Some(history) = mgr.get_output_history(&session_id) {
                                if !history.is_empty() {
                                    // Send history as pty-bytes event (base64 encoded)
                                    let data = base64::engine::general_purpose::STANDARD.encode(&history);
                                    let _ = app.emit(
                                        "pty-bytes",
                                        serde_json::json!({
                                            "sessionId": session_id,
                                            "data": data,
                                        }),
                                    );
                                    tracing::info!(
                                        "Sent {} bytes of PTY history for session {}",
                                        history.len(),
                                        session_id
                                    );
                                }
                            }
                        });
                    }
                }
            });

            // Listen for create-session events from WebSocket (mobile client)
            let db_clone2 = db.clone();
            let app_handle2 = app.handle().clone();
            app.listen("create-session", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
                    let project_path = payload["projectPath"].as_str().unwrap_or("").to_string();
                    let cli_type_str = payload["cliType"].as_str().unwrap_or("claude");

                    // Extract CLI-specific settings from mobile
                    let claude_skip_permissions = payload["claudeSkipPermissions"].as_bool();
                    let codex_approval_policy = payload["codexApprovalPolicy"].as_str().map(|s| s.to_string());

                    let cli_type = db::CliType::from_str(cli_type_str).unwrap_or(db::CliType::ClaudeCode);

                    if !session_id.is_empty() && !project_path.is_empty() {
                        let manager = session_manager_for_create.clone();
                        let db = db_clone2.clone();
                        let app = app_handle2.clone();
                        tauri::async_runtime::spawn(async move {
                            let mut mgr = manager.write().await;
                            // Use start_session_with_settings to pass mobile settings
                            if let Err(e) = mgr.start_session_with_settings(
                                session_id.clone(),
                                project_path,
                                cli_type,
                                db,
                                app,
                                claude_skip_permissions,
                                codex_approval_policy,
                            ).await {
                                tracing::error!("Failed to start session {}: {}", session_id, e);
                            }
                        });
                    }
                }
            });

            // Listen for resume-session events from WebSocket (mobile client)
            let db_clone3 = db.clone();
            let app_handle3 = app.handle().clone();
            app.listen("resume-session", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
                    let project_path = payload["projectPath"].as_str().unwrap_or("").to_string();
                    let conversation_id = payload["conversationId"].as_str().unwrap_or("").to_string();
                    let cli_type_str = payload["cliType"].as_str().unwrap_or("claude");
                    // ISSUE #2: Extract claude_skip_permissions from mobile
                    let claude_skip_permissions = payload["claudeSkipPermissions"].as_bool();

                    let cli_type = db::CliType::from_str(cli_type_str).unwrap_or(db::CliType::ClaudeCode);

                    if !session_id.is_empty() && !project_path.is_empty() && !conversation_id.is_empty() {
                        let manager = session_manager_for_resume.clone();
                        let db = db_clone3.clone();
                        let app = app_handle3.clone();
                        tauri::async_runtime::spawn(async move {
                            let mut mgr = manager.write().await;
                            if let Err(e) = mgr.resume_session(
                                session_id.clone(),
                                project_path,
                                conversation_id,
                                cli_type,
                                db,
                                app,
                                claude_skip_permissions,
                            ).await {
                                tracing::error!("Failed to resume session {}: {}", session_id, e);
                            }
                        });
                    }
                }
            });

            // Listen for close-session events from WebSocket (mobile client)
            let db_clone4 = db.clone();
            let app_handle4 = app.handle().clone();
            app.listen("close-session", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();

                    if !session_id.is_empty() {
                        let manager = session_manager_for_close.clone();
                        let db = db_clone4.clone();
                        let app = app_handle4.clone();
                        tauri::async_runtime::spawn(async move {
                            // Update database status
                            if let Err(e) = db.update_session_status(&session_id, "closed") {
                                tracing::error!("Failed to update session status: {}", e);
                            }

                            // Stop PTY process
                            let mut mgr = manager.write().await;
                            mgr.stop_session(&session_id).await;

                            // Emit session-closed to notify all clients
                            let _ = app.emit(
                                "session-closed",
                                serde_json::json!({
                                    "sessionId": session_id,
                                }),
                            );

                            tracing::info!("Session {} closed via WebSocket", session_id);
                        });
                    }
                }
            });

            // Listen for relay-create-session events (from encrypted relay - no sessionId yet)
            let db_clone5 = db.clone();
            let app_handle5 = app.handle().clone();
            app.listen("relay-create-session", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let project_path = payload["projectPath"].as_str().unwrap_or("").to_string();
                    let name = payload["name"].as_str().map(|s| s.to_string());
                    let cli_type_str = payload["cliType"].as_str().unwrap_or("claude");

                    // Extract CLI-specific settings from relay (mobile)
                    let claude_skip_permissions = payload["claudeSkipPermissions"].as_bool();
                    let codex_approval_policy = payload["codexApprovalPolicy"].as_str().map(|s| s.to_string());

                    let cli_type = db::CliType::from_str(cli_type_str).unwrap_or(db::CliType::ClaudeCode);

                    if !project_path.is_empty() {
                        let manager = session_manager_for_relay_create.clone();
                        let db = db_clone5.clone();
                        let app = app_handle5.clone();
                        // Derive session name before async block (while project_path is still accessible)
                        let session_name = name.unwrap_or_else(|| derive_session_name(&project_path));
                        tauri::async_runtime::spawn(async move {
                            // Create session in DB

                            match db.create_session(&session_name, &project_path, cli_type) {
                                Ok(session) => {
                                    let session_id = session.id.clone();

                                    // Emit session-created so relay can send to mobile
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

                                    // Start PTY process with mobile settings
                                    let mut mgr = manager.write().await;
                                    if let Err(e) = mgr.start_session_with_settings(
                                        session_id.clone(),
                                        project_path,
                                        cli_type,
                                        db,
                                        app,
                                        claude_skip_permissions,
                                        codex_approval_policy,
                                    ).await {
                                        tracing::error!("Failed to start relay session {}: {}", session_id, e);
                                    } else {
                                        tracing::info!("Relay session {} created and started", session_id);
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Failed to create relay session in DB: {}", e);
                                }
                            }
                        });
                    }
                }
            });

            // Listen for relay-resume-session events (from encrypted relay)
            let db_clone6 = db.clone();
            let app_handle6 = app.handle().clone();
            app.listen("relay-resume-session", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    let session_id = payload["sessionId"].as_str().unwrap_or("").to_string();
                    // ISSUE #2: Extract claude_skip_permissions from relay event
                    let claude_skip_permissions = payload["claudeSkipPermissions"].as_bool();

                    if !session_id.is_empty() {
                        let manager = session_manager_for_relay_resume.clone();
                        let db = db_clone6.clone();
                        let app = app_handle6.clone();
                        tauri::async_runtime::spawn(async move {
                            // Get session from DB
                            match db.get_session(&session_id) {
                                Ok(Some(session)) => {
                                    let cli_type = db::CliType::from_str(&session.cli_type)
                                        .unwrap_or(db::CliType::ClaudeCode);

                                    if let Some(conversation_id) = session.conversation_id.clone() {
                                        // Update status to active
                                        let _ = db.update_session_status(&session_id, "active");

                                        // Resume PTY
                                        // ISSUE #2: Use claude_skip_permissions from relay if provided
                                        let mut mgr = manager.write().await;
                                        if let Err(e) = mgr.resume_session(
                                            session_id.clone(),
                                            session.project_path.clone(),
                                            conversation_id,
                                            cli_type,
                                            db,
                                            app.clone(),
                                            claude_skip_permissions,
                                        ).await {
                                            tracing::error!("Failed to resume relay session {}: {}", session_id, e);
                                        } else {
                                            // Emit session-resumed for relay to send to mobile
                                            let _ = app.emit(
                                                "session-resumed",
                                                serde_json::json!({
                                                    "id": session.id,
                                                    "name": session.name,
                                                    "projectPath": session.project_path,
                                                    "createdAt": session.created_at,
                                                    "lastActiveAt": session.last_active_at,
                                                    "status": "active",
                                                    "cliType": session.cli_type,
                                                }),
                                            );
                                            tracing::info!("Relay session {} resumed", session_id);
                                        }
                                    } else {
                                        tracing::error!("Relay session {} has no conversation ID", session_id);
                                    }
                                }
                                Ok(None) => {
                                    tracing::error!("Relay session {} not found", session_id);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to get relay session {}: {}", session_id, e);
                                }
                            }
                        });
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_sessions,
            commands::get_session,
            commands::create_session,
            commands::get_available_clis,
            commands::send_input,
            commands::send_raw_input,
            commands::send_tool_approval,
            commands::resize_pty,
            commands::close_session,
            commands::rename_session,
            commands::delete_session,
            commands::create_directory,
            commands::get_messages,
            commands::get_ws_port,
            commands::is_ws_ready,
            commands::is_session_active,
            commands::update_conversation_id,
            commands::get_claude_history,
            commands::resume_session,
            commands::get_local_ip,
            commands::get_tailscale_status,
            // App info commands
            commands::get_version,
            // Relay commands (E2E encrypted remote access)
            commands::start_relay,
            commands::get_relay_status,
            commands::stop_relay,
            // Config commands (persistent configuration)
            commands::get_config,
            commands::set_config,
            commands::is_first_run,
            commands::set_first_run_complete,
            commands::get_app_mode,
            commands::set_app_mode,
            // Client mode commands (desktop as client)
            commands::connect_as_client,
            commands::disconnect_client,
            commands::is_client_connected,
            commands::send_client_message,
            commands::request_sessions_from_host,
            commands::subscribe_to_session,
            commands::send_input_to_host,
            commands::send_tool_approval_to_host,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
