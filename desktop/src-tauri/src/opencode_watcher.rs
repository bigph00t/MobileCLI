//! OpenCode File Watcher - Real-time updates from OpenCode CLI conversation logs
//!
//! Watches OpenCode's distributed file system for changes and emits activities via Tauri events.
//! OpenCode stores conversations at ~/.local/share/opencode/storage/
//!   - session/<project_hash>/ses_*.json     # Session metadata
//!   - message/ses_<id>/msg_*.json           # Message metadata
//!   - part/msg_<id>/prt_*.json              # Actual text content

use crate::parser::ActivityType;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Get the OpenCode storage directory
pub fn get_opencode_storage_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    PathBuf::from(home)
        .join(".local/share/opencode/storage")
}

/// OpenCode session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSession {
    pub id: String,
    pub slug: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: String,
}

/// OpenCode message metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeMessage {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub role: String,
    pub time: Option<OpenCodeTime>,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(rename = "modelID")]
    pub model_id: Option<String>,
    pub finish: Option<String>,
}

/// OpenCode time fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeTime {
    pub created: Option<u64>,
    pub completed: Option<u64>,
    pub start: Option<u64>,
    pub end: Option<u64>,
}

/// OpenCode part (content) metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodePart {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
    pub tool: Option<String>,
    pub state: Option<OpenCodeToolState>,
}

/// OpenCode tool state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeToolState {
    pub status: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub title: Option<String>,
}

/// Activity representation for OpenCode
#[derive(Debug, Clone)]
pub struct Activity {
    pub activity_type: ActivityType,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_params: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub timestamp: Option<String>,
    pub uuid: Option<String>,
}

/// OpenCode file watcher for a single session
pub struct OpenCodeWatcher {
    /// Flag to signal the watcher should stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the watcher thread
    _watcher_handle: std::thread::JoinHandle<()>,
}

impl OpenCodeWatcher {
    /// Create a new OpenCode watcher for a session
    ///
    /// Watches the distributed storage directories and emits activities via Tauri events.
    pub fn new(session_id: String, opencode_session_id: String, app: AppHandle) -> Result<Self, String> {
        tracing::info!(
            "Creating OpenCode watcher for session {}, OpenCode session: {}",
            session_id,
            opencode_session_id
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        let session_id_clone = session_id.clone();
        let opencode_session_id_clone = opencode_session_id.clone();

        // Spawn watcher thread
        let watcher_handle = std::thread::spawn(move || {
            Self::run_watcher(
                session_id_clone,
                opencode_session_id_clone,
                app,
                stop_flag_clone,
            );
        });

        Ok(Self {
            stop_flag,
            _watcher_handle: watcher_handle,
        })
    }

    /// Stop the watcher
    pub fn stop(&self) {
        tracing::info!("Stopping OpenCode watcher");
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Run the file watcher (called in a separate thread)
    fn run_watcher(
        session_id: String,
        opencode_session_id: String,
        app: AppHandle,
        stop_flag: Arc<AtomicBool>,
    ) {
        // Create a channel for the notify watcher
        let (tx, rx) = std::sync::mpsc::channel();

        // Create the watcher
        let mut watcher: RecommendedWatcher = match Watcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default().with_poll_interval(std::time::Duration::from_millis(200)),
        ) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to create OpenCode watcher: {}", e);
                return;
            }
        };

        let storage_dir = get_opencode_storage_dir();

        // Watch the message and part directories for this session
        let message_dir = storage_dir.join("message").join(&opencode_session_id);
        let part_dir = storage_dir.join("part");

        // Wait for directories to exist
        let mut waited = 0;
        while !message_dir.exists() && !stop_flag.load(Ordering::SeqCst) && waited < 60 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            waited += 1;
        }

        // Watch message directory for this session
        if message_dir.exists() {
            if let Err(e) = watcher.watch(&message_dir, RecursiveMode::NonRecursive) {
                tracing::warn!("Failed to watch OpenCode message directory {:?}: {}", message_dir, e);
            } else {
                tracing::info!("Watching OpenCode message directory: {:?}", message_dir);
            }
        } else {
            tracing::warn!("OpenCode message directory doesn't exist: {:?}", message_dir);
        }

        // Watch part directory recursively (parts are organized by message ID)
        if part_dir.exists() {
            if let Err(e) = watcher.watch(&part_dir, RecursiveMode::Recursive) {
                tracing::warn!("Failed to watch OpenCode part directory {:?}: {}", part_dir, e);
            } else {
                tracing::info!("Watching OpenCode part directory: {:?}", part_dir);
            }
        }

        // Track seen IDs for deduplication
        let mut seen_messages: HashSet<String> = HashSet::new();
        let mut seen_parts: HashSet<String> = HashSet::new();

        // Load existing messages and parts to avoid re-emitting
        if message_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&message_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".json") {
                            seen_messages.insert(name.trim_end_matches(".json").to_string());
                        }
                    }
                }
            }
        }

        tracing::info!(
            "OpenCode watcher started for session {}, tracking {} existing messages",
            session_id,
            seen_messages.len()
        );

        // Main event loop
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                tracing::info!("OpenCode watcher for session {} stopping", session_id);
                break;
            }

            // Wait for events with timeout
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(event) => {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in event.paths {
                                // Check if this is a message file
                                if path.to_string_lossy().contains("/message/")
                                    && path.to_string_lossy().contains(&opencode_session_id)
                                    && path.extension().map_or(false, |e| e == "json")
                                {
                                    Self::process_message_file(
                                        &path,
                                        &session_id,
                                        &app,
                                        &mut seen_messages,
                                    );
                                }
                                // Check if this is a part file
                                else if path.to_string_lossy().contains("/part/")
                                    && path.extension().map_or(false, |e| e == "json")
                                {
                                    Self::process_part_file(
                                        &path,
                                        &session_id,
                                        &opencode_session_id,
                                        &app,
                                        &mut seen_parts,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!(
                        "OpenCode watcher channel disconnected for session {}",
                        session_id
                    );
                    break;
                }
            }
        }

        tracing::info!("OpenCode watcher thread exiting for session {}", session_id);
    }

    /// Process a message file and emit activity if new
    fn process_message_file(
        path: &PathBuf,
        session_id: &str,
        app: &AppHandle,
        seen_messages: &mut HashSet<String>,
    ) {
        let file_name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(name) => name.to_string(),
            None => return,
        };

        // Skip if already seen
        if seen_messages.contains(&file_name) {
            return;
        }

        // Try to read and parse the message
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("Failed to read OpenCode message file {:?}: {}", path, e);
                return;
            }
        };

        let message: OpenCodeMessage = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("Failed to parse OpenCode message: {}", e);
                return;
            }
        };

        // Mark as seen
        seen_messages.insert(file_name);

        tracing::debug!(
            "OpenCode message {} from {} (role: {})",
            message.id,
            session_id,
            message.role
        );

        // Emit user prompt activity for user messages
        if message.role == "user" {
            let _ = app.emit(
                "jsonl-activity",
                serde_json::json!({
                    "sessionId": session_id,
                    "activityType": "user_prompt",
                    "content": format!("User input (message {})", message.id),
                    "isStreaming": false,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "uuid": message.id,
                    "source": "opencode",
                }),
            );
        }
    }

    /// Process a part file and emit activity if new
    fn process_part_file(
        path: &PathBuf,
        session_id: &str,
        opencode_session_id: &str,
        app: &AppHandle,
        seen_parts: &mut HashSet<String>,
    ) {
        let file_name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(name) => name.to_string(),
            None => return,
        };

        // Skip if already seen
        if seen_parts.contains(&file_name) {
            return;
        }

        // Try to read and parse the part
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("Failed to read OpenCode part file {:?}: {}", path, e);
                return;
            }
        };

        let part: OpenCodePart = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("Failed to parse OpenCode part: {}", e);
                return;
            }
        };

        // Only process parts for our session
        if part.session_id != opencode_session_id {
            return;
        }

        // Mark as seen
        seen_parts.insert(file_name);

        // Convert part to activity and emit
        let activity = Self::part_to_activity(&part);
        if let Some(activity) = activity {
            Self::emit_activity(session_id, &activity, app);
        }
    }

    /// Convert an OpenCode part to an activity
    fn part_to_activity(part: &OpenCodePart) -> Option<Activity> {
        match part.part_type.as_str() {
            "text" => {
                let content = part.text.clone().unwrap_or_default();
                if content.is_empty() {
                    return None;
                }
                Some(Activity {
                    activity_type: ActivityType::Text,
                    content,
                    tool_name: None,
                    tool_params: None,
                    file_path: None,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    uuid: Some(part.id.clone()),
                })
            }
            "reasoning" => {
                let content = part.text.clone().unwrap_or_default();
                if content.is_empty() {
                    return None;
                }
                Some(Activity {
                    activity_type: ActivityType::Thinking,
                    content,
                    tool_name: None,
                    tool_params: None,
                    file_path: None,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    uuid: Some(part.id.clone()),
                })
            }
            "tool" => {
                let tool_name = part.tool.clone();
                let state = part.state.as_ref();

                // Determine if this is tool start or result based on status
                let status = state.and_then(|s| s.status.as_ref()).map(|s| s.as_str());

                match status {
                    Some("completed") => {
                        let output = state
                            .and_then(|s| s.output.as_ref())
                            .map(|o| {
                                if let Some(s) = o.as_str() {
                                    s.to_string()
                                } else {
                                    serde_json::to_string_pretty(o).unwrap_or_default()
                                }
                            })
                            .unwrap_or_default();

                        Some(Activity {
                            activity_type: ActivityType::ToolResult,
                            content: output,
                            tool_name,
                            tool_params: state.and_then(|s| s.input.clone()),
                            file_path: None,
                            timestamp: Some(chrono::Utc::now().to_rfc3339()),
                            uuid: Some(part.id.clone()),
                        })
                    }
                    Some("pending") | None => {
                        let title = state.and_then(|s| s.title.clone()).unwrap_or_default();
                        Some(Activity {
                            activity_type: ActivityType::ToolStart,
                            content: title,
                            tool_name,
                            tool_params: state.and_then(|s| s.input.clone()),
                            file_path: None,
                            timestamp: Some(chrono::Utc::now().to_rfc3339()),
                            uuid: Some(part.id.clone()),
                        })
                    }
                    Some(_) => None,
                }
            }
            "step-start" => {
                Some(Activity {
                    activity_type: ActivityType::Progress,
                    content: "Processing...".to_string(),
                    tool_name: None,
                    tool_params: None,
                    file_path: None,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    uuid: Some(part.id.clone()),
                })
            }
            _ => None,
        }
    }

    /// Emit a single activity via Tauri events
    fn emit_activity(session_id: &str, activity: &Activity, app: &AppHandle) {
        let activity_type_str = match activity.activity_type {
            ActivityType::Thinking => "thinking",
            ActivityType::ToolStart => "tool_start",
            ActivityType::ToolResult => "tool_result",
            ActivityType::Text => "text",
            ActivityType::UserPrompt => "user_prompt",
            ActivityType::FileWrite => "file_write",
            ActivityType::FileRead => "file_read",
            ActivityType::BashCommand => "bash_command",
            ActivityType::CodeDiff => "code_diff",
            ActivityType::Progress => "progress",
            ActivityType::Summary => "summary",
        };

        tracing::debug!(
            "Emitting OpenCode activity for session {}: {} ({} chars)",
            session_id,
            activity_type_str,
            activity.content.len()
        );

        let _ = app.emit(
            "jsonl-activity",
            serde_json::json!({
                "sessionId": session_id,
                "activityType": activity_type_str,
                "content": activity.content,
                "toolName": activity.tool_name,
                "toolParams": activity.tool_params,
                "filePath": activity.file_path,
                "isStreaming": false,
                "timestamp": activity.timestamp,
                "uuid": activity.uuid,
                "source": "opencode",
            }),
        );
    }
}

impl Drop for OpenCodeWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Find an existing OpenCode session file for a project
pub fn find_session_for_project(_project_path: &str) -> Option<String> {
    let storage_dir = get_opencode_storage_dir();
    let session_dir = storage_dir.join("session");

    if !session_dir.exists() {
        return None;
    }

    // Hash the project path to find the right directory
    // OpenCode uses a hash of the project path
    // For now, we'll try to find any session and match by checking project contents
    if let Ok(entries) = std::fs::read_dir(&session_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Check session files in this directory
                if let Ok(session_entries) = std::fs::read_dir(&path) {
                    for session_entry in session_entries.flatten() {
                        let session_path = session_entry.path();
                        if session_path.extension().map_or(false, |e| e == "json") {
                            if let Ok(content) = std::fs::read_to_string(&session_path) {
                                if let Ok(session) = serde_json::from_str::<OpenCodeSession>(&content) {
                                    // Return the session ID
                                    tracing::debug!("Found OpenCode session: {}", session.id);
                                    return Some(session.id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Get the latest OpenCode session
pub fn get_latest_session() -> Option<String> {
    let storage_dir = get_opencode_storage_dir();
    let session_dir = storage_dir.join("session");

    if !session_dir.exists() {
        return None;
    }

    let mut latest_session: Option<(String, std::time::SystemTime)> = None;

    if let Ok(entries) = std::fs::read_dir(&session_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().map_or(false, |n| n != "global") {
                if let Ok(session_entries) = std::fs::read_dir(&path) {
                    for session_entry in session_entries.flatten() {
                        let session_path = session_entry.path();
                        if session_path.extension().map_or(false, |e| e == "json") {
                            if let Ok(metadata) = session_path.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    if let Ok(content) = std::fs::read_to_string(&session_path) {
                                        if let Ok(session) = serde_json::from_str::<OpenCodeSession>(&content) {
                                            if latest_session.as_ref().map_or(true, |(_, t)| modified > *t) {
                                                latest_session = Some((session.id, modified));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    latest_session.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_storage_dir() {
        let dir = get_opencode_storage_dir();
        assert!(dir.to_string_lossy().contains(".local/share/opencode/storage"));
    }

    #[test]
    fn test_parse_session() {
        let json = r#"{"id": "ses_123", "slug": "test", "version": "1.0", "projectID": "abc"}"#;
        let session: OpenCodeSession = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "ses_123");
        assert_eq!(session.project_id, "abc");
    }

    #[test]
    fn test_parse_message() {
        let json = r#"{"id": "msg_123", "sessionID": "ses_123", "role": "user", "time": {"created": 123}}"#;
        let message: OpenCodeMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.id, "msg_123");
        assert_eq!(message.role, "user");
    }

    #[test]
    fn test_parse_part() {
        let json = r#"{"id": "prt_123", "sessionID": "ses_123", "messageID": "msg_123", "type": "text", "text": "Hello"}"#;
        let part: OpenCodePart = serde_json::from_str(json).unwrap();
        assert_eq!(part.id, "prt_123");
        assert_eq!(part.part_type, "text");
        assert_eq!(part.text, Some("Hello".to_string()));
    }
}
