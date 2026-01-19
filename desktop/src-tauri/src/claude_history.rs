// Claude History Reader - Reads conversation history from Claude's JSONL files

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

fn sanitize_plan_markup(content: &str) -> String {
    let mut sanitized = content.to_string();
    let xml_tag = Regex::new(r"</?command-[^>]+>").ok();
    let local_tag = Regex::new(r"</?local-command-[^>]+>").ok();
    if let Some(re) = xml_tag {
        sanitized = re.replace_all(&sanitized, "").to_string();
    }
    if let Some(re) = local_tag {
        sanitized = re.replace_all(&sanitized, "").to_string();
    }
    sanitized
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_tool_result(content: &str) -> String {
    let stripped = content.replace('\n', "").replace('\r', "").replace(' ', "");
    if stripped.len() >= 200 {
        let base64_chars = stripped
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
            .count();
        if base64_chars as f32 / stripped.len() as f32 >= 0.9 {
            return "[binary output omitted]".to_string();
        }
    }
    content.to_string()
}

#[derive(Debug, Deserialize)]
struct JsonlEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<MessageContent>,
    #[serde(rename = "isoTimestamp")]
    iso_timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        name: String,
    },
    ToolResult {
        content: Option<String>,
    },
    /// Catch-all for unknown content block types
    Other(serde_json::Value),
}

/// Convert a project path to Claude's directory name format
fn project_path_to_claude_dir(project_path: &str) -> String {
    // Claude converts paths like "/home/user/project" to "-home-user-project"
    project_path.replace('/', "-")
}

/// Get the path to Claude's conversation file
fn get_conversation_file_path(project_path: &str, conversation_id: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let claude_dir = project_path_to_claude_dir(project_path);

    let path = PathBuf::from(&home)
        .join(".claude")
        .join("projects")
        .join(&claude_dir)
        .join(format!("{}.jsonl", conversation_id));

    if path.exists() {
        Some(path)
    } else {
        tracing::warn!("Claude conversation file not found: {:?}", path);
        None
    }
}

/// Read conversation history from Claude's JSONL file
pub fn read_conversation_history(
    project_path: &str,
    conversation_id: &str,
    limit: usize,
) -> Result<Vec<ConversationMessage>, String> {
    let file_path = get_conversation_file_path(project_path, conversation_id)
        .ok_or_else(|| "Conversation file not found".to_string())?;

    let file =
        File::open(&file_path).map_err(|e| format!("Failed to open conversation file: {}", e))?;

    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        let entry: JsonlEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Only process user and assistant messages
        if entry.entry_type != "user" && entry.entry_type != "assistant" {
            continue;
        }

        // Extract content from message
        let content = if let Some(msg) = &entry.message {
            if let Some(content_blocks) = &msg.content {
                let mut text_parts = Vec::new();
                for block in content_blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            text_parts.push(sanitize_plan_markup(text));
                        }
                        ContentBlock::ToolUse { name } => {
                            text_parts.push(format!("[Using tool: {}]", sanitize_plan_markup(name)));
                        }
                        ContentBlock::ToolResult { content } => {
                            if let Some(c) = content {
                                let sanitized = sanitize_tool_result(c);
                                let truncated = if sanitized.len() > 200 {
                                    format!("{}...", &sanitized[..200])
                                } else {
                                    sanitized
                                };
                                text_parts.push(format!("[Tool result: {}]", sanitize_plan_markup(&truncated)));
                            }
                        }
                        ContentBlock::Other(_) => {}
                    }
                }
                text_parts.retain(|part| !part.trim().is_empty());
                text_parts.join("\n")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Skip empty messages
        if content.is_empty() {
            continue;
        }

        let content = sanitize_plan_markup(&content);
        if content.is_empty() {
            continue;
        }

        messages.push(ConversationMessage {
            role: entry.entry_type,
            content,
            timestamp: entry.iso_timestamp,
        });
    }

    // Return the last N messages
    let start = if messages.len() > limit {
        messages.len() - limit
    } else {
        0
    };

    Ok(messages[start..].to_vec())
}
