// PTY module - Manages AI CLI processes in pseudo-terminals

use crate::db::{CliType, Database};
use crate::jsonl_watcher::JsonlWatcher;
use crate::parser::OutputParser;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Lock error")]
    Lock,
}

struct PtySession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    _reader_task: JoinHandle<()>,
    _kill_tx: mpsc::Sender<()>,
    /// Channel to signal user input was sent (for parser tracking)
    user_input_tx: mpsc::Sender<()>,
    /// JSONL file watcher for Claude sessions (None for other CLI types)
    /// Kept alive for its side effects (background thread watching for file changes)
    #[allow(dead_code)]
    jsonl_watcher: Option<JsonlWatcher>,
}

/// Detect dynamic thinking/progress messages from Claude's PTY output
/// and emit them as activity events for mobile display.
///
/// Claude shows orange status text like:
/// - "Ideating", "Fermenting", "Brewing" (single-word thinking states)
/// - "Building core pages with placeholders..." (dynamic progress messages)
/// - "Discussing monetization and GitHub strategy..." (longer status updates)
fn detect_and_emit_thinking(cleaned: &str, session_id: &str, app: &AppHandle) {
    // Simple thinking words from Claude Code v2.1+
    static THINKING_WORDS: &[&str] = &[
        "Ideating", "Fermenting", "Kneading", "Pollinating", "Fluttering",
        "Brewing", "Crafting", "Weaving", "Spinning", "Stewing",
        "Marinating", "Simmering", "Steeping", "Jitterbugging", "Pondering",
        "Contemplating", "Musing", "Philosophising", "Ruminating",
        "Deliberating", "Cogitating", "Dilly-dallying", "Levitating",
    ];

    // Braille spinner characters that Claude uses for animation
    static SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];


    // Check each line for thinking indicators
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip lines that are clearly not status messages
        if trimmed.starts_with('●') || trimmed.starts_with('⎿') || trimmed.starts_with('>') {
            continue;
        }

        // Strip spinner characters from the beginning for detection
        let mut content_to_check = trimmed;
        for c in SPINNER_CHARS {
            if let Some(rest) = trimmed.strip_prefix(*c) {
                content_to_check = rest.trim_start();
                break;
            }
        }

        let mut is_thinking = false;
        let mut thinking_content = String::new();

        // Check for simple thinking words (with or without spinner)
        for word in THINKING_WORDS {
            if content_to_check.contains(word) || content_to_check.eq_ignore_ascii_case(word) {
                is_thinking = true;
                thinking_content = content_to_check.to_string();
                break;
            }
        }

        // Check for dynamic progress messages (lines ending with ... that look like status)
        // These are typically short action descriptions
        if !is_thinking && content_to_check.ends_with("...") && content_to_check.len() < 100 {
            // Filter out lines that are actual content (have response markers)
            // Progress messages are typically clean status text
            let has_special_chars = content_to_check.chars().any(|c| {
                matches!(c, '●' | '⎿' | '│' | '├' | '└' | '┌' | '┐' | '┘' | '┴' | '┬')
            });

            if !has_special_chars {
                is_thinking = true;
                thinking_content = content_to_check.to_string();
            }
        }

        // Also check for "thinking", "thought for X" patterns
        if !is_thinking {
            let lower = content_to_check.to_lowercase();
            if lower.contains("thinking") || lower.contains("thought for") || lower.contains("esc to interrupt") {
                is_thinking = true;
                thinking_content = content_to_check.to_string();
            }
        }

        // Also detect lines that START with spinner characters (dynamic progress)
        // These are Claude's "Building core pages...", "Discussing monetization..." messages
        if !is_thinking && SPINNER_CHARS.iter().any(|c| trimmed.starts_with(*c)) {
            // If line has spinner and meaningful text after it, it's a progress message
            if !content_to_check.is_empty() && content_to_check.len() > 3 {
                is_thinking = true;
                thinking_content = content_to_check.to_string();
            }
        }

        // Emit thinking activity for mobile
        if is_thinking && !thinking_content.is_empty() {
            // Clean up the content - extract just the thinking word/phrase
            // Remove parenthetical info like "(ctrl+c to interrupt · thinking)"
            let clean_content = if let Some(paren_pos) = thinking_content.find('(') {
                thinking_content[..paren_pos].trim().to_string()
            } else {
                thinking_content.clone()
            };

            // Remove leading special characters (✢, *, etc.)
            let clean_content = clean_content
                .trim_start_matches(|c: char| !c.is_alphabetic())
                .trim()
                .to_string();

            // Only emit if we still have meaningful content
            if !clean_content.is_empty() && clean_content.len() > 2 {
                tracing::debug!("[THINKING_DETECT] Emitting: {:?}", clean_content);
                let _ = app.emit(
                    "activity",
                    serde_json::json!({
                        "sessionId": session_id,
                        "activityType": "thinking",
                        "content": clean_content,
                        "isStreaming": true,  // Mark as streaming so it gets replaced when real content arrives
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }),
                );
            }
        }
    }
}

pub struct SessionManager {
    sessions: HashMap<String, PtySession>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub async fn start_session(
        &mut self,
        session_id: String,
        project_path: String,
        cli_type: CliType,
        db: Arc<Database>,
        app: AppHandle,
    ) -> Result<(), PtyError> {
        let pty_system = native_pty_system();

        // Create PTY with reasonable size
        let pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        // Build command based on CLI type
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/bigphoot".to_string());

        // Generate a conversation ID for Claude Code sessions
        let conversation_id = uuid::Uuid::new_v4().to_string();

        let cmd = match cli_type {
            CliType::ClaudeCode => {
                let claude_path = format!("{}/.local/bin/claude", home);
                let mut cmd = CommandBuilder::new(&claude_path);
                // Pass our generated session ID so we can resume later
                cmd.arg("--session-id");
                cmd.arg(&conversation_id);
                cmd.cwd(&project_path);
                cmd
            }
            CliType::GeminiCli => {
                // Gemini CLI is typically installed via npm
                let mut cmd = CommandBuilder::new("gemini");
                cmd.cwd(&project_path);
                cmd
            }
        };

        // Apply common environment setup
        let mut cmd = cmd;
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.env("HOME", &home);
        cmd.env("TERM", "xterm-256color");
        if let Ok(shell) = std::env::var("SHELL") {
            cmd.env("SHELL", shell);
        }

        tracing::info!("Starting {} in {}", cli_type.display_name(), project_path);

        // Store conversation ID for all CLI types - Claude uses it for resume, others for tracking
        let _ = db.update_conversation_id(&session_id, &conversation_id);
        tracing::info!("Set conversation ID for session {}: {}", session_id, conversation_id);

        // Spawn the CLI process with retry on failure
        let mut child = {
            let max_retries = 3;
            let mut result: Result<Box<dyn portable_pty::Child + Send + Sync>, String> =
                Err("No spawn attempt made".to_string());

            for attempt in 0..max_retries {
                match pair.slave.spawn_command(cmd.clone()) {
                    Ok(child) => {
                        if attempt > 0 {
                            tracing::info!(
                                "PTY spawn succeeded on attempt {} for {}",
                                attempt + 1,
                                cli_type.display_name()
                            );
                        }
                        result = Ok(child);
                        break;
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        tracing::warn!(
                            "PTY spawn failed (attempt {}/{}): {}",
                            attempt + 1,
                            max_retries,
                            err_msg
                        );
                        result = Err(err_msg);

                        if attempt < max_retries - 1 {
                            // Wait before retrying with exponential backoff
                            let delay = std::time::Duration::from_millis(100 * (attempt as u64 + 1));
                            std::thread::sleep(delay);
                        }
                    }
                }
            }

            result.map_err(|e| {
                PtyError::Pty(format!(
                    "Failed to spawn {} after {} attempts: {}",
                    cli_type.display_name(),
                    max_retries,
                    e
                ))
            })?
        };

        // Get writer for sending input (wrapped in Arc<Mutex> for interior mutability)
        let writer = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .map_err(|e| PtyError::Pty(e.to_string()))?,
        ));

        // Get reader for capturing output
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        // Keep master for resize operations
        let master = Arc::new(Mutex::new(pair.master));

        // Channel for killing the session
        let (kill_tx, _kill_rx) = mpsc::channel::<()>(1);

        // Channel for signaling user input to the parser
        let (user_input_tx, mut user_input_rx) = mpsc::channel::<()>(16);

        // Clone writer for reader task to use for auto-accept
        let writer_for_reader = writer.clone();

        // Clone app for JSONL watcher (before it's moved into the reader task)
        let app_for_watcher = app.clone();
        let project_path_for_watcher = project_path.clone();
        let conversation_id_for_watcher = conversation_id.clone();

        // Spawn task to read PTY output
        let session_id_clone = session_id.clone();
        let cli_type_for_parser = cli_type; // Copy for the spawned task
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut parser = OutputParser::new(cli_type_for_parser);
            let mut buffer = [0u8; 4096];
            let mut conversation_id_found = false;
            // Track if we've already auto-accepted trust prompt to prevent duplicate sends
            let mut trust_prompt_accepted = false;

            // Helper function to detect trust prompts (should auto-accept)
            // vs tool approval prompts (should show modal to user)
            fn is_trust_prompt(content: &str) -> bool {
                let lower = content.to_lowercase();
                // Trust prompts - auto-accept these
                let trust_patterns = [
                    "do you trust the files",
                    "execution allowed by",
                ];
                // Tool approval patterns - do NOT auto-accept these
                let tool_approval_patterns = [
                    "do you want to proceed",
                    "do you want to continue",
                    "allow this",
                    "1. yes",
                    "2. yes, and",
                    "1 for yes",
                    "2 for yes always",
                    "allow once",
                    "allow always",
                    "deny",
                ];

                // Check if it's a tool approval (should NOT auto-accept)
                for pattern in tool_approval_patterns {
                    if lower.contains(pattern) {
                        return false;
                    }
                }

                // Check if it's a trust prompt (should auto-accept)
                for pattern in trust_patterns {
                    if lower.contains(pattern) {
                        return true;
                    }
                }

                false
            }

            loop {
                // Check for user input signals (non-blocking)
                while let Ok(()) = user_input_rx.try_recv() {
                    tracing::debug!("Parser notified of user input for session {}", session_id_clone);
                    parser.user_sent_input();
                }

                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let output = String::from_utf8_lossy(&buffer[..n]);
                        let cleaned = parser.process(&output);

                        // Try to extract conversation ID from output
                        if !conversation_id_found {
                            if let Some(conv_id) = parser.extract_conversation_id(&output) {
                                conversation_id_found = true;
                                tracing::info!("Found conversation ID for session {}: {}", session_id_clone, conv_id);

                                // Update database with conversation ID
                                let _ = db.update_conversation_id(&session_id_clone, &conv_id);

                                // Emit event to frontend
                                let _ = app.emit(
                                    "conversation-id",
                                    serde_json::json!({
                                        "sessionId": session_id_clone,
                                        "conversationId": conv_id,
                                    }),
                                );
                            }
                        }

                        // Get recent context BEFORE check_waiting_for_input, as the check may clear the buffer
                        let recent_context = parser.get_recent_context(2000);

                        // INDEPENDENT TRUST PROMPT CHECK: Run on every chunk regardless of debounce
                        // This ensures trust prompts are caught immediately when they appear
                        if !trust_prompt_accepted && is_trust_prompt(&cleaned) {
                            tracing::info!("Session {} detected trust prompt in current chunk - auto-accepting immediately", session_id_clone);
                            if let Ok(mut w) = writer_for_reader.lock() {
                                if let Err(e) = w.write_all(b"\r") {
                                    tracing::error!("Failed to auto-accept trust prompt (immediate): {}", e);
                                } else if let Err(e) = w.flush() {
                                    tracing::error!("Failed to flush auto-accept (immediate): {}", e);
                                } else {
                                    tracing::info!("Successfully auto-accepted trust prompt (immediate) for session {}", session_id_clone);
                                    parser.user_sent_input();
                                    trust_prompt_accepted = true;
                                }
                            }
                        }

                        // Check if Claude is waiting for input (use cleaned output for better pattern matching)
                        if parser.check_waiting_for_input(&cleaned) {
                            tracing::debug!("Session {} is waiting for input", session_id_clone);
                            // Include the recent accumulated output as prompt content so mobile can detect
                            // whether this is a tool approval prompt or general waiting
                            // Use the context captured before the check (buffer may be cleared during check)
                            // Fall back to current chunk if context is empty
                            let prompt_content = if recent_context.is_empty() {
                                cleaned.clone()
                            } else {
                                recent_context.clone()
                            };

                            // AUTO-ACCEPT TRUST PROMPTS: Check if this is a trust prompt
                            // and auto-accept it by sending Enter key
                            let mut trust_prompt_handled = false;
                            if is_trust_prompt(&prompt_content) {
                                tracing::info!("Session {} has trust prompt - auto-accepting", session_id_clone);
                                // Send Enter key to auto-accept
                                if let Ok(mut w) = writer_for_reader.lock() {
                                    if let Err(e) = w.write_all(b"\r") {
                                        tracing::error!("Failed to auto-accept trust prompt: {}", e);
                                    } else if let Err(e) = w.flush() {
                                        tracing::error!("Failed to flush auto-accept: {}", e);
                                    } else {
                                        tracing::info!("Successfully auto-accepted trust prompt for session {}", session_id_clone);
                                        // Reset parser state since we sent input
                                        parser.user_sent_input();
                                        // Mark as handled so we skip waiting-for-input emit but NOT pty-output
                                        trust_prompt_handled = true;
                                    }
                                }
                            }

                            // For non-trust prompts (tool approvals, etc), emit the event
                            // so mobile can show the appropriate UI
                            // Skip this emit if we just auto-accepted a trust prompt
                            if !trust_prompt_handled {
                                let _ = app.emit(
                                    "waiting-for-input",
                                    serde_json::json!({
                                        "sessionId": session_id_clone,
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                        "promptContent": prompt_content,
                                    }),
                                );
                            }
                        }

                        // Emit raw PTY output to frontend (for desktop terminal display and streaming)
                        let _ = app.emit(
                            "pty-output",
                            serde_json::json!({
                                "sessionId": session_id_clone,
                                "output": cleaned,
                                "raw": output,
                            }),
                        );

                        // THINKING/PROGRESS DETECTION: Extract dynamic status messages for mobile
                        // Claude shows status like "Building core pages...", "Discussing monetization..."
                        // in orange text while working. We detect these and emit as activities.
                        detect_and_emit_thinking(&cleaned, &session_id_clone, &app);

                        // JSONL REDESIGN: For Claude sessions, the JSONL watcher handles
                        // activity parsing, message extraction, and storage.
                        // PTY is now only used for:
                        // - Running the process
                        // - Sending input
                        // - Tool approval detection (handled above)
                        // - Streaming raw output for visibility
                        //
                        // We no longer call parse_activities() or extract_message() here
                        // since the JSONL watcher emits clean, structured activities
                        // from Claude's authoritative conversation log.
                    }
                    Err(e) => {
                        tracing::error!("PTY read error: {}", e);
                        break;
                    }
                }
            }

            // Wait for process to exit
            let _ = child.wait();
            tracing::info!("Session {} ended", session_id_clone);
        });

        // Create JSONL watcher for Claude sessions
        let jsonl_watcher = if cli_type == CliType::ClaudeCode {
            match JsonlWatcher::new(
                session_id.clone(),
                project_path_for_watcher,
                conversation_id_for_watcher,
                app_for_watcher,
            ) {
                Ok(watcher) => {
                    tracing::info!("Created JSONL watcher for session {}", session_id);
                    Some(watcher)
                }
                Err(e) => {
                    tracing::warn!("Failed to create JSONL watcher for session {}: {}", session_id, e);
                    None
                }
            }
        } else {
            None
        };

        // Store session
        self.sessions.insert(
            session_id,
            PtySession {
                writer,
                master,
                _reader_task: reader_task,
                _kill_tx: kill_tx,
                user_input_tx,
                jsonl_watcher,
            },
        );

        Ok(())
    }

    /// Resize the PTY terminal
    pub fn resize(&self, session_id: &str, rows: u16, cols: u16) -> Result<(), PtyError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        let master = session.master.lock().map_err(|_| PtyError::Lock)?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        tracing::debug!("Resized PTY {} to {}x{}", session_id, cols, rows);
        Ok(())
    }

    pub async fn send_input(&self, session_id: &str, input: &str) -> Result<(), PtyError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        // Very detailed logging to debug input issues
        let input_bytes: Vec<u8> = input.bytes().collect();
        tracing::info!(
            "PTY send_input START: session={}, input_str={:?}, input_len={}, input_hex={:02x?}",
            session_id, input, input.len(), input_bytes
        );

        // Signal the parser that user input was sent
        let _ = session.user_input_tx.try_send(());

        // For mobile chat input, we need to send the text followed by Enter (CR).
        // Key insight: Claude Code uses crossterm which handles terminal input.
        // We'll send the entire input string at once, then CR.
        // This is similar to how pasting works in a terminal.
        let writer = session.writer.clone();
        let input_owned = input.to_string();
        let session_id_owned = session_id.to_string();

        // Use spawn_blocking to ensure we don't block the async runtime
        tokio::task::spawn_blocking(move || {
            let mut w = match writer.lock() {
                Ok(w) => w,
                Err(_) => {
                    tracing::error!("PTY send_input: failed to acquire writer lock");
                    return;
                }
            };

            // Write the entire input string at once
            if let Err(e) = w.write_all(input_owned.as_bytes()) {
                tracing::error!("PTY send_input: write error: {}", e);
                return;
            }
            if let Err(e) = w.flush() {
                tracing::error!("PTY send_input: flush error after text: {}", e);
                return;
            }
            tracing::info!("PTY send_input: wrote {} text bytes", input_owned.len());

            // Small delay to let the terminal process the input
            std::thread::sleep(std::time::Duration::from_millis(5));

            // Write CR (carriage return) - this is the Enter key
            // This tells the terminal to submit the line
            if let Err(e) = w.write_all(b"\r") {
                tracing::error!("PTY send_input: write CR error: {}", e);
                return;
            }
            if let Err(e) = w.flush() {
                tracing::error!("PTY send_input: flush error after CR: {}", e);
                return;
            }
            tracing::info!("PTY send_input: wrote CR and flushed for session {}", session_id_owned);
        })
        .await
        .map_err(|e| PtyError::Pty(format!("spawn_blocking failed: {}", e)))?;

        tracing::info!("PTY send_input COMPLETE: session={}", session_id);

        Ok(())
    }

    /// Send raw input without adding newline (for terminal emulator use)
    /// If input is empty, sends just Enter key (CR) - used for auto-accepting trust prompts
    pub async fn send_raw_input(&self, session_id: &str, input: &str) -> Result<(), PtyError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        // Signal the parser that user input was sent (for state reset)
        let _ = session.user_input_tx.try_send(());

        let mut writer = session.writer.lock().map_err(|_| PtyError::Lock)?;

        // If input is empty, send Enter key (CR) - used for auto-accept trust prompts
        if input.is_empty() {
            tracing::info!("Sending Enter key (CR) to session {} for auto-accept", session_id);
            writer.write_all(b"\r")?;
        } else {
            tracing::debug!("Sending raw input to session {}: {:?}", session_id, input);
            writer.write_all(input.as_bytes())?;
        }
        writer.flush()?;

        Ok(())
    }

    pub async fn stop_session(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.remove(session_id) {
            // Send kill signal
            let _ = session._kill_tx.send(()).await;
            // Task will clean up on its own
            tracing::info!("Stopped session {}", session_id);
        }
    }

    pub fn get_active_sessions(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    /// Check if a session is active (has a running PTY)
    pub fn is_session_active(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Resume a session with an existing conversation ID
    pub async fn resume_session(
        &mut self,
        session_id: String,
        project_path: String,
        conversation_id: String,
        cli_type: CliType,
        _db: Arc<Database>, // Unused after JSONL redesign - JSONL watcher handles storage
        app: AppHandle,
    ) -> Result<(), PtyError> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/bigphoot".to_string());

        // Build resume command based on CLI type
        let cmd = match cli_type {
            CliType::ClaudeCode => {
                let claude_path = format!("{}/.local/bin/claude", home);
                let mut cmd = CommandBuilder::new(&claude_path);
                cmd.arg("--resume");
                cmd.arg(&conversation_id);
                cmd.cwd(&project_path);
                cmd
            }
            CliType::GeminiCli => {
                // Gemini uses --resume with session index or "latest"
                let mut cmd = CommandBuilder::new("gemini");
                cmd.arg("--resume");
                cmd.arg(&conversation_id); // This should be an index like "1" or "latest"
                cmd.cwd(&project_path);
                cmd
            }
        };

        let mut cmd = cmd;
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.env("HOME", &home);
        cmd.env("TERM", "xterm-256color");
        if let Ok(shell) = std::env::var("SHELL") {
            cmd.env("SHELL", shell);
        }

        tracing::info!(
            "Resuming {} session {} with conversation {} in {}",
            cli_type.display_name(),
            session_id,
            conversation_id,
            project_path
        );

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        let writer = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .map_err(|e| PtyError::Pty(e.to_string()))?,
        ));

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        let master = Arc::new(Mutex::new(pair.master));
        let (kill_tx, _kill_rx) = mpsc::channel::<()>(1);

        // Channel for signaling user input to the parser
        let (user_input_tx, mut user_input_rx) = mpsc::channel::<()>(16);

        // Clone writer for reader task to use for auto-accept
        let writer_for_reader = writer.clone();

        // Clone app for JSONL watcher (before it's moved into the reader task)
        let app_for_watcher = app.clone();
        let project_path_for_watcher = project_path.clone();
        let conversation_id_for_watcher = conversation_id.clone();

        let session_id_clone = session_id.clone();
        let cli_type_for_parser = cli_type; // Copy for the spawned task
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut parser = OutputParser::new(cli_type_for_parser);
            let mut buffer = [0u8; 4096];
            // Track if we've already auto-accepted trust prompt to prevent duplicate sends
            let mut trust_prompt_accepted = false;

            // Helper function to detect trust prompts (should auto-accept)
            // vs tool approval prompts (should show modal to user)
            fn is_trust_prompt(content: &str) -> bool {
                let lower = content.to_lowercase();
                // Trust prompts - auto-accept these
                let trust_patterns = [
                    "do you trust the files",
                    "execution allowed by",
                ];
                // Tool approval patterns - do NOT auto-accept these
                let tool_approval_patterns = [
                    "do you want to proceed",
                    "do you want to continue",
                    "allow this",
                    "1. yes",
                    "2. yes, and",
                    "1 for yes",
                    "2 for yes always",
                    "allow once",
                    "allow always",
                    "deny",
                ];

                // Check if it's a tool approval (should NOT auto-accept)
                for pattern in tool_approval_patterns {
                    if lower.contains(pattern) {
                        return false;
                    }
                }

                // Check if it's a trust prompt (should auto-accept)
                for pattern in trust_patterns {
                    if lower.contains(pattern) {
                        return true;
                    }
                }

                false
            }

            loop {
                // Check for user input signals (non-blocking)
                while let Ok(()) = user_input_rx.try_recv() {
                    tracing::debug!("Parser notified of user input for resumed session {}", session_id_clone);
                    parser.user_sent_input();
                }

                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let output = String::from_utf8_lossy(&buffer[..n]);
                        let cleaned = parser.process(&output);

                        // Get recent context BEFORE check_waiting_for_input, as the check may clear the buffer
                        let recent_context = parser.get_recent_context(2000);

                        // INDEPENDENT TRUST PROMPT CHECK: Run on every chunk regardless of debounce
                        // This ensures trust prompts are caught immediately when they appear
                        if !trust_prompt_accepted && is_trust_prompt(&cleaned) {
                            tracing::info!("Resumed session {} detected trust prompt in current chunk - auto-accepting immediately", session_id_clone);
                            if let Ok(mut w) = writer_for_reader.lock() {
                                if let Err(e) = w.write_all(b"\r") {
                                    tracing::error!("Failed to auto-accept trust prompt (immediate): {}", e);
                                } else if let Err(e) = w.flush() {
                                    tracing::error!("Failed to flush auto-accept (immediate): {}", e);
                                } else {
                                    tracing::info!("Successfully auto-accepted trust prompt (immediate) for resumed session {}", session_id_clone);
                                    parser.user_sent_input();
                                    trust_prompt_accepted = true;
                                }
                            }
                        }

                        // Check if Claude is waiting for input (use cleaned output for better pattern matching)
                        if parser.check_waiting_for_input(&cleaned) {
                            tracing::debug!("Resumed session {} is waiting for input", session_id_clone);
                            // Include the recent accumulated output as prompt content so mobile can detect
                            // whether this is a tool approval prompt or general waiting
                            // Use the context captured before the check (buffer may be cleared during check)
                            // Fall back to current chunk if context is empty
                            let prompt_content = if recent_context.is_empty() {
                                cleaned.clone()
                            } else {
                                recent_context.clone()
                            };

                            // AUTO-ACCEPT TRUST PROMPTS: Check if this is a trust prompt
                            // and auto-accept it by sending Enter key
                            if is_trust_prompt(&prompt_content) {
                                tracing::info!("Resumed session {} has trust prompt - auto-accepting", session_id_clone);
                                // Send Enter key to auto-accept
                                if let Ok(mut w) = writer_for_reader.lock() {
                                    if let Err(e) = w.write_all(b"\r") {
                                        tracing::error!("Failed to auto-accept trust prompt: {}", e);
                                    } else if let Err(e) = w.flush() {
                                        tracing::error!("Failed to flush auto-accept: {}", e);
                                    } else {
                                        tracing::info!("Successfully auto-accepted trust prompt for resumed session {}", session_id_clone);
                                        // Reset parser state since we sent input
                                        parser.user_sent_input();
                                        // Don't emit waiting-for-input event since we handled it
                                        continue;
                                    }
                                }
                            }

                            // For non-trust prompts (tool approvals, etc), emit the event
                            let _ = app.emit(
                                "waiting-for-input",
                                serde_json::json!({
                                    "sessionId": session_id_clone,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                    "promptContent": prompt_content,
                                }),
                            );
                        }

                        // Emit raw PTY output to frontend (for desktop terminal display and streaming)
                        let _ = app.emit(
                            "pty-output",
                            serde_json::json!({
                                "sessionId": session_id_clone,
                                "output": cleaned,
                                "raw": output,
                            }),
                        );

                        // THINKING/PROGRESS DETECTION: Extract dynamic status messages for mobile
                        // Claude shows status like "Building core pages...", "Discussing monetization..."
                        // in orange text while working. We detect these and emit as activities.
                        detect_and_emit_thinking(&cleaned, &session_id_clone, &app);

                        // JSONL REDESIGN: For Claude sessions, the JSONL watcher handles
                        // activity parsing, message extraction, and storage.
                        // PTY is now only used for:
                        // - Running the process
                        // - Sending input
                        // - Tool approval detection (handled above)
                        // - Streaming raw output for visibility
                    }
                    Err(e) => {
                        tracing::error!("PTY read error: {}", e);
                        break;
                    }
                }
            }

            let _ = child.wait();
            tracing::info!("Resumed session {} ended", session_id_clone);
        });

        // Create JSONL watcher for Claude sessions
        let jsonl_watcher = if cli_type == CliType::ClaudeCode {
            match JsonlWatcher::new(
                session_id.clone(),
                project_path_for_watcher,
                conversation_id_for_watcher,
                app_for_watcher,
            ) {
                Ok(watcher) => {
                    tracing::info!("Created JSONL watcher for resumed session {}", session_id);
                    Some(watcher)
                }
                Err(e) => {
                    tracing::warn!("Failed to create JSONL watcher for resumed session {}: {}", session_id, e);
                    None
                }
            }
        } else {
            None
        };

        self.sessions.insert(
            session_id,
            PtySession {
                writer,
                master,
                _reader_task: reader_task,
                _kill_tx: kill_tx,
                user_input_tx,
                jsonl_watcher,
            },
        );

        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
