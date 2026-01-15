// Database module - SQLite operations for sessions and messages

use rusqlite::{params, Connection, Result as SqliteResult};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

pub struct Database {
    conn: Mutex<Connection>,
}

/// Supported CLI types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliType {
    ClaudeCode,
    GeminiCli,
    OpenCode,
    Codex,
}

/// Tool approval interaction model - different CLIs use different input methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalModel {
    /// Claude/Gemini/Codex: 1=Yes, 2=Yes Always, 3=No
    NumberedOptions,
    /// Reserved for CLIs that use simple y/n input (not currently used)
    #[allow(dead_code)]
    YesNo,
    /// OpenCode: arrow keys to navigate, Enter to select
    ArrowNavigation,
}

/// User's response to a tool approval prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalResponse {
    /// Allow the tool execution once
    Yes,
    /// Allow the tool execution and don't ask again for this tool
    YesAlways,
    /// Deny the tool execution
    No,
}

impl ApprovalResponse {
    /// Get the input string to send to the PTY for this approval response
    /// based on the CLI type's approval model
    pub fn get_input_for_cli(&self, cli_type: CliType) -> &'static str {
        match (cli_type.approval_model(), self) {
            // Claude: numbered options
            (ApprovalModel::NumberedOptions, ApprovalResponse::Yes) => "1",
            (ApprovalModel::NumberedOptions, ApprovalResponse::YesAlways) => "2",
            (ApprovalModel::NumberedOptions, ApprovalResponse::No) => "3",

            // Gemini/Codex: y/n (YesAlways falls back to Yes)
            (ApprovalModel::YesNo, ApprovalResponse::Yes) => "y",
            (ApprovalModel::YesNo, ApprovalResponse::YesAlways) => "y", // No "always" option for y/n
            (ApprovalModel::YesNo, ApprovalResponse::No) => "n",

            // OpenCode: arrow navigation - returns escape sequences
            // Yes = Enter (first option selected by default)
            // YesAlways = Right arrow then Enter
            // No = Right arrow twice then Enter
            (ApprovalModel::ArrowNavigation, ApprovalResponse::Yes) => "\r",
            (ApprovalModel::ArrowNavigation, ApprovalResponse::YesAlways) => "\x1b[C\r", // Right + Enter
            (ApprovalModel::ArrowNavigation, ApprovalResponse::No) => "\x1b[C\x1b[C\r", // Right + Right + Enter
        }
    }
}

impl CliType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CliType::ClaudeCode => "claude",
            CliType::GeminiCli => "gemini",
            CliType::OpenCode => "opencode",
            CliType::Codex => "codex",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(CliType::ClaudeCode),
            "gemini" => Some(CliType::GeminiCli),
            "opencode" => Some(CliType::OpenCode),
            "codex" => Some(CliType::Codex),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            CliType::ClaudeCode => "Claude Code",
            CliType::GeminiCli => "Gemini CLI",
            CliType::OpenCode => "OpenCode",
            CliType::Codex => "Codex",
        }
    }

    pub fn command(&self) -> &'static str {
        match self {
            CliType::ClaudeCode => "claude",
            CliType::GeminiCli => "gemini",
            CliType::OpenCode => "opencode",
            CliType::Codex => "codex",
        }
    }

    /// Whether this CLI supports session resume via command line
    pub fn supports_resume(&self) -> bool {
        match self {
            CliType::ClaudeCode => true,
            CliType::GeminiCli => true,
            CliType::OpenCode => true,  // via -c flag
            CliType::Codex => true,     // via resume command
        }
    }

    /// Get the tool approval interaction model for this CLI
    pub fn approval_model(&self) -> ApprovalModel {
        match self {
            // Claude, Gemini, and Codex all use numbered options (1, 2, 3)
            // Screenshot confirmed Gemini shows: "1. Allow once", "2. Allow for this session", "3. No"
            CliType::ClaudeCode => ApprovalModel::NumberedOptions,
            CliType::GeminiCli => ApprovalModel::NumberedOptions,
            CliType::Codex => ApprovalModel::NumberedOptions,
            // OpenCode uses arrow key navigation
            CliType::OpenCode => ApprovalModel::ArrowNavigation,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub name: String,
    pub project_path: String,
    pub created_at: String,
    pub last_active_at: String,
    pub status: String,
    pub conversation_id: Option<String>, // CLI-specific session ID for resume
    pub cli_type: String,                 // "claude" or "gemini"
}

/// DEPRECATED: JSONL is now the primary source for messages.
/// This struct is kept for backwards compatibility and DB fallback.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_result: Option<String>,
    pub timestamp: String,
}

impl Database {
    pub fn new(path: &Path) -> SqliteResult<Self> {
        let conn = Connection::open(path)?;

        // Create tables
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                project_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                conversation_id TEXT,
                cli_type TEXT NOT NULL DEFAULT 'claude'
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_name TEXT,
                tool_result TEXT,
                timestamp TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
            CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
            ",
        )?;

        // Migration: Add conversation_id column if it doesn't exist
        let _ = conn.execute(
            "ALTER TABLE sessions ADD COLUMN conversation_id TEXT",
            [],
        );

        // Migration: Add cli_type column if it doesn't exist (default to 'claude' for existing sessions)
        let _ = conn.execute(
            "ALTER TABLE sessions ADD COLUMN cli_type TEXT NOT NULL DEFAULT 'claude'",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn create_session(&self, name: &str, project_path: &str, cli_type: CliType) -> SqliteResult<SessionRecord> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let cli_type_str = cli_type.as_str();

        conn.execute(
            "INSERT INTO sessions (id, name, project_path, created_at, last_active_at, status, conversation_id, cli_type)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', NULL, ?6)",
            params![id, name, project_path, now, now, cli_type_str],
        )?;

        Ok(SessionRecord {
            id,
            name: name.to_string(),
            project_path: project_path.to_string(),
            created_at: now.clone(),
            last_active_at: now,
            status: "active".to_string(),
            conversation_id: None,
            cli_type: cli_type_str.to_string(),
        })
    }

    pub fn get_session(&self, id: &str) -> SqliteResult<Option<SessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, project_path, created_at, last_active_at, status, conversation_id, cli_type
             FROM sessions WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SessionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                project_path: row.get(2)?,
                created_at: row.get(3)?,
                last_active_at: row.get(4)?,
                status: row.get(5)?,
                conversation_id: row.get(6)?,
                cli_type: row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "claude".to_string()),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_sessions(&self) -> SqliteResult<Vec<SessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, project_path, created_at, last_active_at, status, conversation_id, cli_type
             FROM sessions ORDER BY last_active_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                project_path: row.get(2)?,
                created_at: row.get(3)?,
                last_active_at: row.get(4)?,
                status: row.get(5)?,
                conversation_id: row.get(6)?,
                cli_type: row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "claude".to_string()),
            })
        })?;

        rows.collect()
    }

    pub fn update_conversation_id(&self, session_id: &str, conversation_id: &str) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET conversation_id = ?1 WHERE id = ?2",
            params![conversation_id, session_id],
        )?;
        Ok(())
    }

    pub fn update_session_status(&self, id: &str, status: &str) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE sessions SET status = ?1, last_active_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )?;

        Ok(())
    }

    /// Close all active sessions - used on app startup to clean up orphaned sessions
    /// whose PTY processes died when the app closed
    pub fn close_all_active_sessions(&self) -> SqliteResult<usize> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        let count = conn.execute(
            "UPDATE sessions SET status = 'closed', last_active_at = ?1 WHERE status = 'active'",
            params![now],
        )?;

        Ok(count)
    }

    pub fn update_session_activity(&self, id: &str) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE sessions SET last_active_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;

        Ok(())
    }

    /// DEPRECATED: JSONL is now the source of truth for messages.
    /// This function is kept for backwards compatibility with non-Claude CLIs.
    #[allow(dead_code)]
    pub fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        tool_name: Option<&str>,
        tool_result: Option<&str>,
    ) -> SqliteResult<MessageRecord> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO messages (id, session_id, role, content, tool_name, tool_result, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, session_id, role, content, tool_name, tool_result, now],
        )?;

        // Update session activity
        drop(conn);
        self.update_session_activity(session_id)?;

        Ok(MessageRecord {
            id,
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            tool_name: tool_name.map(String::from),
            tool_result: tool_result.map(String::from),
            timestamp: now,
        })
    }

    pub fn get_messages(&self, session_id: &str, limit: i64) -> SqliteResult<Vec<MessageRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, tool_name, tool_result, timestamp
             FROM messages WHERE session_id = ?1
             ORDER BY timestamp DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![session_id, limit], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_name: row.get(4)?,
                tool_result: row.get(5)?,
                timestamp: row.get(6)?,
            })
        })?;

        let mut messages: Vec<MessageRecord> = rows.collect::<SqliteResult<Vec<_>>>()?;
        messages.reverse(); // Return in chronological order
        Ok(messages)
    }

    /// DEPRECATED: JSONL is now the source of truth for messages.
    #[allow(dead_code)]
    pub fn update_message_content(&self, id: &str, content: &str) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET content = ?1 WHERE id = ?2",
            params![content, id],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn rename_session(&self, id: &str, new_name: &str) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_test_db() -> (Database, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();
        (db, dir)
    }

    #[test]
    fn test_cli_type_roundtrip() {
        assert_eq!(CliType::from_str(CliType::ClaudeCode.as_str()), Some(CliType::ClaudeCode));
        assert_eq!(CliType::from_str(CliType::GeminiCli.as_str()), Some(CliType::GeminiCli));
        assert_eq!(CliType::from_str(CliType::OpenCode.as_str()), Some(CliType::OpenCode));
        assert_eq!(CliType::from_str(CliType::Codex.as_str()), Some(CliType::Codex));
        assert_eq!(CliType::from_str("invalid"), None);
    }

    #[test]
    fn test_cli_type_display_name() {
        assert_eq!(CliType::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(CliType::GeminiCli.display_name(), "Gemini CLI");
        assert_eq!(CliType::OpenCode.display_name(), "OpenCode");
        assert_eq!(CliType::Codex.display_name(), "Codex");
    }

    #[test]
    fn test_cli_type_supports_resume() {
        assert!(CliType::ClaudeCode.supports_resume());
        assert!(CliType::GeminiCli.supports_resume());
        assert!(CliType::OpenCode.supports_resume());
        assert!(CliType::Codex.supports_resume());
    }

    #[test]
    fn test_cli_type_approval_model() {
        // Claude, Gemini, and Codex all use numbered options (1, 2, 3)
        assert_eq!(CliType::ClaudeCode.approval_model(), ApprovalModel::NumberedOptions);
        assert_eq!(CliType::GeminiCli.approval_model(), ApprovalModel::NumberedOptions);
        assert_eq!(CliType::Codex.approval_model(), ApprovalModel::NumberedOptions);
        // OpenCode uses arrow navigation
        assert_eq!(CliType::OpenCode.approval_model(), ApprovalModel::ArrowNavigation);
    }

    #[test]
    fn test_create_session() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("Test Session", "/tmp/test", CliType::ClaudeCode).unwrap();

        assert_eq!(session.name, "Test Session");
        assert_eq!(session.project_path, "/tmp/test");
        assert_eq!(session.cli_type, "claude");
        assert_eq!(session.status, "active");
    }

    #[test]
    fn test_get_session() {
        let (db, _dir) = setup_test_db();

        let created = db.create_session("Test", "/tmp/test", CliType::GeminiCli).unwrap();
        let loaded = db.get_session(&created.id).unwrap().unwrap();

        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded.cli_type, "gemini");
    }

    #[test]
    fn test_get_nonexistent_session() {
        let (db, _dir) = setup_test_db();
        let result = db.get_session("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_all_sessions() {
        let (db, _dir) = setup_test_db();

        db.create_session("Session 1", "/tmp/1", CliType::ClaudeCode).unwrap();
        db.create_session("Session 2", "/tmp/2", CliType::GeminiCli).unwrap();

        let sessions = db.get_all_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_update_session_status() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("Test", "/tmp/test", CliType::ClaudeCode).unwrap();
        db.update_session_status(&session.id, "closed").unwrap();

        let loaded = db.get_session(&session.id).unwrap().unwrap();
        assert_eq!(loaded.status, "closed");
    }

    #[test]
    fn test_update_conversation_id() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("Test", "/tmp/test", CliType::ClaudeCode).unwrap();
        db.update_conversation_id(&session.id, "conv-123").unwrap();

        let loaded = db.get_session(&session.id).unwrap().unwrap();
        assert_eq!(loaded.conversation_id, Some("conv-123".to_string()));
    }

    #[test]
    fn test_rename_session() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("Old Name", "/tmp/test", CliType::ClaudeCode).unwrap();
        db.rename_session(&session.id, "New Name").unwrap();

        let loaded = db.get_session(&session.id).unwrap().unwrap();
        assert_eq!(loaded.name, "New Name");
    }

    #[test]
    fn test_delete_session() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("To Delete", "/tmp/test", CliType::ClaudeCode).unwrap();
        db.delete_session(&session.id).unwrap();

        let loaded = db.get_session(&session.id).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_add_and_get_messages() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("Test", "/tmp/test", CliType::ClaudeCode).unwrap();

        db.add_message(&session.id, "user", "Hello!", None, None).unwrap();
        db.add_message(&session.id, "assistant", "Hi there!", None, None).unwrap();

        let messages = db.get_messages(&session.id, 10).unwrap();
        assert_eq!(messages.len(), 2);

        // Check both messages exist (order may vary when timestamps are identical)
        let roles: Vec<&str> = messages.iter().map(|m| m.role.as_str()).collect();
        assert!(roles.contains(&"user"));
        assert!(roles.contains(&"assistant"));

        // Check content
        let contents: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"Hello!"));
        assert!(contents.contains(&"Hi there!"));
    }

    #[test]
    fn test_close_all_active_sessions() {
        let (db, _dir) = setup_test_db();

        db.create_session("Active 1", "/tmp/1", CliType::ClaudeCode).unwrap();
        db.create_session("Active 2", "/tmp/2", CliType::GeminiCli).unwrap();

        let closed_count = db.close_all_active_sessions().unwrap();
        assert_eq!(closed_count, 2);

        let sessions = db.get_all_sessions().unwrap();
        for session in sessions {
            assert_eq!(session.status, "closed");
        }
    }

    #[test]
    fn test_sql_injection_prevention() {
        let (db, _dir) = setup_test_db();

        // Create a session with a name that could be SQL injection
        let malicious_name = "'; DROP TABLE sessions; --";
        let session = db.create_session(malicious_name, "/tmp/test", CliType::ClaudeCode);

        // Should succeed (parameterized query prevents injection)
        assert!(session.is_ok());

        // Tables should still exist
        let all = db.get_all_sessions();
        assert!(all.is_ok());
    }

    #[test]
    fn test_session_activity_update() {
        let (db, _dir) = setup_test_db();

        let session = db.create_session("Test", "/tmp/test", CliType::ClaudeCode).unwrap();
        let original_activity = session.last_active_at.clone();

        std::thread::sleep(std::time::Duration::from_millis(10));
        db.update_session_activity(&session.id).unwrap();

        let loaded = db.get_session(&session.id).unwrap().unwrap();
        assert_ne!(loaded.last_active_at, original_activity);
    }
}
