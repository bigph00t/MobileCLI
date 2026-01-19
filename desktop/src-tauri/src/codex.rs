//! Codex Module - Read and parse OpenAI Codex CLI conversation logs
//!
//! Codex stores structured conversation data at:
//! ~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl
//!
//! This module reads these files to get clean, structured activities
//! for the mobile app.

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
pub enum CodexError {
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

/// Get the Codex sessions directory
pub fn get_codex_sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    let codex_home = std::env::var("CODEX_HOME").unwrap_or_else(|_| format!("{}/.codex", home));
    PathBuf::from(format!("{}/sessions", codex_home))
}

/// Find the JSONL file for a Codex session by ID
/// Searches through date-organized directories
pub fn find_session_file(session_id: &str) -> Option<PathBuf> {
    let sessions_dir = get_codex_sessions_dir();
    if !sessions_dir.exists() {
        return None;
    }

    // Walk the directory tree looking for matching rollout file
    for year_entry in std::fs::read_dir(&sessions_dir).ok()? {
        let year_dir = year_entry.ok()?.path();
        if !year_dir.is_dir() {
            continue;
        }

        for month_entry in std::fs::read_dir(&year_dir).ok()? {
            let month_dir = month_entry.ok()?.path();
            if !month_dir.is_dir() {
                continue;
            }

            for day_entry in std::fs::read_dir(&month_dir).ok()? {
                let day_dir = day_entry.ok()?.path();
                if !day_dir.is_dir() {
                    continue;
                }

                for file_entry in std::fs::read_dir(&day_dir).ok()? {
                    let file_path = file_entry.ok()?.path();
                    if let Some(filename) = file_path.file_name() {
                        let filename_str = filename.to_string_lossy();
                        if filename_str.contains(session_id) && filename_str.ends_with(".jsonl") {
                            return Some(file_path);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Get the most recent Codex session file (for resume)
pub fn get_latest_session_file() -> Option<PathBuf> {
    let sessions_dir = get_codex_sessions_dir();
    if !sessions_dir.exists() {
        return None;
    }

    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;

    // Walk the directory tree
    for year_entry in std::fs::read_dir(&sessions_dir).ok()? {
        let year_dir = year_entry.ok()?.path();
        if !year_dir.is_dir() {
            continue;
        }

        for month_entry in std::fs::read_dir(&year_dir).ok()? {
            let month_dir = month_entry.ok()?.path();
            if !month_dir.is_dir() {
                continue;
            }

            for day_entry in std::fs::read_dir(&month_dir).ok()? {
                let day_dir = day_entry.ok()?.path();
                if !day_dir.is_dir() {
                    continue;
                }

                for file_entry in std::fs::read_dir(&day_dir).ok()? {
                    let file_path = file_entry.ok()?.path();
                    if file_path.extension().map_or(false, |e| e == "jsonl") {
                        if let Ok(metadata) = file_path.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                match &latest {
                                    None => latest = Some((file_path, modified)),
                                    Some((_, latest_time)) if modified > *latest_time => {
                                        latest = Some((file_path, modified));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    latest.map(|(path, _)| path)
}

// ============================================================================
// JSONL Entry Types (matching Codex format)
// ============================================================================

/// The type field in a Codex JSONL record
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexRecordType {
    SessionMeta,
    ResponseItem,
    EventMsg,
    #[serde(other)]
    Unknown,
}

/// Content item in a response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    InputText {
        text: String,
    },
    OutputText {
        text: String,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        id: Option<String>,
        name: String,
        arguments: String,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        call_id: Option<String>,
        output: String,
    },
    #[serde(other)]
    Other,
}

/// A response item payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseItemPayload {
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub role: Option<String>,
    pub content: Option<Vec<ContentItem>>,
}

/// Session metadata payload

/// A single Codex JSONL record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRecord {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub record_type: CodexRecordType,
    pub payload: serde_json::Value,
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

/// Convert a Codex record into activities for display
pub fn record_to_activities(record: &CodexRecord) -> Vec<Activity> {
    let mut activities = Vec::new();
    let timestamp = record.timestamp.clone();

    match record.record_type {
        CodexRecordType::ResponseItem => {
            // Try to parse as ResponseItemPayload
            if let Ok(payload) =
                serde_json::from_value::<ResponseItemPayload>(record.payload.clone())
            {
                let role = payload.role.as_deref().unwrap_or("");

                if let Some(content) = payload.content {
                    for item in content {
                        match item {
                            ContentItem::InputText { text } => {
                                // User input
                                if !text.trim().is_empty()
                                    && (role == "user" || role == "developer")
                                {
                                    activities.push(Activity::new(
                                        ActivityType::UserPrompt,
                                        text,
                                        timestamp.clone(),
                                    ));
                                }
                            }
                            ContentItem::OutputText { text } => {
                                // Assistant response
                                if !text.trim().is_empty() {
                                    activities.push(Activity::new(
                                        ActivityType::Text,
                                        text,
                                        timestamp.clone(),
                                    ));
                                }
                            }
                            ContentItem::FunctionCall {
                                id,
                                name,
                                arguments,
                            } => {
                                // Tool call
                                let content = format_tool_call(&name, &arguments);
                                activities.push(
                                    Activity::new(
                                        ActivityType::ToolStart,
                                        content,
                                        timestamp.clone(),
                                    )
                                    .with_uuid(id)
                                    .with_tool(name, Some(arguments)),
                                );
                            }
                            ContentItem::FunctionCallOutput { output, .. } => {
                                // Tool result
                                if !output.trim().is_empty() {
                                    activities.push(Activity::new(
                                        ActivityType::ToolResult,
                                        output,
                                        timestamp.clone(),
                                    ));
                                }
                            }
                            ContentItem::Other => {}
                        }
                    }
                }
            }
        }
        CodexRecordType::SessionMeta => {
            // Skip session metadata for activity display
        }
        CodexRecordType::EventMsg | CodexRecordType::Unknown => {
            // Try to extract user messages from EventMsg
            if let Some(msg_type) = record.payload.get("type").and_then(|v| v.as_str()) {
                if msg_type == "UserMessage" {
                    if let Some(text) = record.payload.get("text").and_then(|v| v.as_str()) {
                        if !text.trim().is_empty() {
                            activities.push(Activity::new(
                                ActivityType::UserPrompt,
                                text.to_string(),
                                timestamp,
                            ));
                        }
                    }
                }
            }
        }
    }

    activities
}

/// Format a tool call for display
fn format_tool_call(name: &str, arguments: &str) -> String {
    // Try to parse arguments as JSON to extract key info
    if let Ok(args) = serde_json::from_str::<serde_json::Value>(arguments) {
        match name {
            "shell" | "bash" | "execute_command" => {
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    return format!("Bash({})", cmd);
                }
            }
            "read_file" | "read" => {
                if let Some(path) = args
                    .get("path")
                    .or_else(|| args.get("file_path"))
                    .and_then(|v| v.as_str())
                {
                    return format!("Read({})", path);
                }
            }
            "write_file" | "write" => {
                if let Some(path) = args
                    .get("path")
                    .or_else(|| args.get("file_path"))
                    .and_then(|v| v.as_str())
                {
                    return format!("Write({})", path);
                }
            }
            "edit_file" | "apply_diff" => {
                if let Some(path) = args
                    .get("path")
                    .or_else(|| args.get("file_path"))
                    .and_then(|v| v.as_str())
                {
                    return format!("Edit({})", path);
                }
            }
            _ => {}
        }
    }
    format!("{}()", name)
}

// ============================================================================
// File Reading
// ============================================================================

/// Parse a single JSONL line into a record
pub fn parse_codex_line(line: &str) -> Result<CodexRecord, CodexError> {
    let record: CodexRecord = serde_json::from_str(line)?;
    Ok(record)
}

/// Read all records from a Codex JSONL file
pub fn read_codex_file(path: &PathBuf) -> Result<Vec<CodexRecord>, CodexError> {
    if !path.exists() {
        return Err(CodexError::FileNotFound(path.clone()));
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        match line_result {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }

                match parse_codex_line(&line) {
                    Ok(record) => records.push(record),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse Codex JSONL line {} in {:?}: {}",
                            line_num + 1,
                            path,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to read line {} in {:?}: {}", line_num + 1, path, e);
            }
        }
    }

    tracing::info!("Read {} Codex records from {:?}", records.len(), path);
    Ok(records)
}

/// Read records and convert to activities for display
pub fn read_activities(session_id: &str) -> Result<Vec<Activity>, CodexError> {
    let path = match find_session_file(session_id) {
        Some(p) => p,
        None => {
            tracing::info!("Codex session file not found for: {}", session_id);
            return Ok(Vec::new());
        }
    };

    let records = read_codex_file(&path)?;

    let activities: Vec<Activity> = records.iter().flat_map(record_to_activities).collect();

    tracing::info!(
        "Converted {} Codex records to {} activities",
        records.len(),
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
    fn test_get_codex_sessions_dir() {
        let dir = get_codex_sessions_dir();
        assert!(dir.to_string_lossy().contains(".codex/sessions"));
    }

    #[test]
    fn test_parse_session_meta() {
        let json = r#"{"timestamp":"2026-01-15T20:25:44.682Z","type":"session_meta","payload":{"id":"test-uuid","cwd":"/home/user/project","originator":"codex_cli_rs"}}"#;
        let record = parse_codex_line(json).unwrap();

        assert_eq!(record.record_type, CodexRecordType::SessionMeta);
        assert_eq!(record.timestamp, "2026-01-15T20:25:44.682Z");
    }

    #[test]
    fn test_parse_user_message() {
        let json = r#"{"timestamp":"2026-01-15T20:26:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Hello Codex"}]}}"#;
        let record = parse_codex_line(json).unwrap();

        let activities = record_to_activities(&record);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::UserPrompt);
        assert_eq!(activities[0].content, "Hello Codex");
    }

    #[test]
    fn test_parse_assistant_response() {
        let json = r#"{"timestamp":"2026-01-15T20:26:01.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello! How can I help?"}]}}"#;
        let record = parse_codex_line(json).unwrap();

        let activities = record_to_activities(&record);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::Text);
        assert_eq!(activities[0].content, "Hello! How can I help?");
    }

    #[test]
    fn test_parse_function_call() {
        let json = r#"{"timestamp":"2026-01-15T20:26:02.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"function_call","id":"call-123","name":"shell","arguments":"{\"command\":\"ls -la\"}"}]}}"#;
        let record = parse_codex_line(json).unwrap();

        let activities = record_to_activities(&record);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, ActivityType::ToolStart);
        assert_eq!(activities[0].content, "Bash(ls -la)");
        assert_eq!(activities[0].tool_name, Some("shell".to_string()));
    }

    #[test]
    fn test_skip_session_meta_activities() {
        let json = r#"{"timestamp":"2026-01-15T20:25:44.682Z","type":"session_meta","payload":{"id":"test"}}"#;
        let record = parse_codex_line(json).unwrap();

        let activities = record_to_activities(&record);
        assert!(activities.is_empty());
    }
}
