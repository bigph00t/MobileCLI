//! Gemini CLI File Watcher - Real-time updates from Gemini CLI conversation logs
//!
//! Watches Gemini's JSON session files for changes and emits activities via Tauri events.
//! Gemini stores conversations at ~/.gemini/tmp/<project_hash>/chats/session-*.json
//!
//! Unlike JSONL watchers, Gemini uses JSON files that get fully rewritten on each update,
//! so we need to re-parse the entire file and compare with previous state.

use crate::gemini::{message_to_activities, read_session_file, Activity};
use crate::parser::ActivityType;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Gemini JSON file watcher for a single session
pub struct GeminiWatcher {
    /// Flag to signal the watcher should stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the watcher thread
    _watcher_handle: std::thread::JoinHandle<()>,
}

impl GeminiWatcher {
    /// Create a new Gemini watcher for a session
    ///
    /// Watches the JSON file at `~/.gemini/tmp/<hash>/chats/session-*.json`
    /// and emits activities via Tauri events when the file changes.
    pub fn new(
        session_id: String,
        json_path: PathBuf,
        app: AppHandle,
    ) -> Result<Self, String> {
        tracing::info!(
            "Creating Gemini watcher for session {}: {:?}",
            session_id,
            json_path
        );

        // Track how many messages we've processed to detect new ones
        let last_message_count = Arc::new(AtomicUsize::new(0));

        // Track seen message IDs for deduplication
        let seen_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        // If file already exists, get initial message count
        if json_path.exists() {
            if let Ok(session) = read_session_file(&json_path) {
                let count = session.messages.len();
                last_message_count.store(count, Ordering::SeqCst);

                // Populate seen IDs
                if let Ok(mut ids) = seen_ids.lock() {
                    for msg in &session.messages {
                        if let Some(ref id) = msg.id {
                            ids.insert(id.clone());
                        }
                    }
                }

                tracing::info!(
                    "Gemini session file exists with {} messages, will emit new messages only",
                    count
                );
            }
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Clone for the watcher thread
        let session_id_clone = session_id.clone();
        let json_path_clone = json_path.clone();
        let last_message_count_clone = last_message_count.clone();
        let seen_ids_clone = seen_ids.clone();

        // Spawn watcher thread
        let watcher_handle = std::thread::spawn(move || {
            Self::run_watcher(
                session_id_clone,
                json_path_clone,
                app,
                last_message_count_clone,
                seen_ids_clone,
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
        tracing::info!("Stopping Gemini watcher");
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Run the file watcher (called in a separate thread)
    fn run_watcher(
        session_id: String,
        json_path: PathBuf,
        app: AppHandle,
        last_message_count: Arc<AtomicUsize>,
        seen_ids: Arc<Mutex<HashSet<String>>>,
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
                tracing::error!("Failed to create Gemini watcher: {}", e);
                return;
            }
        };

        // Watch the parent directory (chats folder)
        let parent_dir = json_path.parent().unwrap_or(&json_path);

        // Wait for directory to exist
        if !parent_dir.exists() {
            tracing::info!(
                "Gemini chats directory doesn't exist yet, waiting: {:?}",
                parent_dir
            );
            let mut waited = 0;
            while !parent_dir.exists() && !stop_flag.load(Ordering::SeqCst) && waited < 60 {
                std::thread::sleep(std::time::Duration::from_secs(1));
                waited += 1;
            }
            if !parent_dir.exists() {
                tracing::warn!("Gemini chats directory still doesn't exist after 60s");
                return;
            }
        }

        // Start watching
        if let Err(e) = watcher.watch(parent_dir, RecursiveMode::NonRecursive) {
            tracing::error!("Failed to watch Gemini directory {:?}: {}", parent_dir, e);
            return;
        }

        tracing::info!(
            "Started watching Gemini directory for session {}: {:?}",
            session_id,
            parent_dir
        );

        // Main event loop
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                tracing::info!("Gemini watcher for session {} stopping", session_id);
                break;
            }

            // Wait for events with timeout
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(event) => {
                    // Check if this event is for our JSON file
                    let is_our_file = event.paths.iter().any(|p| p == &json_path);

                    if is_our_file {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => {
                                tracing::debug!("Gemini file changed for session {}", session_id);

                                // Read and emit new messages
                                Self::emit_new_messages(
                                    &session_id,
                                    &json_path,
                                    &app,
                                    &last_message_count,
                                    &seen_ids,
                                );
                            }
                            _ => {}
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!("Gemini watcher channel disconnected for session {}", session_id);
                    break;
                }
            }
        }

        tracing::info!("Gemini watcher thread exiting for session {}", session_id);
    }

    /// Read the JSON file and emit any new messages as activities
    fn emit_new_messages(
        session_id: &str,
        json_path: &PathBuf,
        app: &AppHandle,
        last_message_count: &Arc<AtomicUsize>,
        seen_ids: &Arc<Mutex<HashSet<String>>>,
    ) {
        if !json_path.exists() {
            return;
        }

        let session = match read_session_file(json_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to read Gemini session file: {}", e);
                return;
            }
        };

        let old_count = last_message_count.load(Ordering::SeqCst);
        let new_count = session.messages.len();

        if new_count <= old_count {
            return; // No new messages
        }

        tracing::debug!(
            "Gemini session has {} new messages for session {}",
            new_count - old_count,
            session_id
        );

        // Process new messages
        for message in session.messages.iter().skip(old_count) {
            // Skip if we've seen this message ID
            if let Some(ref id) = message.id {
                if let Ok(mut ids) = seen_ids.lock() {
                    if ids.contains(id) {
                        continue;
                    }
                    ids.insert(id.clone());
                }
            }

            // Convert to activities and emit
            let activities = message_to_activities(message);
            for activity in activities {
                Self::emit_activity(session_id, &activity, app);
            }
        }

        // Update count
        last_message_count.store(new_count, Ordering::SeqCst);
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
        };

        tracing::debug!(
            "Emitting Gemini activity for session {}: {} ({} chars)",
            session_id,
            activity_type_str,
            activity.content.len()
        );

        let _ = app.emit(
            "jsonl-activity", // Use same event name for mobile compatibility
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
                "source": "gemini", // Mark as coming from Gemini watcher
            }),
        );
    }
}

impl Drop for GeminiWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini::{compute_project_hash, get_project_chats_dir};

    #[test]
    fn test_project_hash_path() {
        let project = "/home/user/project";
        let hash = compute_project_hash(project);
        let chats_dir = get_project_chats_dir(project);
        assert!(chats_dir.to_string_lossy().contains(&hash));
        assert!(chats_dir.to_string_lossy().ends_with("/chats"));
    }
}
