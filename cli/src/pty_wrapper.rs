//! PTY wrapper - spawns commands in a PTY with mobile streaming
//!
//! This is the core module that:
//! 1. Spawns the target command (or shell) in a PTY we control
//! 2. Starts a WebSocket server for mobile connections
//! 3. Streams PTY output to both local terminal AND mobile
//! 4. Relays input from mobile to the PTY
//! 5. Handles terminal resize events

use crate::protocol::ConnectionInfo;
use crate::qr;
use crate::session::{self, SessionInfo};
use crate::websocket::WsServer;
use chrono::Utc;
use colored::Colorize;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

/// Default port range for WebSocket server
const DEFAULT_PORT_START: u16 = 9847;
const DEFAULT_PORT_END: u16 = 9857;

#[derive(Error, Debug)]
pub enum WrapError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("No available port in range {0}-{1}")]
    NoAvailablePort(u16, u16),
}

/// Configuration for running a wrapped command
pub struct WrapConfig {
    pub command: String,
    pub args: Vec<String>,
    pub session_name: String,
    pub port: Option<u16>,
    pub show_qr: bool,
}

/// Resolve a command to its full path
fn resolve_command(cmd: &str) -> Option<String> {
    // First check if it's already an absolute path
    if std::path::Path::new(cmd).is_absolute() && std::path::Path::new(cmd).exists() {
        return Some(cmd.to_string());
    }

    // Use which to find in PATH
    which::which(cmd)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// Get terminal size from the current terminal
fn get_terminal_size() -> (u16, u16) {
    if let Some((w, h)) = term_size::dimensions() {
        return (w as u16, h as u16);
    }
    // Default fallback
    (80, 24)
}

/// Find an available port in the default range
async fn find_available_port(preferred: Option<u16>) -> Result<u16, WrapError> {
    if let Some(port) = preferred {
        // Try the preferred port
        if tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
            .await
            .is_ok()
        {
            return Ok(port);
        }
    }

    // Try ports in the default range
    for port in DEFAULT_PORT_START..=DEFAULT_PORT_END {
        if tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
            .await
            .is_ok()
        {
            return Ok(port);
        }
    }

    Err(WrapError::NoAvailablePort(DEFAULT_PORT_START, DEFAULT_PORT_END))
}

/// Run a command wrapped with mobile streaming
pub async fn run_wrapped(config: WrapConfig) -> Result<i32, WrapError> {
    // Resolve the command path
    let cmd_path = resolve_command(&config.command)
        .ok_or_else(|| WrapError::CommandNotFound(config.command.clone()))?;

    // Generate session ID
    let session_id = uuid::Uuid::new_v4().to_string();

    // Get current working directory
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    // Find available port
    let ws_port = find_available_port(config.port).await?;

    // Start WebSocket server
    let (ws_server, mut ws_channels) = WsServer::start(session_id.clone(), ws_port)
        .await
        .map_err(|e| WrapError::WebSocket(e.to_string()))?;

    // Register session
    let session_info = SessionInfo {
        session_id: session_id.clone(),
        name: config.session_name.clone(),
        command: config.command.clone(),
        args: config.args.clone(),
        project_path: cwd.clone(),
        ws_port,
        pid: std::process::id(),
        started_at: Utc::now(),
    };

    if let Err(e) = session::register_session(session_info) {
        tracing::warn!("Failed to register session: {}", e);
    }

    // Show connection info
    if config.show_qr {
        print_connection_banner(&config.session_name, &session_id, ws_port);
    } else {
        println!(
            "{} {} {} {}",
            "▶".green().bold(),
            config.session_name.bold(),
            "streaming on".dimmed(),
            format!("ws://localhost:{}", ws_port).cyan()
        );
    }

    // Create PTY
    let pty_system = native_pty_system();
    let (cols, rows) = get_terminal_size();

    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| WrapError::Pty(e.to_string()))?;

    // Build command
    let mut cmd = CommandBuilder::new(&cmd_path);
    cmd.args(&config.args);
    cmd.cwd(&cwd);

    // Set up environment for interactive shell
    cmd.env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()));

    // Spawn the command
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| WrapError::Pty(e.to_string()))?;

    // Drop the slave - we communicate through the master
    drop(pair.slave);

    // Get master reader/writer
    let master = pair.master;
    let mut reader = master
        .try_clone_reader()
        .map_err(|e| WrapError::Pty(e.to_string()))?;
    let mut writer = master
        .take_writer()
        .map_err(|e| WrapError::Pty(e.to_string()))?;

    // Flag to signal shutdown
    let running = Arc::new(AtomicBool::new(true));
    let running_reader = running.clone();

    // Channel for PTY output
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Spawn thread to read from PTY
    let reader_handle = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while running_reader.load(Ordering::SeqCst) {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let _ = output_tx.send(buf[..n].to_vec());
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::Interrupted {
                        break;
                    }
                }
            }
        }
    });

    // Set up Ctrl+C handler
    let running_ctrlc = running.clone();
    let _ = ctrlc::set_handler(move || {
        running_ctrlc.store(false, Ordering::SeqCst);
    });

    // Set up stdin reading (for local terminal input)
    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let running_stdin = running.clone();

    // Configure terminal for raw mode
    let original_termios = setup_raw_mode();

    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        while running_stdin.load(Ordering::SeqCst) {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = stdin_tx.send(buf[..n].to_vec());
                }
                Err(_) => break,
            }
        }
    });

    // Main event loop
    let mut stdout = std::io::stdout();
    let exit_code: i32;

    loop {
        tokio::select! {
            // PTY output
            Some(data) = output_rx.recv() => {
                // Write to local terminal
                let _ = stdout.write_all(&data);
                let _ = stdout.flush();

                // Broadcast to mobile clients
                ws_server.broadcast_pty_output(&data);
            }

            // Local stdin input
            Some(input) = stdin_rx.recv() => {
                if let Err(e) = writer.write_all(&input) {
                    tracing::debug!("Failed to write stdin to PTY: {}", e);
                }
                let _ = writer.flush();
            }

            // Input from mobile
            Some(input) = ws_channels.input_rx.recv() => {
                if let Err(e) = writer.write_all(input.as_bytes()) {
                    tracing::debug!("Failed to write mobile input to PTY: {}", e);
                }
                let _ = writer.flush();
            }

            // Resize from mobile
            Some((cols, rows)) = ws_channels.resize_rx.recv() => {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }

            // Check if child exited
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                if let Ok(Some(status)) = child.try_wait() {
                    exit_code = status.exit_code() as i32;
                    break;
                }
                if !running.load(Ordering::SeqCst) {
                    // Ctrl+C pressed, kill child
                    let _ = child.kill();
                    exit_code = 130; // Standard exit code for SIGINT
                    break;
                }
            }
        }
    }

    // Restore terminal mode
    restore_terminal_mode(original_termios);

    // Cleanup
    running.store(false, Ordering::SeqCst);
    ws_server.shutdown();

    // Wait for reader thread
    let _ = reader_handle.join();

    // Unregister session
    if let Err(e) = session::unregister_session(&session_id) {
        tracing::warn!("Failed to unregister session: {}", e);
    }

    // Print exit message
    println!();
    if exit_code == 0 {
        println!("{} Session ended", "✓".green());
    } else if exit_code == 130 {
        println!("{} Session interrupted", "•".yellow());
    } else {
        println!("{} Session ended with code {}", "✗".red(), exit_code);
    }

    Ok(exit_code)
}

/// Print the connection banner when starting a session
fn print_connection_banner(session_name: &str, session_id: &str, ws_port: u16) {
    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════╗"
            .cyan()
    );
    println!(
        "{}  {} {}",
        "║".cyan(),
        "📱 MobileCLI".bold(),
        format!("- {} ", session_name).dimmed(),
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════╝"
            .cyan()
    );

    // Show QR code if we can get local IP
    if let Ok(local_ip) = qr::get_local_ip() {
        let info = ConnectionInfo {
            ws_url: format!("ws://{}:{}", local_ip, ws_port),
            session_id: session_id.to_string(),
            session_name: Some(session_name.to_string()),
            encryption_key: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        qr::display_session_qr(&info);
    } else {
        println!(
            "  {} ws://localhost:{}",
            "Connect:".dimmed(),
            ws_port
        );
        println!();
    }

    println!(
        "{}",
        "────────────────────────────────────────────────────────────────"
            .dimmed()
    );
    println!();
}

/// Set up raw terminal mode for proper input handling
#[cfg(unix)]
fn setup_raw_mode() -> Option<nix::sys::termios::Termios> {
    use nix::sys::termios::{self, LocalFlags, SetArg};
    use std::os::fd::AsFd;

    let stdin = std::io::stdin();

    if let Ok(original) = termios::tcgetattr(stdin.as_fd()) {
        let mut raw = original.clone();
        // Disable canonical mode and echo
        raw.local_flags.remove(LocalFlags::ICANON);
        raw.local_flags.remove(LocalFlags::ECHO);
        raw.local_flags.remove(LocalFlags::ISIG);

        if termios::tcsetattr(stdin.as_fd(), SetArg::TCSANOW, &raw).is_ok() {
            return Some(original);
        }
    }
    None
}

#[cfg(not(unix))]
fn setup_raw_mode() -> Option<()> {
    None
}

/// Restore terminal mode
#[cfg(unix)]
fn restore_terminal_mode(original: Option<nix::sys::termios::Termios>) {
    use nix::sys::termios::{self, SetArg};
    use std::os::fd::AsFd;

    if let Some(termios_settings) = original {
        let stdin = std::io::stdin();
        let _ = termios::tcsetattr(stdin.as_fd(), SetArg::TCSANOW, &termios_settings);
    }
}

#[cfg(not(unix))]
fn restore_terminal_mode(_original: Option<()>) {}
