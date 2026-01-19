//! JSONL File Watcher - Real-time updates from Claude's conversation logs
//!
//! Watches Claude's JSONL files for changes and emits activities via Tauri events.
//! This provides clean, structured conversation data instead of parsing raw PTY output.

use crate::jsonl::{entry_to_activities_with_context, get_jsonl_path, read_jsonl_file, Activity};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// JSONL file watcher for a single session
pub struct JsonlWatcher {
    /// Flag to signal the watcher should stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the watcher thread
    _watcher_handle: std::thread::JoinHandle<()>,
}

impl JsonlWatcher {
    /// Create a new JSONL watcher for a Claude session
    ///
    /// Watches the JSONL file at `~/.claude/projects/{encoded-path}/{conversation_id}.jsonl`
    /// and emits activities via Tauri events when new entries are added.
    pub fn new(
        session_id: String,
        project_path: String,
        conversation_id: String,
        app: AppHandle,
    ) -> Result<Self, String> {
        let jsonl_path = get_jsonl_path(&project_path, &conversation_id);

        tracing::info!(
            "Creating JSONL watcher for session {}: {:?}",
            session_id,
            jsonl_path
        );

        // Track entries we've already processed to avoid duplicates
        let last_entry_count = Arc::new(AtomicUsize::new(0));

        // If file already exists, get initial entry count
        if jsonl_path.exists() {
            if let Ok(entries) = read_jsonl_file(&jsonl_path) {
                last_entry_count.store(entries.len(), Ordering::SeqCst);
                tracing::info!(
                    "JSONL file exists with {} entries, will emit new entries only",
                    entries.len()
                );
            }
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Clone for the watcher thread
        let session_id_clone = session_id.clone();
        let jsonl_path_clone = jsonl_path.clone();
        let last_entry_count_clone = last_entry_count.clone();

        // Spawn watcher thread
        let watcher_handle = std::thread::spawn(move || {
            Self::run_watcher(
                session_id_clone,
                jsonl_path_clone,
                app,
                last_entry_count_clone,
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
        tracing::info!("Stopping JSONL watcher");
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Run the file watcher (called in a separate thread)
    fn run_watcher(
        session_id: String,
        jsonl_path: PathBuf,
        app: AppHandle,
        last_entry_count: Arc<AtomicUsize>,
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
                tracing::error!("Failed to create JSONL watcher: {}", e);
                return;
            }
        };

        // Watch the parent directory since the file might not exist yet
        let parent_dir = jsonl_path.parent().unwrap_or(&jsonl_path);

        // Try to create the parent directory if it doesn't exist
        if !parent_dir.exists() {
            tracing::info!(
                "JSONL parent directory doesn't exist yet, waiting: {:?}",
                parent_dir
            );
            // Poll for directory creation
            let mut waited = 0;
            while !parent_dir.exists() && !stop_flag.load(Ordering::SeqCst) && waited < 60 {
                std::thread::sleep(std::time::Duration::from_secs(1));
                waited += 1;
            }
            if !parent_dir.exists() {
                tracing::warn!("JSONL parent directory still doesn't exist after 60s");
                return;
            }
        }

        // Start watching
        if let Err(e) = watcher.watch(parent_dir, RecursiveMode::NonRecursive) {
            tracing::error!("Failed to watch JSONL directory {:?}: {}", parent_dir, e);
            return;
        }

        tracing::info!(
            "Started watching JSONL directory for session {}: {:?}",
            session_id,
            parent_dir
        );

        // Track UUIDs we've seen to avoid duplicates
        let mut seen_uuids: HashSet<String> = HashSet::new();

        // Track tool_use_id → toolName mappings for associating ToolResult with ToolUse
        let mut tool_map: HashMap<String, String> = HashMap::new();

        // Initialize seen_uuids with existing entries and build initial tool_map
        if jsonl_path.exists() {
            if let Ok(entries) = read_jsonl_file(&jsonl_path) {
                for entry in &entries {
                    if let Some(ref uuid) = entry.uuid {
                        seen_uuids.insert(uuid.clone());
                    }
                    // Also process the entry to build the initial tool_map
                    // (without emitting activities)
                    let _ = entry_to_activities_with_context(entry, &mut tool_map);
                }
            }
        }

        // Main event loop
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                tracing::info!("JSONL watcher for session {} stopping", session_id);
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
                                tracing::debug!("JSONL file changed for session {}", session_id);

                                // Read new entries and emit
                                Self::emit_new_entries(
                                    &session_id,
                                    &jsonl_path,
                                    &app,
                                    &last_entry_count,
                                    &mut seen_uuids,
                                    &mut tool_map,
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
                        "JSONL watcher channel disconnected for session {}",
                        session_id
                    );
                    break;
                }
            }
        }

        tracing::info!("JSONL watcher thread exiting for session {}", session_id);
    }

    /// Read the JSONL file and emit any new entries as activities
    fn emit_new_entries(
        session_id: &str,
        jsonl_path: &PathBuf,
        app: &AppHandle,
        last_entry_count: &Arc<AtomicUsize>,
        seen_uuids: &mut HashSet<String>,
        tool_map: &mut HashMap<String, String>,
    ) {
        if !jsonl_path.exists() {
            return;
        }

        let entries = match read_jsonl_file(jsonl_path) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("Failed to read JSONL file for new entries: {}", e);
                return;
            }
        };

        let old_count = last_entry_count.load(Ordering::SeqCst);
        let new_count = entries.len();

        if new_count <= old_count {
            return; // No new entries
        }

        tracing::debug!(
            "JSONL has {} new entries for session {}",
            new_count - old_count,
            session_id
        );

        // Process new entries (from old_count onwards)
        for entry in entries.iter().skip(old_count) {
            // Skip if we've already seen this UUID
            if let Some(ref uuid) = entry.uuid {
                if seen_uuids.contains(uuid) {
                    continue;
                }
                seen_uuids.insert(uuid.clone());
            }

            // Convert entry to activities and emit each one
            // Use context-aware version to track tool_use_id → toolName mappings
            let activities = entry_to_activities_with_context(entry, tool_map);

            for activity in activities {
                Self::emit_activity(session_id, &activity, app);
            }
        }

        // Update count
        last_entry_count.store(new_count, Ordering::SeqCst);
    }

    /// Emit a single activity via Tauri events
    fn emit_activity(session_id: &str, activity: &Activity, app: &AppHandle) {
        let activity_type_str = match activity.activity_type {
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

        tracing::debug!(
            "Emitting JSONL activity for session {}: {} ({} chars)",
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
                "isStreaming": false, // JSONL entries are always complete
                "timestamp": activity.timestamp,
                "uuid": activity.uuid,
                "summary": activity.summary,
                "source": "jsonl", // Mark as coming from JSONL watcher
            }),
        );
    }
}

impl Drop for JsonlWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonl_path_construction() {
        let path = get_jsonl_path("/home/user/project", "abc-123");
        assert!(path.to_string_lossy().contains(".claude/projects/"));
        assert!(path.to_string_lossy().ends_with("abc-123.jsonl"));
    }
}
