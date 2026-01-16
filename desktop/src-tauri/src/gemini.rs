//! Gemini CLI Module - Read and parse Gemini CLI conversation logs
//!
//! Gemini CLI stores structured conversation data at:
//! ~/.gemini/tmp/<project_hash>/chats/session-<timestamp>-<uuid>.json
//!
//! The project_hash is SHA-256 of the absolute project path.
//! This module reads these files to get clean, structured activities
//! for the mobile app.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use thiserror::Error;

use crate::parser::ActivityType;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum GeminiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
}

// ============================================================================
// Path Discovery
// ============================================================================

/// Get the Gemini home directory
pub fn get_gemini_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    PathBuf::from(format!("{}/.gemini", home))
}

/// Get the tmp directory where chats are stored
pub fn get_gemini_tmp_dir() -> PathBuf {
    get_gemini_home().join("tmp")
}

/// Compute SHA-256 hash of project path (Gemini's method)
pub fn compute_project_hash(project_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_path.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Get the chats directory for a specific project
pub fn get_project_chats_dir(project_path: &str) -> PathBuf {
    let hash = compute_project_hash(project_path);
    get_gemini_tmp_dir().join(&hash).join("chats")
}

/// Find the session file for a Gemini session by looking for matching UUID
pub fn find_session_file(project_path: &str, session_id: &str) -> Option<PathBuf> {
    let chats_dir = get_project_chats_dir(project_path);
    if !chats_dir.exists() {
        return None;
    }

    // Look for a session file matching the session_id
    for entry in std::fs::read_dir(&chats_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if let Some(filename) = path.file_name() {
            let filename_str = filename.to_string_lossy();
            if filename_str.contains(session_id) && filename_str.ends_with(".json") {
                return Some(path);
            }
        }
    }

    None
}

/// Get the most recent session file for a project
pub fn get_latest_session_file(project_path: &str) -> Option<PathBuf> {
    let chats_dir = get_project_chats_dir(project_path);
    if !chats_dir.exists() {
        return None;
    }

    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&chats_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            if let Ok(metadata) = path.metadata() {
                if let Ok(modified) = metadata.modified() {
                    match &latest {
                        None => latest = Some((path, modified)),
                        Some((_, latest_time)) if modified > *latest_time => {
                            latest = Some((path, modified));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    latest.map(|(path, _)| path)
}

/// Extract session ID from a Gemini session filename
/// e.g., "session-2026-01-15T14-30-6be474c8.json" -> "6be474c8"
pub fn extract_session_id_from_filename(filename: &str) -> Option<String> {
    // Pattern: session-<date>T<time>-<uuid-prefix>.json
    let name = filename.strip_suffix(".json")?;
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() >= 4 {
        // Last part is the UUID prefix
        return Some(parts.last()?.to_string());
    }
    None
}

// ============================================================================
// Session JSON Types (matching Gemini format)
// ============================================================================

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cached: Option<u64>,
    pub thoughts: Option<u64>,
}

/// Thought entry in Gemini's extended thinking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub timestamp: Option<String>,
}

/// Tool call entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub result: serde_json::Value,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiMessage {
    pub id: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: String, // "user", "gemini", "info"
    pub content: Option<String>,
    #[serde(default)]
    pub thoughts: Vec<Thought>,
    #[serde(default)]
    pub tokens: Option<TokenUsage>,
    #[serde(rename = "toolCalls", default)]
    pub tool_calls: Vec<ToolCall>,
}

/// A complete Gemini session file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiSession {
    pub session_id: String,
    pub project_hash: Option<String>,
    pub start_time: Option<String>,
    pub last_updated: Option<String>,
    #[serde(default)]
    pub messages: Vec<GeminiMessage>,
}

// ============================================================================
// Activity Conversion
// ============================================================================

/// Activity for mobile display (matches existing Activity structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub activity_type: ActivityType,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default)]
    pub is_streaming: bool,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

impl Activity {
    fn new(activity_type: ActivityType, content: String, timestamp: String) -> Self {
        Self {
            activity_type,
            content,
            tool_name: None,
            tool_params: None,
            file_path: None,
            is_streaming: false,
            timestamp,
            uuid: None,
        }
    }

    fn with_uuid(mut self, uuid: Option<String>) -> Self {
        self.uuid = uuid;
        self
    }

    fn with_tool(mut self, name: String, params: Option<String>) -> Self {
        self.tool_name = Some(name);
        self.tool_params = params;
        self
    }
}

/// Convert a Gemini message into activities for display
pub fn message_to_activities(message: &GeminiMessage) -> Vec<Activity> {
    let mut activities = Vec::new();
    let timestamp = message.timestamp.clone().unwrap_or_default();

    match message.msg_type.as_str() {
        "user" => {
            // User message
            if let Some(ref content) = message.content {
                if !content.trim().is_empty() {
                    activities.push(
                        Activity::new(ActivityType::UserPrompt, content.clone(), timestamp.clone())
                            .with_uuid(message.id.clone()),
                    );
                }
            }
        }
        "gemini" => {
            // Gemini response - may include thoughts, content, and tool calls

            // Add thinking activities from thoughts
            for thought in &message.thoughts {
                let thought_text = thought.description.clone().unwrap_or_default();
                if !thought_text.is_empty() {
                    let subject = thought
                        .subject
                        .clone()
                        .unwrap_or_else(|| "Thinking".to_string());
                    activities.push(Activity::new(
                        ActivityType::Thinking,
                        format!("{}: {}", subject, thought_text),
                        thought
                            .timestamp
                            .clone()
                            .unwrap_or_else(|| timestamp.clone()),
                    ));
                }
            }

            // Add tool calls
            for tool_call in &message.tool_calls {
                let tool_content = format_tool_call(&tool_call.name, &tool_call.args);
                activities.push(
                    Activity::new(ActivityType::ToolStart, tool_content, timestamp.clone())
                        .with_uuid(tool_call.id.clone())
                        .with_tool(
                            tool_call.name.clone(),
                            Some(serde_json::to_string(&tool_call.args).unwrap_or_default()),
                        ),
                );

                // Add tool result if present
                if !tool_call.result.is_null() {
                    let result_str = if tool_call.result.is_string() {
                        tool_call.result.as_str().unwrap_or("").to_string()
                    } else {
                        serde_json::to_string_pretty(&tool_call.result).unwrap_or_default()
                    };
                    if !result_str.is_empty() {
                        activities.push(Activity::new(
                            ActivityType::ToolResult,
                            result_str,
                            timestamp.clone(),
                        ));
                    }
                }
            }

            // Add text content
            if let Some(ref content) = message.content {
                if !content.trim().is_empty() {
                    activities.push(
                        Activity::new(ActivityType::Text, content.clone(), timestamp)
                            .with_uuid(message.id.clone()),
                    );
                }
            }
        }
        "info" => {
            // Info messages (system notifications) - could show as progress
            if let Some(ref content) = message.content {
                if !content.trim().is_empty() {
                    activities.push(Activity::new(
                        ActivityType::Progress,
                        content.clone(),
                        timestamp,
                    ));
                }
            }
        }
        _ => {
            // Unknown type - show as text if has content
            if let Some(ref content) = message.content {
                if !content.trim().is_empty() {
                    activities.push(Activity::new(
                        ActivityType::Text,
                        content.clone(),
                        timestamp,
                    ));
                }
            }
        }
    }

    activities
}

/// Format a tool call for display
fn format_tool_call(name: &str, args: &serde_json::Value) -> String {
    match name {
        "shell" | "bash" | "execute_command" | "run_shell_command" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                return format!("Bash({})", cmd);
            }
        }
        "read_file" | "read" | "read_files" => {
            if let Some(path) = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .or_else(|| args.get("paths").and_then(|p| p.get(0)))
                .and_then(|v| v.as_str())
            {
                return format!("Read({})", path);
            }
        }
        "write_file" | "write" | "write_files" => {
            if let Some(path) = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
            {
                return format!("Write({})", path);
            }
        }
        "edit_file" | "apply_diff" | "edit" => {
            if let Some(path) = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
            {
                return format!("Edit({})", path);
            }
        }
        "search_files" | "glob" | "find_files" => {
            if let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) {
                return format!("Search({})", pattern);
            }
        }
        "grep" | "search_code" => {
            if let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) {
                return format!("Grep({})", pattern);
            }
        }
        _ => {}
    }
    format!("{}()", name)
}

// ============================================================================
// File Reading
// ============================================================================

/// Read a Gemini session file
pub fn read_session_file(path: &PathBuf) -> Result<GeminiSession, GeminiError> {
    if !path.exists() {
        return Err(GeminiError::FileNotFound(path.clone()));
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let session: GeminiSession = serde_json::from_reader(reader)?;

    tracing::info!(
        "Read Gemini session with {} messages from {:?}",
        session.messages.len(),
        path
    );

    Ok(session)
}

/// Read session and convert to activities for display
pub fn read_activities(project_path: &str, session_id: &str) -> Result<Vec<Activity>, GeminiError> {
    let path = match find_session_file(project_path, session_id) {
        Some(p) => p,
        None => {
            tracing::info!("Gemini session file not found for: {}", session_id);
            return Ok(Vec::new());
        }
    };

    let session = read_session_file(&path)?;

    let activities: Vec<Activity> = session
        .messages
        .iter()
        .flat_map(message_to_activities)
        .collect();

    tracing::info!(
        "Converted {} Gemini messages to {} activities",
        session.messages.len(),
        activities.len()
    );

    Ok(activities)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_project_hash() {
        let hash = compute_project_hash("/home/user/project");
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex chars
    }

    #[test]
    fn test_get_gemini_home() {
        let home = get_gemini_home();
        assert!(home.to_string_lossy().contains(".gemini"));
    }

    #[test]
    fn test_extract_session_id_from_filename() {
        let filename = "session-2026-01-15T14-30-6be474c8.json";
        let id = extract_session_id_from_filename(filename);
        assert_eq!(id, Some("6be474c8".to_string()));
    }

    #[test]
    fn test_user_message_to_activities() {
        let msg = GeminiMessage {
            id: Some("msg-1".to_string()),
            timestamp: Some("2026-01-15T12:00:00Z".to_string()),
            msg_type: "user".to_string(),
            content: Some("Hello Gemini".to_string()),
            thoughts: vec![],
            tokens: None,
            tool_calls: vec![],
        };

        let activities = message_to_activities(&msg);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::UserPrompt);
        assert_eq!(activities[0].content, "Hello Gemini");
    }

    #[test]
    fn test_gemini_response_to_activities() {
        let msg = GeminiMessage {
            id: Some("msg-2".to_string()),
            timestamp: Some("2026-01-15T12:00:01Z".to_string()),
            msg_type: "gemini".to_string(),
            content: Some("Hello! How can I help?".to_string()),
            thoughts: vec![Thought {
                subject: Some("Analysis".to_string()),
                description: Some("User greeted me".to_string()),
                timestamp: None,
            }],
            tokens: None,
            tool_calls: vec![],
        };

        let activities = message_to_activities(&msg);
        assert_eq!(activities.len(), 2); // 1 thinking + 1 text
        assert_eq!(activities[0].activity_type, ActivityType::Thinking);
        assert_eq!(activities[1].activity_type, ActivityType::Text);
    }

    #[test]
    fn test_tool_call_formatting() {
        let args = serde_json::json!({ "command": "ls -la" });
        assert_eq!(format_tool_call("shell", &args), "Bash(ls -la)");

        let args = serde_json::json!({ "path": "/home/test.txt" });
        assert_eq!(format_tool_call("read_file", &args), "Read(/home/test.txt)");
    }
}
