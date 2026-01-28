//! Interactive setup wizard for MobileCLI
//!
//! Handles first-time setup and connection configuration.

use colored::Colorize;
use std::io::{self, Write};
use std::process::Command;

/// Connection mode for the CLI
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionMode {
    /// Local network (same WiFi)
    Local,
    /// Tailscale VPN
    Tailscale,
    /// Custom/manual configuration
    Custom(String),
}

/// Configuration stored for the CLI
#[derive(Debug, Clone)]
pub struct Config {
    pub connection_mode: ConnectionMode,
    pub tailscale_ip: Option<String>,
    pub local_ip: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connection_mode: ConnectionMode::Local,
            tailscale_ip: None,
            local_ip: None,
        }
    }
}

/// Check if this is the first run (no config exists)
pub fn is_first_run() -> bool {
    let config_path = get_config_path();
    !config_path.exists()
}

/// Get the config file path
fn get_config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".mobilecli")
        .join("config.json")
}

/// Load saved configuration
pub fn load_config() -> Option<Config> {
    let config_path = get_config_path();
    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let mode = match json.get("connection_mode")?.as_str()? {
        "local" => ConnectionMode::Local,
        "tailscale" => ConnectionMode::Tailscale,
        s if s.starts_with("custom:") => ConnectionMode::Custom(s[7..].to_string()),
        _ => return None,
    };

    Some(Config {
        connection_mode: mode,
        tailscale_ip: json.get("tailscale_ip").and_then(|v| v.as_str()).map(|s| s.to_string()),
        local_ip: json.get("local_ip").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}

/// Save configuration
pub fn save_config(config: &Config) -> io::Result<()> {
    let config_path = get_config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mode_str = match &config.connection_mode {
        ConnectionMode::Local => "local".to_string(),
        ConnectionMode::Tailscale => "tailscale".to_string(),
        ConnectionMode::Custom(url) => format!("custom:{}", url),
    };

    let json = serde_json::json!({
        "connection_mode": mode_str,
        "tailscale_ip": config.tailscale_ip,
        "local_ip": config.local_ip,
    });

    std::fs::write(&config_path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

/// Check Tailscale status
#[derive(Debug)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub running: bool,
    pub logged_in: bool,
    pub ip: Option<String>,
}

pub fn check_tailscale() -> TailscaleStatus {
    // Check if tailscale command exists
    let installed = Command::new("which")
        .arg("tailscale")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !installed {
        return TailscaleStatus {
            installed: false,
            running: false,
            logged_in: false,
            ip: None,
        };
    }

    // Check tailscale status
    let status_output = Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .output();

    match status_output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                let backend_state = json.get("BackendState")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let running = backend_state == "Running";
                let logged_in = running && json.get("Self").is_some();

                // Get the Tailscale IP
                let ip = json.get("TailscaleIPs")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                TailscaleStatus {
                    installed: true,
                    running,
                    logged_in,
                    ip,
                }
            } else {
                TailscaleStatus {
                    installed: true,
                    running: false,
                    logged_in: false,
                    ip: None,
                }
            }
        }
        _ => TailscaleStatus {
            installed: true,
            running: false,
            logged_in: false,
            ip: None,
        },
    }
}

/// Get local IP address
pub fn get_local_ip() -> Option<String> {
    local_ip_address::local_ip()
        .ok()
        .map(|ip| ip.to_string())
}

/// Prompt user for input
fn prompt(message: &str) -> String {
    print!("{}", message);
    if io::stdout().flush().is_err() {
        return String::new();
    }
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return String::new();
    }
    input.trim().to_string()
}

/// Prompt user for yes/no
fn prompt_yn(message: &str, default: bool) -> bool {
    let suffix = if default { "[Y/n]" } else { "[y/N]" };
    let input = prompt(&format!("{} {} ", message, suffix));

    if input.is_empty() {
        return default;
    }

    matches!(input.to_lowercase().as_str(), "y" | "yes")
}

/// Install Tailscale (Linux)
fn install_tailscale_linux() -> io::Result<bool> {
    println!();
    println!("{}", "Installing Tailscale...".cyan());
    println!("This will run: curl -fsSL https://tailscale.com/install.sh | sh");
    println!();

    if !prompt_yn("Continue?", true) {
        return Ok(false);
    }

    // Download and run install script
    let status = Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://tailscale.com/install.sh | sh")
        .status()?;

    if status.success() {
        println!("{}", "✓ Tailscale installed successfully!".green());
        Ok(true)
    } else {
        println!("{}", "✗ Tailscale installation failed".red());
        Ok(false)
    }
}

/// Install Tailscale (macOS)
fn install_tailscale_macos() -> io::Result<bool> {
    println!();
    println!("{}", "Installing Tailscale via Homebrew...".cyan());

    // Check if brew is available
    let brew_available = Command::new("which")
        .arg("brew")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !brew_available {
        println!("{}", "Homebrew not found. Please install Tailscale manually:".yellow());
        println!("  https://tailscale.com/download/mac");
        return Ok(false);
    }

    println!("This will run: brew install tailscale");
    println!();

    if !prompt_yn("Continue?", true) {
        return Ok(false);
    }

    let status = Command::new("brew")
        .args(["install", "tailscale"])
        .status()?;

    if status.success() {
        println!("{}", "✓ Tailscale installed successfully!".green());
        Ok(true)
    } else {
        println!("{}", "✗ Tailscale installation failed".red());
        Ok(false)
    }
}

/// Start Tailscale and login
fn start_tailscale() -> io::Result<bool> {
    println!();
    println!("{}", "Starting Tailscale...".cyan());

    // Try to start tailscaled (Linux)
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("sudo")
            .args(["systemctl", "start", "tailscaled"])
            .status();
    }

    // Run tailscale up
    println!("Running: tailscale up");
    println!("{}", "This will open a browser for authentication.".dimmed());
    println!();

    let status = Command::new("tailscale")
        .arg("up")
        .status()?;

    if status.success() {
        println!("{}", "✓ Tailscale connected!".green());
        Ok(true)
    } else {
        println!("{}", "✗ Tailscale connection failed".red());
        Ok(false)
    }
}

/// Run the interactive setup wizard
pub fn run_setup_wizard() -> io::Result<Config> {
    println!();
    println!("{}", "╔══════════════════════════════════════════════════════════════╗".cyan());
    println!("{}", "║              📱 MobileCLI Setup Wizard                       ║".cyan());
    println!("{}", "╚══════════════════════════════════════════════════════════════╝".cyan());
    println!();
    println!("How would you like to connect your mobile device?");
    println!();
    println!("  {} {} - Same WiFi network (easiest)", "1.".bold(), "Local Network".green());
    println!("     Good for home/office use");
    println!();
    println!("  {} {} - Connect from anywhere (recommended)", "2.".bold(), "Tailscale VPN".green());
    println!("     Secure, works on any network");
    println!();
    println!("  {} {} - Enter your own WebSocket URL", "3.".bold(), "Custom".dimmed());
    println!();

    let choice = loop {
        let input = prompt("Choose an option [1-3]: ");
        match input.as_str() {
            "1" => break 1,
            "2" => break 2,
            "3" => break 3,
            "" => break 1, // Default to local
            _ => println!("{}", "Please enter 1, 2, or 3".yellow()),
        }
    };

    let mut config = Config::default();

    match choice {
        1 => {
            // Local network
            config.connection_mode = ConnectionMode::Local;
            config.local_ip = get_local_ip();

            if let Some(ip) = &config.local_ip {
                println!();
                println!("{} Local IP: {}", "✓".green(), ip.cyan());
                println!();
                println!("{}", "Make sure your phone is on the same WiFi network.".dimmed());
            } else {
                println!();
                println!("{}", "⚠ Could not detect local IP address".yellow());
                println!("  You may need to find it manually (ifconfig / ip addr)");
            }
        }
        2 => {
            // Tailscale
            config.connection_mode = ConnectionMode::Tailscale;

            println!();
            println!("{}", "Checking Tailscale status...".dimmed());

            let mut ts_status = check_tailscale();

            // Install if needed
            if !ts_status.installed {
                println!();
                println!("{}", "Tailscale is not installed.".yellow());

                if prompt_yn("Would you like to install Tailscale now?", true) {
                    #[cfg(target_os = "macos")]
                    let installed = install_tailscale_macos()?;

                    #[cfg(target_os = "linux")]
                    let installed = install_tailscale_linux()?;

                    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                    let installed = {
                        println!("{}", "Automatic installation not supported on this OS.".yellow());
                        println!("Please install Tailscale manually: https://tailscale.com/download");
                        false
                    };

                    if installed {
                        ts_status = check_tailscale();
                    }
                }
            }

            // Login if needed
            if ts_status.installed && !ts_status.logged_in {
                println!();
                println!("{}", "Tailscale is not logged in.".yellow());

                if prompt_yn("Would you like to login now?", true) {
                    if start_tailscale()? {
                        ts_status = check_tailscale();
                    }
                }
            }

            // Get IP
            if ts_status.logged_in {
                config.tailscale_ip = ts_status.ip.clone();

                if let Some(ip) = &ts_status.ip {
                    println!();
                    println!("{} Tailscale IP: {}", "✓".green(), ip.cyan());
                    println!();
                    println!("{}", "Your phone will need Tailscale installed and logged into the same account.".dimmed());
                }
            } else {
                println!();
                println!("{}", "⚠ Tailscale not fully configured.".yellow());
                println!("  Run 'tailscale up' to complete setup.");

                // Fall back to local
                config.local_ip = get_local_ip();
            }
        }
        3 => {
            // Custom
            println!();
            let url = prompt("Enter WebSocket URL (e.g., ws://192.168.1.100:9847): ");
            config.connection_mode = ConnectionMode::Custom(url);
        }
        _ => unreachable!(),
    }

    // Save configuration
    save_config(&config)?;

    println!();
    println!("{}", "╔══════════════════════════════════════════════════════════════╗".green());
    println!("{}", "║                    ✓ Setup Complete!                         ║".green());
    println!("{}", "╚══════════════════════════════════════════════════════════════╝".green());
    println!();
    println!("Run {} to start a terminal session.", "mobilecli".cyan().bold());
    println!("Run {} to change these settings.", "mobilecli --setup".dimmed());
    println!();

    Ok(config)
}

/// Get the IP to use based on config
pub fn get_connection_ip(config: &Config) -> Option<String> {
    match &config.connection_mode {
        ConnectionMode::Local => config.local_ip.clone().or_else(get_local_ip),
        ConnectionMode::Tailscale => config.tailscale_ip.clone().or_else(|| {
            // Try to get Tailscale IP dynamically
            let status = check_tailscale();
            status.ip
        }),
        ConnectionMode::Custom(url) => {
            // Extract host from URL
            url.strip_prefix("ws://")
                .or_else(|| url.strip_prefix("wss://"))
                .and_then(|s| s.split(':').next())
                .map(|s| s.to_string())
        }
    }
}
