//! MobileCLI - Stream any terminal session to your phone
//!
//! Usage:
//!   mobilecli              # Start your shell with mobile streaming
//!   mobilecli <command>    # Run a command with mobile streaming
//!   mobilecli -n "Work"    # Name your session
//!   mobilecli --setup      # Run setup wizard
//!   mobilecli status       # Show active sessions
//!   mobilecli --help       # Show help

mod pty_wrapper;
mod websocket;
mod qr;
mod protocol;
mod session;
mod setup;

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

    /// Run the setup wizard to configure connection settings
    #[arg(long = "setup")]
    setup: bool,

    /// Use local network connection (same WiFi)
    #[arg(long = "local")]
    use_local: bool,

    /// Use Tailscale VPN connection
    #[arg(long = "tailscale")]
    use_tailscale: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Show active streaming sessions
    Status,
    /// Generate QR code for mobile pairing (standalone, no session)
    Pair,
    /// Run the setup wizard
    Setup,
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

    // Run setup wizard if requested or first run
    if cli.setup {
        match setup::run_setup_wizard() {
            Ok(_) => return ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{}: {}", "Setup error".red().bold(), e);
                return ExitCode::FAILURE;
            }
        }
    }

    // Handle subcommands
    if let Some(command) = &cli.command {
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
            Commands::Setup => {
                match setup::run_setup_wizard() {
                    Ok(_) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("{}: {}", "Setup error".red().bold(), e);
                        ExitCode::FAILURE
                    }
                }
            }
        };
    }

    // Check for first run - show setup wizard
    if setup::is_first_run() && cli.args.is_empty() {
        println!();
        println!("{}", "Welcome to MobileCLI! Let's get you set up.".cyan().bold());
        match setup::run_setup_wizard() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{}: {}", "Setup error".red().bold(), e);
                return ExitCode::FAILURE;
            }
        }
    }

    // Load config (or use defaults)
    let config = setup::load_config().unwrap_or_default();

    // Determine connection mode from flags or config
    let connection_mode = if cli.use_tailscale {
        setup::ConnectionMode::Tailscale
    } else if cli.use_local {
        setup::ConnectionMode::Local
    } else {
        config.connection_mode.clone()
    };

    // Get the IP to use for QR code
    let connection_ip = match &connection_mode {
        setup::ConnectionMode::Local => setup::get_local_ip(),
        setup::ConnectionMode::Tailscale => {
            let ts = setup::check_tailscale();
            if !ts.logged_in {
                eprintln!("{}", "⚠ Tailscale not connected. Run 'mobilecli --setup' or 'tailscale up'".yellow());
                setup::get_local_ip() // Fall back to local
            } else {
                ts.ip.or_else(setup::get_local_ip)
            }
        }
        setup::ConnectionMode::Custom(_) => setup::get_connection_ip(&config),
    };

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
    let wrap_config = pty_wrapper::WrapConfig {
        command,
        args,
        session_name,
        port: cli.port,
        show_qr: !cli.no_qr,
        connection_ip,
    };

    match pty_wrapper::run_wrapped(wrap_config).await {
        Ok(exit_code) => ExitCode::from(exit_code as u8),
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            ExitCode::FAILURE
        }
    }
}
