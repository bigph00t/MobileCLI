//! Codex File Watcher - Real-time updates from Codex CLI conversation logs
//!
//! Watches Codex's JSONL files for changes and emits activities via Tauri events.
//! Codex stores conversations at ~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl

use crate::codex::{parse_codex_line, record_to_activities, Activity};
use crate::parser::ActivityType;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Codex JSONL file watcher for a single session
pub struct CodexWatcher {
    /// Flag to signal the watcher should stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the watcher thread
    _watcher_handle: std::thread::JoinHandle<()>,
}

impl CodexWatcher {
    /// Create a new Codex watcher for a session
    ///
    /// Watches the JSONL file at `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
    /// and emits activities via Tauri events when new entries are added.
    pub fn new(session_id: String, jsonl_path: PathBuf, app: AppHandle) -> Result<Self, String> {
        tracing::info!(
            "Creating Codex watcher for session {}: {:?}",
            session_id,
            jsonl_path
        );

        // Track file position for incremental reads
        let last_position = Arc::new(AtomicU64::new(0));

        // If file already exists, get initial position (skip existing content)
        if jsonl_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&jsonl_path) {
                last_position.store(metadata.len(), Ordering::SeqCst);
                tracing::info!(
                    "Codex JSONL file exists with {} bytes, will emit new entries only",
                    metadata.len()
                );
            }
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Clone for the watcher thread
        let session_id_clone = session_id.clone();
        let jsonl_path_clone = jsonl_path.clone();
        let last_position_clone = last_position.clone();

        // Spawn watcher thread
        let watcher_handle = std::thread::spawn(move || {
            Self::run_watcher(
                session_id_clone,
                jsonl_path_clone,
                app,
                last_position_clone,
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
        tracing::info!("Stopping Codex watcher");
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Run the file watcher (called in a separate thread)
    fn run_watcher(
        session_id: String,
        jsonl_path: PathBuf,
        app: AppHandle,
        last_position: Arc<AtomicU64>,
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
                tracing::error!("Failed to create Codex watcher: {}", e);
                return;
            }
        };

        // Watch the parent directory since the file might not exist yet
        let parent_dir = jsonl_path.parent().unwrap_or(&jsonl_path);

        // Try to create the parent directory if it doesn't exist
        if !parent_dir.exists() {
            tracing::info!(
                "Codex JSONL parent directory doesn't exist yet, waiting: {:?}",
                parent_dir
            );
            // Poll for directory creation
            let mut waited = 0;
            while !parent_dir.exists() && !stop_flag.load(Ordering::SeqCst) && waited < 60 {
                std::thread::sleep(std::time::Duration::from_secs(1));
                waited += 1;
            }
            if !parent_dir.exists() {
                tracing::warn!("Codex JSONL parent directory still doesn't exist after 60s");
                return;
            }
        }

        // Start watching
        if let Err(e) = watcher.watch(parent_dir, RecursiveMode::NonRecursive) {
            tracing::error!("Failed to watch Codex directory {:?}: {}", parent_dir, e);
            return;
        }

        tracing::info!(
            "Started watching Codex directory for session {}: {:?}",
            session_id,
            parent_dir
        );

        // Track UUIDs we've seen to avoid duplicates
        let mut seen_uuids: HashSet<String> = HashSet::new();

        // Main event loop
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                tracing::info!("Codex watcher for session {} stopping", session_id);
                break;
            }

            // Wait for events with timeout
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(event) => {
                    // Check if this event is for our JSONL file
                    let is_our_file = event.paths.iter().any(|p| p == &jsonl_path);

                    if is_our_file {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => {
                                tracing::debug!("Codex file changed for session {}", session_id);

                                // Read new entries and emit
                                Self::emit_new_entries(
                                    &session_id,
                                    &jsonl_path,
                                    &app,
                                    &last_position,
                                    &mut seen_uuids,
                                );
                            }
                            _ => {}
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Continue loop to check stop flag
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!(
                        "Codex watcher channel disconnected for session {}",
                        session_id
                    );
                    break;
                }
            }
        }

        tracing::info!("Codex watcher thread exiting for session {}", session_id);
    }

    /// Read the JSONL file from last position and emit any new entries as activities
    fn emit_new_entries(
        session_id: &str,
        jsonl_path: &PathBuf,
        app: &AppHandle,
        last_position: &Arc<AtomicU64>,
        seen_uuids: &mut HashSet<String>,
    ) {
        if !jsonl_path.exists() {
            return;
        }

        let file = match File::open(jsonl_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Failed to open Codex JSONL file: {}", e);
                return;
            }
        };

        let old_pos = last_position.load(Ordering::SeqCst);
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);

        if file_len <= old_pos {
            return; // No new content
        }

        let mut reader = BufReader::new(file);

        // Seek to last position
        if let Err(e) = reader.seek(SeekFrom::Start(old_pos)) {
            tracing::warn!("Failed to seek in Codex JSONL: {}", e);
            return;
        }

        tracing::debug!(
            "Codex JSONL has {} new bytes for session {}",
            file_len - old_pos,
            session_id
        );

        // Read new lines
        let mut new_pos = old_pos;
        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    new_pos += line.len() as u64 + 1; // +1 for newline

                    if line.trim().is_empty() {
                        continue;
                    }

                    // Parse the Codex record
                    match parse_codex_line(&line) {
                        Ok(record) => {
                            // Convert to activities
                            let activities = record_to_activities(&record);

                            for activity in activities {
                                // Skip if we've seen this UUID
                                if let Some(ref uuid) = activity.uuid {
                                    if seen_uuids.contains(uuid) {
                                        continue;
                                    }
                                    seen_uuids.insert(uuid.clone());
                                }

                                Self::emit_activity(session_id, &activity, app);
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Failed to parse Codex line: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read Codex JSONL line: {}", e);
                    break;
                }
            }
        }

        // Update position
        last_position.store(new_pos, Ordering::SeqCst);
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
            "Emitting Codex activity for session {}: {} ({} chars)",
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
                "isStreaming": false, // JSONL entries are always complete
                "timestamp": activity.timestamp,
                "uuid": activity.uuid,
                "source": "codex", // Mark as coming from Codex watcher
            }),
        );
    }
}

impl Drop for CodexWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::get_codex_sessions_dir;

    #[test]
    fn test_codex_sessions_dir() {
        let dir = get_codex_sessions_dir();
        assert!(dir.to_string_lossy().contains(".codex/sessions"));
    }
}
