// PTY module - Manages AI CLI processes in pseudo-terminals

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use crate::codex;
use crate::codex_watcher::CodexWatcher;
use crate::config;
use crate::db::{CliType, Database};
use crate::gemini;
use crate::gemini_watcher::GeminiWatcher;
use crate::jsonl_watcher::JsonlWatcher;
use crate::opencode_watcher::{self, OpenCodeWatcher};
use crate::parser::OutputParser;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Unified CLI watcher enum - handles different file formats for each CLI
/// Note: The watcher variants hold watchers that are kept alive for their side effects
/// (background threads watching files), not for direct access to their fields.
#[allow(dead_code)]
enum CliWatcher {
    /// Claude: JSONL at ~/.claude/projects/{hash}/{session}.jsonl
    Claude(JsonlWatcher),
    /// Codex: JSONL at ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl
    Codex(CodexWatcher),
    /// Gemini: JSON at ~/.gemini/tmp/{hash}/chats/session-*.json
    Gemini(GeminiWatcher),
    /// OpenCode: Distributed JSON at ~/.local/share/opencode/storage/
    OpenCode(OpenCodeWatcher),
}

impl CliWatcher {
    fn stop(&self) {
        match self {
            CliWatcher::Claude(w) => w.stop(),
            CliWatcher::Codex(w) => w.stop(),
            CliWatcher::Gemini(w) => w.stop(),
            CliWatcher::OpenCode(w) => w.stop(),
        }
    }
}

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

/// Size of PTY output history buffer in bytes (32KB)
/// This allows mobile clients to receive recent output when subscribing to an existing session
const OUTPUT_HISTORY_SIZE: usize = 32 * 1024;

fn resolve_home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| {
            let drive = std::env::var("HOMEDRIVE").ok();
            let path = std::env::var("HOMEPATH").ok();
            match (drive, path) {
                (Some(drive), Some(path)) => Ok(format!("{}{}", drive, path)),
                _ => Err(std::env::VarError::NotPresent),
            }
        })
        .unwrap_or_else(|_| ".".to_string())
}

fn expand_tilde(path: &str, home: &str) -> String {
    if home.is_empty() {
        return path.to_string();
    }
    if path == "~" {
        return home.to_string();
    }
    if path.starts_with("~/") || path.starts_with("~\\") {
        let trimmed = path[2..].trim_start_matches(['/', '\\']);
        let mut expanded = home.to_string();
        if !expanded.ends_with(std::path::MAIN_SEPARATOR) {
            expanded.push(std::path::MAIN_SEPARATOR);
        }
        expanded.push_str(trimmed);
        return expanded;
    }
    path.to_string()
}

fn resolve_project_dir(raw_path: &str, home: &str) -> Result<PathBuf, PtyError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(PtyError::Pty("Project path is empty".to_string()));
    }

    let expanded = expand_tilde(trimmed, home);
    let mut path = PathBuf::from(expanded);
    if !path.is_absolute() {
        let cwd = std::env::current_dir().map_err(|e| {
            PtyError::Pty(format!("Failed to resolve current directory: {}", e))
        })?;
        path = cwd.join(path);
    }

    if !path.exists() {
        fs::create_dir_all(&path).map_err(|e| {
            PtyError::Pty(format!(
                "Failed to create project directory {}: {}",
                path.display(),
                e
            ))
        })?;
    }

    if !path.is_dir() {
        return Err(PtyError::Pty(format!(
            "Project path is not a directory: {}",
            path.display()
        )));
    }

    Ok(path)
}

fn resolve_cli_binary(cli_type: CliType, home: &str) -> String {
    let home_path = Path::new(home);

    // Helper to find first existing path from candidates
    let find_binary = |candidates: &[PathBuf], fallback: &str| -> String {
        for candidate in candidates {
            if candidate.exists() && candidate.is_file() {
                tracing::debug!("Resolved {} to: {}", fallback, candidate.display());
                return candidate.to_string_lossy().to_string();
            }
        }
        // Fall back to command name (relies on PATH in shell)
        tracing::debug!("Using PATH fallback for: {}", fallback);
        fallback.to_string()
    };

    match cli_type {
        CliType::ClaudeCode => {
            let candidates = vec![
                // Check common installation paths in order of preference
                home_path.join(".local").join("bin").join("claude"),
                home_path.join(".npm-global").join("bin").join("claude"),
                home_path.join(".yarn").join("bin").join("claude"),
                home_path.join(".bun").join("bin").join("claude"),
                PathBuf::from("/usr/local/bin/claude"),
                PathBuf::from("/usr/bin/claude"),
                PathBuf::from("/opt/homebrew/bin/claude"),
            ];
            find_binary(&candidates, "claude")
        }
        CliType::OpenCode => {
            let candidates = vec![
                home_path.join(".opencode").join("bin").join("opencode"),
                home_path.join(".local").join("bin").join("opencode"),
                PathBuf::from("/usr/local/bin/opencode"),
                PathBuf::from("/usr/bin/opencode"),
            ];
            find_binary(&candidates, "opencode")
        }
        CliType::GeminiCli => {
            let candidates = vec![
                home_path.join(".local").join("bin").join("gemini"),
                PathBuf::from("/usr/local/bin/gemini"),
                PathBuf::from("/usr/bin/gemini"),
            ];
            find_binary(&candidates, "gemini")
        }
        CliType::Codex => {
            let candidates = vec![
                home_path.join(".local").join("bin").join("codex"),
                home_path.join(".npm-global").join("bin").join("codex"),
                PathBuf::from("/usr/local/bin/codex"),
                PathBuf::from("/usr/bin/codex"),
            ];
            find_binary(&candidates, "codex")
        }
    }
}

struct CliCommand {
    program: String,
    args: Vec<String>,
}

fn build_cli_command_for_start(
    cli_type: CliType,
    project_path: &str,
    conversation_id: &str,
    skip_permissions: bool,
    codex_policy_flag: &str,
    home: &str,
) -> CliCommand {
    match cli_type {
        CliType::ClaudeCode => {
            let mut args = vec!["--session-id".to_string(), conversation_id.to_string()];
            if skip_permissions {
                args.push("--dangerously-skip-permissions".to_string());
            }
            CliCommand {
                program: resolve_cli_binary(cli_type, home),
                args,
            }
        }
        CliType::GeminiCli => CliCommand {
            program: resolve_cli_binary(cli_type, home),
            args: Vec::new(),
        },
        CliType::OpenCode => CliCommand {
            program: resolve_cli_binary(cli_type, home),
            args: vec![project_path.to_string()],
        },
        CliType::Codex => CliCommand {
            program: resolve_cli_binary(cli_type, home),
            args: vec![
                "-C".to_string(),
                project_path.to_string(),
                "-a".to_string(),
                codex_policy_flag.to_string(),
            ],
        },
    }
}

fn build_cli_command_for_resume(
    cli_type: CliType,
    project_path: &str,
    conversation_id: &str,
    skip_permissions: bool,
    codex_policy_flag: &str,
    home: &str,
) -> CliCommand {
    match cli_type {
        CliType::ClaudeCode => {
            let mut args = vec!["--resume".to_string(), conversation_id.to_string()];
            if skip_permissions {
                args.push("--dangerously-skip-permissions".to_string());
            }
            CliCommand {
                program: resolve_cli_binary(cli_type, home),
                args,
            }
        }
        CliType::GeminiCli => CliCommand {
            program: resolve_cli_binary(cli_type, home),
            args: vec!["--resume".to_string(), conversation_id.to_string()],
        },
        CliType::OpenCode => CliCommand {
            program: resolve_cli_binary(cli_type, home),
            args: vec!["-c".to_string(), project_path.to_string()],
        },
        CliType::Codex => CliCommand {
            program: resolve_cli_binary(cli_type, home),
            args: vec![
                "resume".to_string(),
                conversation_id.to_string(),
                "-C".to_string(),
                project_path.to_string(),
                "-a".to_string(),
                codex_policy_flag.to_string(),
            ],
        },
    }
}

#[cfg(not(windows))]
fn shell_escape(value: &str) -> String {
    let mut out = String::from("'");
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(not(windows))]
fn build_shell_command(cli_cmd: &CliCommand) -> String {
    let mut parts = Vec::with_capacity(cli_cmd.args.len() + 1);
    parts.push(shell_escape(&cli_cmd.program));
    for arg in &cli_cmd.args {
        parts.push(shell_escape(arg));
    }
    let command = parts.join(" ");
    format!("stty -echo; exec {}", command)
}

#[cfg(not(windows))]
fn resolve_shell_path() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if Path::new(&shell).exists() {
            return shell;
        }
    }

    for candidate in ["/bin/bash", "/bin/zsh", "/usr/bin/bash", "/usr/bin/zsh", "/bin/sh"] {
        if Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }

    "/bin/sh".to_string()
}

fn find_nvm_dir(home: &str) -> Option<String> {
    if let Ok(nvm_dir) = std::env::var("NVM_DIR") {
        return Some(nvm_dir);
    }

    let candidate = Path::new(home).join(".nvm");
    if candidate.exists() {
        Some(candidate.to_string_lossy().to_string())
    } else {
        None
    }
}

fn append_nvm_path(path_parts: &mut Vec<String>, nvm_dir: &str) {
    let nvm_current = Path::new(nvm_dir).join("versions").join("node");
    if !nvm_current.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&nvm_current) {
        if let Some(Ok(entry)) = entries.into_iter().next() {
            path_parts.push(entry.path().join("bin").to_string_lossy().to_string());
        }
    }
}

fn configure_command_env(cmd: &mut CommandBuilder, home: &str) {
    let mut path_parts: Vec<String> = Vec::new();

    if cfg!(windows) {
        if !home.is_empty() {
            path_parts.push(
                Path::new(home)
                    .join("AppData")
                    .join("Roaming")
                    .join("npm")
                    .to_string_lossy()
                    .to_string(),
            );
            path_parts.push(
                Path::new(home)
                    .join(".npm-global")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            path_parts.push(
                Path::new(home)
                    .join(".yarn")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            path_parts.push(
                Path::new(home)
                    .join(".bun")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            path_parts.push(
                Path::new(home)
                    .join("scoop")
                    .join("shims")
                    .to_string_lossy()
                    .to_string(),
            );
        }
    } else {
        if !home.is_empty() {
            path_parts.push(Path::new(home).join(".local").join("bin").to_string_lossy().to_string());
            path_parts.push(Path::new(home).join(".npm-global").join("bin").to_string_lossy().to_string());
            path_parts.push(Path::new(home).join("node_modules").join(".bin").to_string_lossy().to_string());
            path_parts.push(Path::new(home).join(".yarn").join("bin").to_string_lossy().to_string());
            path_parts.push(Path::new(home).join(".bun").join("bin").to_string_lossy().to_string());
            path_parts.push(Path::new(home).join(".local").join("share").join("pnpm").to_string_lossy().to_string());
        }
        if let Some(nvm_dir) = find_nvm_dir(home) {
            append_nvm_path(&mut path_parts, &nvm_dir);
            cmd.env("NVM_DIR", nvm_dir);
        }
        path_parts.push("/usr/local/bin".to_string());
        path_parts.push("/usr/bin".to_string());
        path_parts.push("/bin".to_string());
        path_parts.push("/opt/homebrew/bin".to_string());
        path_parts.push("/opt/homebrew/sbin".to_string());
    }

    if let Ok(existing_path) = std::env::var("PATH") {
        path_parts.push(existing_path);
    }

    let separator = if cfg!(windows) { ";" } else { ":" };
    if !path_parts.is_empty() {
        cmd.env("PATH", path_parts.join(separator));
    }

    if !home.is_empty() {
        cmd.env("HOME", home);
        if cfg!(windows) {
            cmd.env("USERPROFILE", home);
        }
    }

    if !cfg!(windows) {
        cmd.env("TERM", "xterm-256color");
        if let Ok(shell) = std::env::var("SHELL") {
            cmd.env("SHELL", shell);
        }
    }
}

fn build_command_builder(
    cli_cmd: &CliCommand,
    project_dir: &Path,
    home: &str,
) -> CommandBuilder {
    #[cfg(windows)]
    {
        let mut cmd = CommandBuilder::new(&cli_cmd.program);
        for arg in &cli_cmd.args {
            cmd.arg(arg);
        }
        cmd.cwd(project_dir);
        configure_command_env(&mut cmd, home);
        cmd
    }

    #[cfg(not(windows))]
    {
        let mut cmd = CommandBuilder::new(resolve_shell_path());
        cmd.arg("-l");
        cmd.arg("-c");
        cmd.arg(&build_shell_command(cli_cmd));
        cmd.cwd(project_dir);
        configure_command_env(&mut cmd, home);
        cmd
    }
}

fn update_session_conversation_id(
    db: &Arc<Database>,
    app: &AppHandle,
    session_id: &str,
    conversation_id: &str,
) {
    if conversation_id.trim().is_empty() {
        return;
    }
    if db.update_conversation_id(session_id, conversation_id).is_ok() {
        let _ = app.emit(
            "conversation-id",
            serde_json::json!({
                "sessionId": session_id,
                "conversationId": conversation_id,
            }),
        );
    }
}

fn maybe_write_pty_snapshot(
    capture_dir: &Option<String>,
    cli_type: CliType,
    session_id: &str,
    prompt_content: &str,
    output_history: &Arc<Mutex<VecDeque<u8>>>,
) {
    let dir = match capture_dir {
        Some(dir) if !dir.is_empty() => dir,
        _ => return,
    };

    if !matches!(cli_type, CliType::ClaudeCode | CliType::Codex) {
        return;
    }

    let bytes: Vec<u8> = match output_history.lock() {
        Ok(history) => history.iter().copied().collect(),
        Err(_) => return,
    };

    let prompt_excerpt: String = prompt_content.chars().take(400).collect();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let filename = format!(
        "{}_{}_{}.json",
        session_id,
        cli_type.as_str(),
        timestamp.replace(':', "-")
    );

    let mut path = PathBuf::from(dir);
    path.push(filename);

    let payload = serde_json::json!({
        "session_id": session_id,
        "cli_type": cli_type.as_str(),
        "timestamp": timestamp,
        "prompt_excerpt": prompt_excerpt,
        "pty_base64": BASE64.encode(&bytes),
    });

    if fs::create_dir_all(dir).is_ok() {
        let _ = fs::write(path, payload.to_string());
    }
}

struct PtySession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    _reader_task: JoinHandle<()>,
    _kill_tx: mpsc::Sender<()>,
    /// Channel to signal user input was sent (for parser tracking)
    user_input_tx: mpsc::Sender<()>,
    /// File watcher for CLI sessions (type depends on CLI)
    /// - Claude: JSONL watcher for ~/.claude/projects/...
    /// - Codex: JSONL watcher for ~/.codex/sessions/...
    /// - Gemini: JSON watcher for ~/.gemini/tmp/...
    /// Kept alive for its side effects (background thread watching for file changes)
    #[allow(dead_code)]
    cli_watcher: Option<CliWatcher>,
    /// Ring buffer of recent PTY output for session history replay
    /// New subscribers receive this history to see terminal state
    output_history: Arc<Mutex<VecDeque<u8>>>,
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
        "Ideating",
        "Fermenting",
        "Kneading",
        "Pollinating",
        "Fluttering",
        "Brewing",
        "Crafting",
        "Weaving",
        "Spinning",
        "Stewing",
        "Marinating",
        "Simmering",
        "Steeping",
        "Jitterbugging",
        "Pondering",
        "Contemplating",
        "Musing",
        "Philosophising",
        "Ruminating",
        "Deliberating",
        "Cogitating",
        "Dilly-dallying",
        "Levitating",
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

        // CRITICAL: Skip hook output - these should NOT be classified as thinking
        // Hook patterns include: "Running hooks...", "hook success", "PostToolUse:", etc.
        let lower_trimmed = trimmed.to_lowercase();
        let is_hook_output = lower_trimmed.contains("hook")
            || lower_trimmed.contains("posttooluse")
            || lower_trimmed.contains("pretooluse")
            || lower_trimmed.contains("sessionstart")
            || lower_trimmed.contains("sessionstop")
            || lower_trimmed.contains("ran ")  // "Ran 3/6 hooks"
            || (lower_trimmed.contains('/') && lower_trimmed.chars().filter(|c| c.is_ascii_digit()).count() >= 2)  // "2/6" pattern
            || lower_trimmed.contains("success")
            || lower_trimmed.contains("failed:");

        if is_hook_output {
            continue;
        }

        // Strip spinner characters from the beginning for detection
        let mut content_to_check = trimmed;
        let mut has_spinner_prefix = false;
        for c in SPINNER_CHARS {
            if let Some(rest) = trimmed.strip_prefix(*c) {
                content_to_check = rest.trim_start();
                has_spinner_prefix = true;
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
        // TIGHTENED: Only trigger if line has spinner prefix - prevents false positives
        // like "Running stop hooks... 2/6" which don't have spinners
        if !is_thinking && has_spinner_prefix && content_to_check.ends_with("...") && content_to_check.len() < 100 {
            // Filter out lines that are actual content (have response markers)
            // Progress messages are typically clean status text
            let has_special_chars = content_to_check
                .chars()
                .any(|c| matches!(c, '●' | '⎿' | '│' | '├' | '└' | '┌' | '┐' | '┘' | '┴' | '┬'));

            if !has_special_chars {
                is_thinking = true;
                thinking_content = content_to_check.to_string();
            }
        }

        // Also check for "thinking", "thought for X" patterns
        if !is_thinking {
            let lower = content_to_check.to_lowercase();
            if lower.contains("thinking")
                || lower.contains("thought for")
                || lower.contains("esc to interrupt")
            {
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
            // Also handle malformed content like "thinking)" where opening paren is missing
            let clean_content = if let Some(paren_pos) = thinking_content.find('(') {
                thinking_content[..paren_pos].trim().to_string()
            } else {
                // Strip trailing ) if present (handles "thinking)" from malformed content)
                thinking_content.trim_end_matches(')').trim().to_string()
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

    /// Optional settings that can be passed from mobile to override config
    pub async fn start_session(
        &mut self,
        session_id: String,
        project_path: String,
        cli_type: CliType,
        db: Arc<Database>,
        app: AppHandle,
    ) -> Result<(), PtyError> {
        // Default to config settings when not provided
        self.start_session_with_settings(session_id, project_path, cli_type, db, app, None, None)
            .await
    }

    /// Start a session with optional mobile-provided settings
    pub async fn start_session_with_settings(
        &mut self,
        session_id: String,
        project_path: String,
        cli_type: CliType,
        db: Arc<Database>,
        app: AppHandle,
        claude_skip_permissions: Option<bool>,
        codex_approval_policy: Option<String>,
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

        // Resolve home directory for cross-platform compatibility
        let home = resolve_home_dir();
        let project_dir = resolve_project_dir(&project_path, &home)?;
        let project_path = project_dir.to_string_lossy().to_string();

        // Load config for fallback settings
        let app_config = config::load_config(&app).unwrap_or_default();

        // Use passed settings if provided, otherwise fall back to config
        let use_skip_permissions =
            claude_skip_permissions.unwrap_or(app_config.claude_skip_permissions);
        let use_codex_policy = codex_approval_policy
            .as_deref()
            .and_then(config::CodexApprovalPolicy::from_str)
            .unwrap_or(app_config.codex_approval_policy);

        // Generate a conversation ID for Claude Code sessions
        let conversation_id = uuid::Uuid::new_v4().to_string();

        if matches!(cli_type, CliType::Codex) {
            tracing::info!(
                "Codex session starting with approval policy: {}",
                use_codex_policy.as_flag()
            );
        }
        if use_skip_permissions && matches!(cli_type, CliType::ClaudeCode) {
            tracing::info!("Claude session starting with --dangerously-skip-permissions");
        }

        let cli_cmd = build_cli_command_for_start(
            cli_type,
            &project_path,
            &conversation_id,
            use_skip_permissions,
            use_codex_policy.as_flag(),
            &home,
        );
        let cmd = build_command_builder(&cli_cmd, &project_dir, &home);

        tracing::info!("Starting {} in {}", cli_type.display_name(), project_path);

        // Store conversation ID only when we explicitly control it (Claude).
        if matches!(cli_type, CliType::ClaudeCode) {
            let _ = db.update_conversation_id(&session_id, &conversation_id);
            tracing::info!(
                "Set conversation ID for session {}: {}",
                session_id,
                conversation_id
            );
            let _ = app.emit(
                "conversation-id",
                serde_json::json!({
                    "sessionId": session_id,
                    "conversationId": conversation_id,
                }),
            );
        }

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
                            let delay =
                                std::time::Duration::from_millis(100 * (attempt as u64 + 1));
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

        // Ring buffer for PTY output history - allows new subscribers to see recent terminal output
        let output_history: Arc<Mutex<VecDeque<u8>>> = Arc::new(Mutex::new(VecDeque::with_capacity(OUTPUT_HISTORY_SIZE)));
        let output_history_for_reader = output_history.clone();

        // Clone writer for reader task to use for auto-accept
        let writer_for_reader = writer.clone();

        // Clone app for JSONL watcher (before it's moved into the reader task)
        let app_for_watcher = app.clone();
        let project_path_for_watcher = project_path.clone();
        let conversation_id_for_watcher = conversation_id.clone();
        let capture_dir = std::env::var("MOBILECLI_PTY_CAPTURE_DIR").ok();

        // Spawn task to read PTY output
        let session_id_clone = session_id.clone();
        let cli_type_for_parser = cli_type; // Copy for the spawned task
        let capture_dir_for_reader = capture_dir.clone();
        let db_for_reader = db.clone();
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut parser = OutputParser::new(cli_type_for_parser);
            let mut buffer = [0u8; 4096];
            let mut conversation_id_found = cli_type_for_parser == CliType::ClaudeCode;
            // Track if we've already auto-accepted trust prompt to prevent duplicate sends
            let mut trust_prompt_accepted = false;
            let respond_to_dsr = cli_type_for_parser == CliType::Codex;
            let mut dsr_carry: Vec<u8> = Vec::new();

            // Helper function to detect trust prompts (should auto-accept)
            // vs tool approval prompts (should show modal to user)
            fn is_trust_prompt(content: &str) -> bool {
                let lower = content.to_lowercase();
                // Trust prompts - auto-accept these
                let trust_patterns = ["do you trust the files", "execution allowed by"];
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
                    tracing::debug!(
                        "Parser notified of user input for session {}",
                        session_id_clone
                    );
                    parser.user_sent_input();

                    // ISSUE 5 FIX: Emit processing started immediately
                    // This eliminates the 1-2 second delay before mobile shows thinking indicator
                    let _ = app.emit(
                        "activity",
                        serde_json::json!({
                            "sessionId": session_id_clone,
                            "activityType": "thinking",
                            "content": "Processing...",
                            "isStreaming": true,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                    );
                }

                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let mut raw_bytes = buffer[..n].to_vec();
                        if respond_to_dsr {
                            const DSR_SEQUENCE: [u8; 4] = [0x1b, b'[', b'6', b'n'];
                            let mut combined: Vec<u8> = Vec::new();
                            if !dsr_carry.is_empty() {
                                combined.extend_from_slice(&dsr_carry);
                                dsr_carry.clear();
                            }
                            combined.extend_from_slice(&raw_bytes);

                            let mut filtered: Vec<u8> = Vec::with_capacity(combined.len());
                            let mut i = 0;
                            let mut dsr_count = 0;
                            while i < combined.len() {
                                let remaining = combined.len() - i;
                                if remaining >= 4 && combined[i..i + 4] == DSR_SEQUENCE {
                                    dsr_count += 1;
                                    i += 4;
                                    continue;
                                }
                                if remaining < 4 && combined[i] == DSR_SEQUENCE[0] {
                                    let mut is_prefix = true;
                                    for j in 0..remaining {
                                        if combined[i + j] != DSR_SEQUENCE[j] {
                                            is_prefix = false;
                                            break;
                                        }
                                    }
                                    if is_prefix {
                                        dsr_carry.extend_from_slice(&combined[i..]);
                                        break;
                                    }
                                }
                                filtered.push(combined[i]);
                                i += 1;
                            }

                            if dsr_count > 0 {
                                if let Ok(mut w) = writer_for_reader.lock() {
                                    for _ in 0..dsr_count {
                                        let _ = w.write_all(b"\x1b[1;1R");
                                    }
                                    let _ = w.flush();
                                }
                            }

                            raw_bytes = filtered;
                            if raw_bytes.is_empty() {
                                continue;
                            }
                        }

                        let output = String::from_utf8_lossy(&raw_bytes);
                        let cleaned = parser.process(&output);

                        // Try to extract conversation ID from output
                        if !conversation_id_found {
                            if let Some(conv_id) = parser.extract_conversation_id(&output) {
                                conversation_id_found = true;
                                tracing::info!(
                                    "Found conversation ID for session {}: {}",
                                    session_id_clone,
                                    conv_id
                                );

                                // Update database with conversation ID
                                let _ = db_for_reader.update_conversation_id(&session_id_clone, &conv_id);

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
                        let recent_context = parser.get_recent_context(4000);

                        // INDEPENDENT TRUST PROMPT CHECK: Run on every chunk regardless of debounce
                        // This ensures trust prompts are caught immediately when they appear
                        if !trust_prompt_accepted && is_trust_prompt(&cleaned) {
                            tracing::info!("Session {} detected trust prompt in current chunk - auto-accepting immediately", session_id_clone);
                            if let Ok(mut w) = writer_for_reader.lock() {
                                if let Err(e) = w.write_all(b"\r") {
                                    tracing::error!(
                                        "Failed to auto-accept trust prompt (immediate): {}",
                                        e
                                    );
                                } else if let Err(e) = w.flush() {
                                    tracing::error!(
                                        "Failed to flush auto-accept (immediate): {}",
                                        e
                                    );
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
                            // IMPORTANT: Combine recent_context with current chunk because
                            // tool approval options (1. Yes, 2. Yes...) often arrive AFTER
                            // the prompt pattern ("> ") that triggers waiting detection.
                            // Without combining, we'd miss the approval patterns.
                            let prompt_content = if recent_context.is_empty() {
                                cleaned.clone()
                            } else {
                                format!("{}\n{}", recent_context, cleaned)
                            };

                            let prompt_lower = prompt_content.to_lowercase();
                            let wait_type = if is_trust_prompt(&prompt_content) {
                                Some("trust_prompt".to_string())
                            } else if prompt_lower.contains("exitplanmode")
                                || prompt_lower.contains("plan mode")
                                || prompt_lower.contains("approve this plan")
                                || prompt_lower.contains("plan is complete")
                                || prompt_lower.contains("ready to implement")
                                || prompt_lower.contains("ready to code")
                            {
                                Some("plan_approval".to_string())
                            } else if prompt_lower.contains("which would you prefer")
                                || prompt_lower.contains("which option")
                                || prompt_lower.contains("what approach")
                                || prompt_lower.contains("what would you prefer")
                                || prompt_lower.contains("please select")
                                || prompt_lower.contains("askuserquestion")
                            {
                                Some("clarifying_question".to_string())
                            } else {
                                let tool_approval_patterns = [
                                    "do you want to proceed",
                                    "do you want to continue",
                                    "allow this",
                                    "1. yes",
                                    "2. yes",
                                    "3. no",
                                    "allow once",
                                    "allow always",
                                    "yes, and don't ask again",
                                    "type here to tell claude",
                                    "tab to add additional",
                                ];
                                if tool_approval_patterns.iter().any(|p| prompt_lower.contains(p)) {
                                    Some("tool_approval".to_string())
                                } else {
                                    Some("awaiting_response".to_string())
                                }
                            };

                            // AUTO-ACCEPT TRUST PROMPTS: Check if this is a trust prompt
                            // and auto-accept it by sending Enter key
                            let mut trust_prompt_handled = false;
                            if is_trust_prompt(&prompt_content) {
                                tracing::info!(
                                    "Session {} has trust prompt - auto-accepting",
                                    session_id_clone
                                );
                                // Send Enter key to auto-accept
                                if let Ok(mut w) = writer_for_reader.lock() {
                                    if let Err(e) = w.write_all(b"\r") {
                                        tracing::error!(
                                            "Failed to auto-accept trust prompt: {}",
                                            e
                                        );
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
                                maybe_write_pty_snapshot(
                                    &capture_dir_for_reader,
                                    cli_type_for_parser,
                                    &session_id_clone,
                                    &prompt_content,
                                    &output_history_for_reader,
                                );
                                let _ = app.emit(
                                    "waiting-for-input",
                                    serde_json::json!({
                                        "sessionId": session_id_clone,
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                        "promptContent": prompt_content,
                                        "waitType": wait_type,
                                        "cliType": cli_type_for_parser.as_str(),
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

                        // Emit raw bytes (base64 encoded) for xterm.js rendering on mobile
                        // This preserves all terminal escape sequences for perfect rendering
                        let _ = app.emit(
                            "pty-bytes",
                            serde_json::json!({
                                "sessionId": session_id_clone,
                                "data": BASE64.encode(&raw_bytes),
                            }),
                        );

                        // Store PTY bytes in history ring buffer for new subscribers
                        // This allows mobile clients to see recent terminal output when they connect
                        if let Ok(mut history) = output_history_for_reader.lock() {
                            // Add new bytes to the buffer
                            for byte in &raw_bytes {
                                if history.len() >= OUTPUT_HISTORY_SIZE {
                                    history.pop_front(); // Remove oldest byte to make room
                                }
                                history.push_back(*byte);
                            }
                        }

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

        // Create file watcher based on CLI type
        let cli_watcher = match cli_type {
            CliType::ClaudeCode => {
                // Claude: JSONL at ~/.claude/projects/{hash}/{session}.jsonl
                match JsonlWatcher::new(
                    session_id.clone(),
                    project_path_for_watcher,
                    conversation_id_for_watcher,
                    app_for_watcher,
                ) {
                    Ok(watcher) => {
                        tracing::info!("Created Claude JSONL watcher for session {}", session_id);
                        Some(CliWatcher::Claude(watcher))
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create Claude JSONL watcher for session {}: {}",
                            session_id,
                            e
                        );
                        None
                    }
                }
            }
            CliType::Codex => {
                // Codex: JSONL at ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl
                // Find or create the JSONL path for this session
                let codex_path = codex::find_session_file(&conversation_id_for_watcher)
                    .or_else(|| codex::get_latest_session_file());

                match codex_path {
                    Some(path) => {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            if let Some(conv_id) = codex::extract_session_id_from_filename(filename) {
                                update_session_conversation_id(&db, &app_for_watcher, &session_id, &conv_id);
                            }
                        }
                        match CodexWatcher::new(session_id.clone(), path, app_for_watcher) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created Codex JSONL watcher for session {}",
                                    session_id
                                );
                                Some(CliWatcher::Codex(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create Codex watcher for session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                    None => {
                        // No existing session file - watch the sessions directory for new files
                        // For now, we'll create a watcher that watches the sessions dir
                        tracing::info!("No Codex session file found yet, will watch for creation");
                        let sessions_dir = codex::get_codex_sessions_dir();
                        let today = chrono::Local::now();
                        let date_path = sessions_dir
                            .join(today.format("%Y").to_string())
                            .join(today.format("%m").to_string())
                            .join(today.format("%d").to_string());

                        // Create a placeholder path - the watcher will wait for the directory/file
                        let placeholder_path = date_path.join(format!(
                            "rollout-placeholder-{}.jsonl",
                            conversation_id_for_watcher
                        ));
                        match CodexWatcher::new(
                            session_id.clone(),
                            placeholder_path,
                            app_for_watcher,
                        ) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created Codex directory watcher for session {}",
                                    session_id
                                );
                                Some(CliWatcher::Codex(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create Codex watcher for session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                }
            }
            CliType::GeminiCli => {
                // Gemini: JSON at ~/.gemini/tmp/{hash}/chats/session-*.json
                let gemini_path = gemini::find_session_file(
                    &project_path_for_watcher,
                    &conversation_id_for_watcher,
                )
                .or_else(|| gemini::get_latest_session_file(&project_path_for_watcher));

                match gemini_path {
                    Some(path) => {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            if let Some(conv_id) = gemini::extract_session_id_from_filename(filename) {
                                update_session_conversation_id(&db, &app_for_watcher, &session_id, &conv_id);
                            }
                        }
                        match GeminiWatcher::new(session_id.clone(), path, app_for_watcher) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created Gemini JSON watcher for session {}",
                                    session_id
                                );
                                Some(CliWatcher::Gemini(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create Gemini watcher for session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                    None => {
                        // No existing session file - watch the chats directory
                        tracing::info!("No Gemini session file found yet, will watch for creation");
                        let chats_dir = gemini::get_project_chats_dir(&project_path_for_watcher);
                        // Create placeholder path in the chats directory
                        let placeholder_path = chats_dir.join(format!(
                            "session-placeholder-{}.json",
                            conversation_id_for_watcher
                        ));
                        match GeminiWatcher::new(
                            session_id.clone(),
                            placeholder_path,
                            app_for_watcher,
                        ) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created Gemini directory watcher for session {}",
                                    session_id
                                );
                                Some(CliWatcher::Gemini(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create Gemini watcher for session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                }
            }
            CliType::OpenCode => {
                // OpenCode: Distributed storage at ~/.local/share/opencode/storage/
                // Watch message and part directories for the session
                let opencode_session = opencode_watcher::get_latest_session()
                    .or_else(|| opencode_watcher::find_session_for_project(&project_path_for_watcher));

                match opencode_session {
                    Some(oc_session_id) => {
                        update_session_conversation_id(&db, &app_for_watcher, &session_id, &oc_session_id);
                        match OpenCodeWatcher::new(
                            session_id.clone(),
                            oc_session_id.clone(),
                            app_for_watcher,
                        ) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created OpenCode watcher for session {}, OpenCode session: {}",
                                    session_id,
                                    oc_session_id
                                );
                                Some(CliWatcher::OpenCode(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create OpenCode watcher for session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                    None => {
                        // No existing session - create watcher that will wait for session creation
                        tracing::info!(
                            "No OpenCode session found yet for {}, will watch for creation",
                            session_id
                        );
                        // Use a placeholder session ID - watcher will detect actual session
                        match OpenCodeWatcher::new(
                            session_id.clone(),
                            format!("pending_{}", conversation_id_for_watcher),
                            app_for_watcher,
                        ) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created OpenCode directory watcher for session {}",
                                    session_id
                                );
                                Some(CliWatcher::OpenCode(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create OpenCode watcher for session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                }
            }
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
                cli_watcher,
                output_history,
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

            // CRITICAL FIX: Clear any pending desktop input before sending mobile's message
            // This prevents input duplication when desktop has typed something but mobile sends first.
            // Ctrl+U (0x15) is the "kill line" sequence that clears the current line in most terminals.
            // We send this before the mobile message to ensure only the mobile's text is submitted.
            if let Err(e) = w.write_all(b"\x15") {
                tracing::error!("PTY send_input: write Ctrl+U error: {}", e);
                return;
            }
            if let Err(e) = w.flush() {
                tracing::error!("PTY send_input: flush error after Ctrl+U: {}", e);
                return;
            }

            // Write the entire input string at once
            if let Err(e) = w.write_all(input_owned.as_bytes()) {
                tracing::error!("PTY send_input: write error: {}", e);
                return;
            }
            if let Err(e) = w.flush() {
                tracing::error!("PTY send_input: flush error after text: {}", e);
                return;
            }

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
        })
        .await
        .map_err(|e| PtyError::Pty(format!("spawn_blocking failed: {}", e)))?;

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
            writer.write_all(b"\r")?;
        } else {
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
    /// ISSUE #2: Added claude_skip_permissions parameter for mobile-provided setting
    pub async fn resume_session(
        &mut self,
        session_id: String,
        project_path: String,
        conversation_id: String,
        cli_type: CliType,
        db: Arc<Database>,
        app: AppHandle,
        claude_skip_permissions: Option<bool>,
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

        let home = resolve_home_dir();
        let project_dir = resolve_project_dir(&project_path, &home)?;
        let project_path = project_dir.to_string_lossy().to_string();

        // Load config for CLI-specific settings
        let app_config = config::load_config(&app).unwrap_or_default();

        let use_skip_permissions = claude_skip_permissions.unwrap_or(app_config.claude_skip_permissions);
        if matches!(cli_type, CliType::Codex) {
            tracing::info!(
                "Codex resume starting with approval policy: {}",
                app_config.codex_approval_policy.as_flag()
            );
        }
        if use_skip_permissions && matches!(cli_type, CliType::ClaudeCode) {
            tracing::info!("Claude resume starting with --dangerously-skip-permissions");
        }

        let cli_cmd = build_cli_command_for_resume(
            cli_type,
            &project_path,
            &conversation_id,
            use_skip_permissions,
            app_config.codex_approval_policy.as_flag(),
            &home,
        );
        let mut cmd = build_command_builder(&cli_cmd, &project_dir, &home);

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

        // Ring buffer for PTY output history - allows new subscribers to see recent terminal output
        let output_history: Arc<Mutex<VecDeque<u8>>> = Arc::new(Mutex::new(VecDeque::with_capacity(OUTPUT_HISTORY_SIZE)));
        let output_history_for_reader = output_history.clone();

        // Clone writer for reader task to use for auto-accept
        let writer_for_reader = writer.clone();

        // Clone app for JSONL watcher (before it's moved into the reader task)
        let app_for_watcher = app.clone();
        let project_path_for_watcher = project_path.clone();
        let conversation_id_for_watcher = conversation_id.clone();
        let capture_dir = std::env::var("MOBILECLI_PTY_CAPTURE_DIR").ok();

        let session_id_clone = session_id.clone();
        let cli_type_for_parser = cli_type; // Copy for the spawned task
        let capture_dir_for_reader = capture_dir.clone();
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut parser = OutputParser::new(cli_type_for_parser);
            let mut buffer = [0u8; 4096];
            // Track if we've already auto-accepted trust prompt to prevent duplicate sends
            let mut trust_prompt_accepted = false;
            let respond_to_dsr = cli_type_for_parser == CliType::Codex;
            let mut dsr_carry: Vec<u8> = Vec::new();

            // Helper function to detect trust prompts (should auto-accept)
            // vs tool approval prompts (should show modal to user)
            fn is_trust_prompt(content: &str) -> bool {
                let lower = content.to_lowercase();
                // Trust prompts - auto-accept these
                let trust_patterns = ["do you trust the files", "execution allowed by"];
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
                    tracing::debug!(
                        "Parser notified of user input for resumed session {}",
                        session_id_clone
                    );
                    parser.user_sent_input();

                    // ISSUE 5 FIX: Emit processing started immediately
                    // This eliminates the 1-2 second delay before mobile shows thinking indicator
                    let _ = app.emit(
                        "activity",
                        serde_json::json!({
                            "sessionId": session_id_clone,
                            "activityType": "thinking",
                            "content": "Processing...",
                            "isStreaming": true,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }),
                    );
                }

                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut raw_bytes = buffer[..n].to_vec();
                        if respond_to_dsr {
                            const DSR_SEQUENCE: [u8; 4] = [0x1b, b'[', b'6', b'n'];
                            let mut combined: Vec<u8> = Vec::new();
                            if !dsr_carry.is_empty() {
                                combined.extend_from_slice(&dsr_carry);
                                dsr_carry.clear();
                            }
                            combined.extend_from_slice(&raw_bytes);

                            let mut filtered: Vec<u8> = Vec::with_capacity(combined.len());
                            let mut i = 0;
                            let mut dsr_count = 0;
                            while i < combined.len() {
                                let remaining = combined.len() - i;
                                if remaining >= 4 && combined[i..i + 4] == DSR_SEQUENCE {
                                    dsr_count += 1;
                                    i += 4;
                                    continue;
                                }
                                if remaining < 4 && combined[i] == DSR_SEQUENCE[0] {
                                    let mut is_prefix = true;
                                    for j in 0..remaining {
                                        if combined[i + j] != DSR_SEQUENCE[j] {
                                            is_prefix = false;
                                            break;
                                        }
                                    }
                                    if is_prefix {
                                        dsr_carry.extend_from_slice(&combined[i..]);
                                        break;
                                    }
                                }
                                filtered.push(combined[i]);
                                i += 1;
                            }

                            if dsr_count > 0 {
                                if let Ok(mut w) = writer_for_reader.lock() {
                                    for _ in 0..dsr_count {
                                        let _ = w.write_all(b"\x1b[1;1R");
                                    }
                                    let _ = w.flush();
                                }
                            }

                            raw_bytes = filtered;
                            if raw_bytes.is_empty() {
                                continue;
                            }
                        }

                        let output = String::from_utf8_lossy(&raw_bytes);
                        let cleaned = parser.process(&output);

                        // Get recent context BEFORE check_waiting_for_input, as the check may clear the buffer
                        let recent_context = parser.get_recent_context(4000);

                        // INDEPENDENT TRUST PROMPT CHECK: Run on every chunk regardless of debounce
                        // This ensures trust prompts are caught immediately when they appear
                        if !trust_prompt_accepted && is_trust_prompt(&cleaned) {
                            tracing::info!("Resumed session {} detected trust prompt in current chunk - auto-accepting immediately", session_id_clone);
                            if let Ok(mut w) = writer_for_reader.lock() {
                                if let Err(e) = w.write_all(b"\r") {
                                    tracing::error!(
                                        "Failed to auto-accept trust prompt (immediate): {}",
                                        e
                                    );
                                } else if let Err(e) = w.flush() {
                                    tracing::error!(
                                        "Failed to flush auto-accept (immediate): {}",
                                        e
                                    );
                                } else {
                                    tracing::info!("Successfully auto-accepted trust prompt (immediate) for resumed session {}", session_id_clone);
                                    parser.user_sent_input();
                                    trust_prompt_accepted = true;
                                }
                            }
                        }

                        // Check if Claude is waiting for input (use cleaned output for better pattern matching)
                        if parser.check_waiting_for_input(&cleaned) {
                            tracing::debug!(
                                "Resumed session {} is waiting for input",
                                session_id_clone
                            );
                            // Include the recent accumulated output as prompt content so mobile can detect
                            // whether this is a tool approval prompt or general waiting
                            // IMPORTANT: Combine recent_context with current chunk because
                            // tool approval options (1. Yes, 2. Yes...) often arrive AFTER
                            // the prompt pattern ("> ") that triggers waiting detection.
                            // Without combining, we'd miss the approval patterns.
                            let prompt_content = if recent_context.is_empty() {
                                cleaned.clone()
                            } else {
                                format!("{}\n{}", recent_context, cleaned)
                            };

                            let prompt_lower = prompt_content.to_lowercase();
                            let wait_type = if is_trust_prompt(&prompt_content) {
                                Some("trust_prompt".to_string())
                            } else if prompt_lower.contains("exitplanmode")
                                || prompt_lower.contains("plan mode")
                                || prompt_lower.contains("approve this plan")
                                || prompt_lower.contains("plan is complete")
                                || prompt_lower.contains("ready to implement")
                                || prompt_lower.contains("ready to code")
                            {
                                Some("plan_approval".to_string())
                            } else if prompt_lower.contains("which would you prefer")
                                || prompt_lower.contains("which option")
                                || prompt_lower.contains("what approach")
                                || prompt_lower.contains("what would you prefer")
                                || prompt_lower.contains("please select")
                                || prompt_lower.contains("askuserquestion")
                            {
                                Some("clarifying_question".to_string())
                            } else {
                                let tool_approval_patterns = [
                                    "do you want to proceed",
                                    "do you want to continue",
                                    "allow this",
                                    "1. yes",
                                    "2. yes",
                                    "3. no",
                                    "allow once",
                                    "allow always",
                                    "yes, and don't ask again",
                                    "type here to tell claude",
                                    "tab to add additional",
                                ];
                                if tool_approval_patterns.iter().any(|p| prompt_lower.contains(p)) {
                                    Some("tool_approval".to_string())
                                } else {
                                    Some("awaiting_response".to_string())
                                }
                            };

                            // AUTO-ACCEPT TRUST PROMPTS: Check if this is a trust prompt
                            // and auto-accept it by sending Enter key
                            if is_trust_prompt(&prompt_content) {
                                tracing::info!(
                                    "Resumed session {} has trust prompt - auto-accepting",
                                    session_id_clone
                                );
                                // Send Enter key to auto-accept
                                if let Ok(mut w) = writer_for_reader.lock() {
                                    if let Err(e) = w.write_all(b"\r") {
                                        tracing::error!(
                                            "Failed to auto-accept trust prompt: {}",
                                            e
                                        );
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
                            maybe_write_pty_snapshot(
                                &capture_dir_for_reader,
                                cli_type_for_parser,
                                &session_id_clone,
                                &prompt_content,
                                &output_history_for_reader,
                            );
                            let _ = app.emit(
                                "waiting-for-input",
                                serde_json::json!({
                                    "sessionId": session_id_clone,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                    "promptContent": prompt_content,
                                    "waitType": wait_type,
                                    "cliType": cli_type_for_parser.as_str(),
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

                        // Emit raw bytes (base64 encoded) for xterm.js rendering on mobile
                        // This preserves all terminal escape sequences for perfect rendering
                        let _ = app.emit(
                            "pty-bytes",
                            serde_json::json!({
                            "sessionId": session_id_clone,
                            "data": BASE64.encode(&raw_bytes),
                        }),
                    );

                        // Store PTY bytes in history ring buffer for new subscribers
                        // This allows mobile clients to see recent terminal output when they connect
                        if let Ok(mut history) = output_history_for_reader.lock() {
                            // Add new bytes to the buffer
                            for byte in &raw_bytes {
                                if history.len() >= OUTPUT_HISTORY_SIZE {
                                    history.pop_front(); // Remove oldest byte to make room
                                }
                                history.push_back(*byte);
                            }
                        }

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

        // Create file watcher based on CLI type (same logic as start_session)
        let cli_watcher = match cli_type {
            CliType::ClaudeCode => {
                match JsonlWatcher::new(
                    session_id.clone(),
                    project_path_for_watcher,
                    conversation_id_for_watcher,
                    app_for_watcher,
                ) {
                    Ok(watcher) => {
                        tracing::info!(
                            "Created Claude JSONL watcher for resumed session {}",
                            session_id
                        );
                        Some(CliWatcher::Claude(watcher))
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create Claude JSONL watcher for resumed session {}: {}",
                            session_id,
                            e
                        );
                        None
                    }
                }
            }
            CliType::Codex => {
                let codex_path = codex::find_session_file(&conversation_id_for_watcher)
                    .or_else(|| codex::get_latest_session_file());

                match codex_path {
                    Some(path) => {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            if let Some(conv_id) = codex::extract_session_id_from_filename(filename) {
                                update_session_conversation_id(&db, &app_for_watcher, &session_id, &conv_id);
                            }
                        }
                        match CodexWatcher::new(session_id.clone(), path, app_for_watcher) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created Codex JSONL watcher for resumed session {}",
                                    session_id
                                );
                                Some(CliWatcher::Codex(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create Codex watcher for resumed session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                    None => {
                        tracing::warn!("Could not find Codex session file for resume");
                        None
                    }
                }
            }
            CliType::GeminiCli => {
                let gemini_path = gemini::find_session_file(
                    &project_path_for_watcher,
                    &conversation_id_for_watcher,
                )
                .or_else(|| gemini::get_latest_session_file(&project_path_for_watcher));

                match gemini_path {
                    Some(path) => {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            if let Some(conv_id) = gemini::extract_session_id_from_filename(filename) {
                                update_session_conversation_id(&db, &app_for_watcher, &session_id, &conv_id);
                            }
                        }
                        match GeminiWatcher::new(session_id.clone(), path, app_for_watcher) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created Gemini JSON watcher for resumed session {}",
                                    session_id
                                );
                                Some(CliWatcher::Gemini(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create Gemini watcher for resumed session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                    None => {
                        tracing::warn!("Could not find Gemini session file for resume");
                        None
                    }
                }
            }
            CliType::OpenCode => {
                // OpenCode: Distributed storage at ~/.local/share/opencode/storage/
                let opencode_session = opencode_watcher::get_latest_session()
                    .or_else(|| opencode_watcher::find_session_for_project(&project_path_for_watcher));

                match opencode_session {
                    Some(oc_session_id) => {
                        update_session_conversation_id(&db, &app_for_watcher, &session_id, &oc_session_id);
                        match OpenCodeWatcher::new(
                            session_id.clone(),
                            oc_session_id.clone(),
                            app_for_watcher,
                        ) {
                            Ok(watcher) => {
                                tracing::info!(
                                    "Created OpenCode watcher for resumed session {}, OpenCode session: {}",
                                    session_id,
                                    oc_session_id
                                );
                                Some(CliWatcher::OpenCode(watcher))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create OpenCode watcher for resumed session {}: {}",
                                    session_id,
                                    e
                                );
                                None
                            }
                        }
                    }
                    None => {
                        tracing::warn!("Could not find OpenCode session for resume");
                        None
                    }
                }
            }
        };

        self.sessions.insert(
            session_id,
            PtySession {
                writer,
                master,
                _reader_task: reader_task,
                _kill_tx: kill_tx,
                user_input_tx,
                cli_watcher,
                output_history,
            },
        );

        Ok(())
    }

    /// Get the output history for a session (for sending to new subscribers)
    pub fn get_output_history(&self, session_id: &str) -> Option<Vec<u8>> {
        let session = self.sessions.get(session_id)?;
        let history = session.output_history.lock().ok()?;
        Some(history.iter().copied().collect())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
