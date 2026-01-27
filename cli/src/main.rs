//! MobileCLI - Stream any terminal session to your phone
//!
//! Usage:
//!   mobilecli              # Start your shell with mobile streaming
//!   mobilecli <command>    # Run a command with mobile streaming
//!   mobilecli -n "Work"    # Name your session
//!   mobilecli status       # Show active sessions
//!   mobilecli --help       # Show help

mod pty_wrapper;
mod websocket;
mod qr;
mod protocol;
mod session;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "mobilecli")]
#[command(author = "bigphoot")]
#[command(version)]
#[command(about = "Stream any terminal session to your phone", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Command to run (defaults to your shell if not specified)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,

    /// Name for this session (shown in mobile app)
    #[arg(short = 'n', long = "name")]
    session_name: Option<String>,

    /// WebSocket port to use (default: auto-select from 9847-9857)
    #[arg(short = 'p', long = "port")]
    port: Option<u16>,

    /// Don't show QR code on startup
    #[arg(long = "no-qr")]
    no_qr: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Show active streaming sessions
    Status,
    /// Generate QR code for mobile pairing (standalone, no session)
    Pair,
}

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mobilecli=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    // Handle subcommands
    if let Some(command) = cli.command {
        return match command {
            Commands::Status => {
                session::show_status();
                ExitCode::SUCCESS
            }
            Commands::Pair => {
                match qr::show_pairing_qr().await {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("{}: {}", "Error".red().bold(), e);
                        ExitCode::FAILURE
                    }
                }
            }
        };
    }

    // Determine what command to run
    let (command, args) = if cli.args.is_empty() {
        // No command specified - run user's shell
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        (shell, vec![])
    } else {
        // Command specified
        let mut args = cli.args;
        let command = args.remove(0);
        (command, args)
    };

    // Generate session name
    let session_name = cli.session_name.unwrap_or_else(|| {
        // Use command name as default session name
        std::path::Path::new(&command)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Terminal".to_string())
    });

    // Run the wrapped command
    let config = pty_wrapper::WrapConfig {
        command,
        args,
        session_name,
        port: cli.port,
        show_qr: !cli.no_qr,
    };

    match pty_wrapper::run_wrapped(config).await {
        Ok(exit_code) => ExitCode::from(exit_code as u8),
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            ExitCode::FAILURE
        }
    }
}
