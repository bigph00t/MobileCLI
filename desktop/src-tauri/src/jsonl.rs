//! JSONL Module - Read and parse Claude Code's conversation logs
//!
//! Claude Code stores structured conversation data at:
//! ~/.claude/projects/{encoded-project-path}/{session-id}.jsonl
//!
//! This module reads these files to get clean, structured activities
//! instead of parsing raw PTY output.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use thiserror::Error;

use crate::parser::ActivityType;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum JsonlError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid entry type: {0}")]
    #[allow(dead_code)] // For future use
    InvalidEntryType(String),

    #[error("Missing required field: {0}")]
    #[allow(dead_code)] // For future use
    MissingField(String),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
}

// ============================================================================
// Path Encoding
// ============================================================================

/// Convert project path to Claude's encoded format
/// `/home/bigphoot/Desktop` → `-home-bigphoot-Desktop`
pub fn encode_project_path(path: &str) -> String {
    // Claude uses dash-separated paths with leading dash
    if path.starts_with('/') {
        path.replace('/', "-")
    } else {
        format!("-{}", path.replace('/', "-"))
    }
}

/// Get full path to Claude's JSONL file for a session
pub fn get_jsonl_path(project_path: &str, conversation_id: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    let encoded = encode_project_path(project_path);

    PathBuf::from(format!(
        "{}/.claude/projects/{}/{}.jsonl",
        home, encoded, conversation_id
    ))
}

/// Check if a JSONL file exists for the given session
#[allow(dead_code)] // Utility function for future use
pub fn jsonl_exists(project_path: &str, conversation_id: &str) -> bool {
    get_jsonl_path(project_path, conversation_id).exists()
}

// ============================================================================
// JSONL Entry Types (matching Claude's format)
// ============================================================================

/// Top-level entry type in JSONL
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    User,
    Assistant,
    System,
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot,
    Summary,
}

/// Content block types within assistant messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
}

/// Message content - can be a string or array of blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// The message object within an entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

/// Tool use result metadata (for user entries with tool results)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseResult {
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default)]
    pub is_image: bool,
}

/// A single JSONL entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonlEntry {
    #[serde(rename = "type")]
    pub entry_type: EntryType,

    #[serde(default)]
    pub message: Option<Message>,

    #[serde(default)]
    pub session_id: Option<String>,

    #[serde(default)]
    pub timestamp: Option<String>,

    #[serde(default)]
    pub uuid: Option<String>,

    #[serde(default)]
    pub cwd: Option<String>,

    #[serde(default)]
    pub tool_use_result: Option<ToolUseResult>,

    // System entry fields
    #[serde(default)]
    pub subtype: Option<String>,

    #[serde(default)]
    pub hook_count: Option<i32>,

    // Summary entry fields
    #[serde(default)]
    pub summary: Option<String>,
}

// ============================================================================
// Activity Conversion
// ============================================================================

/// Activity for mobile display (matches existing ActivityBlock structure)
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

/// Convert a JSONL entry into activities for display
pub fn entry_to_activities(entry: &JsonlEntry) -> Vec<Activity> {
    let mut activities = Vec::new();
    let timestamp = entry.timestamp.clone().unwrap_or_default();
    let uuid = entry.uuid.clone();

    match entry.entry_type {
        EntryType::User => {
            if let Some(ref message) = entry.message {
                match &message.content {
                    MessageContent::Text(text) => {
                        // Regular user message
                        if !text.trim().is_empty() {
                            activities.push(
                                Activity::new(ActivityType::UserPrompt, text.clone(), timestamp)
                                    .with_uuid(uuid),
                            );
                        }
                    }
                    MessageContent::Blocks(blocks) => {
                        // Tool results from user entries
                        for block in blocks {
                            if let ContentBlock::ToolResult {
                                tool_use_id: _,
                                content,
                                is_error,
                            } = block
                            {
                                let result_content = match content {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Null => "(No output)".to_string(),
                                    other => other.to_string(),
                                };

                                // Also check toolUseResult for stdout/stderr
                                let full_content = if let Some(ref tool_result) = entry.tool_use_result {
                                    if !tool_result.stdout.is_empty() {
                                        tool_result.stdout.clone()
                                    } else if !tool_result.stderr.is_empty() {
                                        tool_result.stderr.clone()
                                    } else {
                                        result_content
                                    }
                                } else {
                                    result_content
                                };

                                let activity_type = if *is_error {
                                    ActivityType::ToolResult // Could add error indicator
                                } else {
                                    ActivityType::ToolResult
                                };

                                activities.push(
                                    Activity::new(activity_type, full_content, timestamp.clone())
                                        .with_uuid(uuid.clone()),
                                );
                            }
                        }
                    }
                }
            }
        }

        EntryType::Assistant => {
            if let Some(ref message) = entry.message {
                if let MessageContent::Blocks(blocks) = &message.content {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                if !text.trim().is_empty() {
                                    activities.push(
                                        Activity::new(
                                            ActivityType::Text,
                                            text.clone(),
                                            timestamp.clone(),
                                        )
                                        .with_uuid(uuid.clone()),
                                    );
                                }
                            }

                            ContentBlock::Thinking { thinking, .. } => {
                                if !thinking.trim().is_empty() {
                                    activities.push(
                                        Activity::new(
                                            ActivityType::Thinking,
                                            thinking.clone(),
                                            timestamp.clone(),
                                        )
                                        .with_uuid(uuid.clone()),
                                    );
                                }
                            }

                            ContentBlock::ToolUse { id: _, name, input } => {
                                // Format tool call content
                                let content = format_tool_call(name, input);
                                let params = serde_json::to_string(input).ok();

                                activities.push(
                                    Activity::new(
                                        ActivityType::ToolStart,
                                        content,
                                        timestamp.clone(),
                                    )
                                    .with_uuid(uuid.clone())
                                    .with_tool(name.clone(), params),
                                );
                            }

                            ContentBlock::ToolResult { .. } => {
                                // Tool results in assistant messages are rare, but handle them
                            }
                        }
                    }
                }
            }
        }

        EntryType::System => {
            // Skip system entries like stop_hook_summary
            // These are internal to Claude and not useful for display
        }

        EntryType::FileHistorySnapshot | EntryType::Summary => {
            // Skip file history and summary entries
        }
    }

    activities
}

/// Format a tool call for display
fn format_tool_call(name: &str, input: &serde_json::Value) -> String {
    match name {
        "Bash" => {
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                format!("Bash({})", cmd)
            } else {
                "Bash()".to_string()
            }
        }
        "Read" => {
            if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                format!("Read({})", path)
            } else {
                "Read()".to_string()
            }
        }
        "Write" => {
            if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                format!("Write({})", path)
            } else {
                "Write()".to_string()
            }
        }
        "Edit" => {
            if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                format!("Edit({})", path)
            } else {
                "Edit()".to_string()
            }
        }
        "Glob" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                format!("Glob({})", pattern)
            } else {
                "Glob()".to_string()
            }
        }
        "Grep" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                format!("Grep({})", pattern)
            } else {
                "Grep()".to_string()
            }
        }
        "Task" => {
            if let Some(desc) = input.get("description").and_then(|v| v.as_str()) {
                format!("Task({})", desc)
            } else {
                "Task()".to_string()
            }
        }
        _ => {
            // Generic format for unknown tools
            format!("{}()", name)
        }
    }
}

// ============================================================================
// File Reading
// ============================================================================

/// Parse a single JSONL line into an entry
pub fn parse_jsonl_line(line: &str) -> Result<JsonlEntry, JsonlError> {
    let entry: JsonlEntry = serde_json::from_str(line)?;
    Ok(entry)
}

/// Read all entries from a JSONL file
pub fn read_jsonl_file(path: &PathBuf) -> Result<Vec<JsonlEntry>, JsonlError> {
    if !path.exists() {
        return Err(JsonlError::FileNotFound(path.clone()));
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        match line_result {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }

                match parse_jsonl_line(&line) {
                    Ok(entry) => entries.push(entry),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse JSONL line {} in {:?}: {}",
                            line_num + 1,
                            path,
                            e
                        );
                        // Continue reading other lines
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to read line {} in {:?}: {}", line_num + 1, path, e);
            }
        }
    }

    tracing::info!("Read {} entries from {:?}", entries.len(), path);
    Ok(entries)
}

/// Read entries and convert to activities for display
pub fn read_activities(project_path: &str, conversation_id: &str) -> Result<Vec<Activity>, JsonlError> {
    let path = get_jsonl_path(project_path, conversation_id);

    if !path.exists() {
        tracing::info!("JSONL file not found: {:?}", path);
        return Ok(Vec::new());
    }

    let entries = read_jsonl_file(&path)?;

    let activities: Vec<Activity> = entries
        .iter()
        .flat_map(entry_to_activities)
        .collect();

    tracing::info!(
        "Converted {} JSONL entries to {} activities",
        entries.len(),
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
    fn test_encode_project_path() {
        assert_eq!(
            encode_project_path("/home/bigphoot/Desktop"),
            "-home-bigphoot-Desktop"
        );
        assert_eq!(
            encode_project_path("/home/user/project"),
            "-home-user-project"
        );
    }

    #[test]
    fn test_get_jsonl_path() {
        let path = get_jsonl_path("/home/bigphoot/Desktop", "abc-123");
        assert!(path.to_string_lossy().contains(".claude/projects/"));
        assert!(path.to_string_lossy().contains("-home-bigphoot-Desktop"));
        assert!(path.to_string_lossy().ends_with("abc-123.jsonl"));
    }

    #[test]
    fn test_parse_user_message() {
        let json = r#"{"type":"user","message":{"role":"user","content":"Hello world"},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        assert_eq!(entry.entry_type, EntryType::User);
        assert!(entry.message.is_some());

        let activities = entry_to_activities(&entry);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::UserPrompt);
        assert_eq!(activities[0].content, "Hello world");
    }

    #[test]
    fn test_parse_assistant_text() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I created the folder."}]},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        assert_eq!(entry.entry_type, EntryType::Assistant);

        let activities = entry_to_activities(&entry);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::Text);
        assert_eq!(activities[0].content, "I created the folder.");
    }

    #[test]
    fn test_parse_tool_use() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"Bash","input":{"command":"mkdir test"}}]},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        let activities = entry_to_activities(&entry);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::ToolStart);
        assert_eq!(activities[0].content, "Bash(mkdir test)");
        assert_eq!(activities[0].tool_name, Some("Bash".to_string()));
    }

    #[test]
    fn test_skip_system_entries() {
        let json = r#"{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-01-01T00:00:00Z"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        let activities = entry_to_activities(&entry);
        assert!(activities.is_empty());
    }

    #[test]
    fn test_parse_thinking_block() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me analyze this..."}]},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        let activities = entry_to_activities(&entry);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::Thinking);
        assert_eq!(activities[0].content, "Let me analyze this...");
    }

    #[test]
    fn test_parse_tool_result() {
        let json = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"File created successfully"}]},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        let activities = entry_to_activities(&entry);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::ToolResult);
        assert_eq!(activities[0].content, "File created successfully");
    }

    #[test]
    fn test_malformed_json_returns_error() {
        let result = parse_jsonl_line("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_content_array() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[]},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        let activities = entry_to_activities(&entry);
        assert!(activities.is_empty());
    }

    #[test]
    fn test_multiple_content_blocks() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"First message."},{"type":"text","text":"Second message."}]},"timestamp":"2026-01-01T00:00:00Z","uuid":"test-uuid"}"#;
        let entry = parse_jsonl_line(json).unwrap();

        let activities = entry_to_activities(&entry);
        assert_eq!(activities.len(), 2);
        assert_eq!(activities[0].content, "First message.");
        assert_eq!(activities[1].content, "Second message.");
    }
}
