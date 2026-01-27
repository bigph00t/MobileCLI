//! QR code generation for terminal display
//!
//! Generates QR codes that can be scanned by the mobile app.

use crate::protocol::ConnectionInfo;
use colored::Colorize;
use qrcode::QrCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum QrError {
    #[error("Failed to generate QR code: {0}")]
    Generation(String),
    #[error("Failed to get local IP: {0}")]
    LocalIp(String),
}

/// Default WebSocket port
pub const DEFAULT_WS_PORT: u16 = 9847;

/// Get the local IP address for LAN connections
pub fn get_local_ip() -> Result<String, QrError> {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .map_err(|e| QrError::LocalIp(e.to_string()))
}

/// Generate and display a QR code in the terminal
pub fn display_qr(data: &str) -> Result<(), QrError> {
    let code = QrCode::new(data.as_bytes())
        .map_err(|e| QrError::Generation(e.to_string()))?;

    // Use Unicode block characters for high-density QR display
    let string = code
        .render::<char>()
        .quiet_zone(true)
        .module_dimensions(2, 1)
        .build();

    println!("{}", string);
    Ok(())
}

/// Generate connection info for a session
#[allow(dead_code)]
pub fn generate_connection_info(
    session_id: &str,
    ws_port: u16,
    encryption_key: Option<String>,
) -> Result<ConnectionInfo, QrError> {
    let local_ip = get_local_ip()?;

    Ok(ConnectionInfo {
        ws_url: format!("ws://{}:{}", local_ip, ws_port),
        session_id: session_id.to_string(),
        session_name: None,
        encryption_key,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Show pairing QR code for mobile app
pub async fn show_pairing_qr() -> Result<(), QrError> {
    let local_ip = get_local_ip()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let info = ConnectionInfo {
        ws_url: format!("ws://{}:{}", local_ip, DEFAULT_WS_PORT),
        session_id,
        session_name: None,
        encryption_key: None, // TODO: Add encryption
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════╗"
            .cyan()
    );
    println!(
        "{}",
        "║           📱 Scan with MobileCLI app to connect              ║"
            .cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════╝"
            .cyan()
    );
    println!();

    display_qr(&info.to_qr_data())?;

    println!();
    println!(
        "  {} {}",
        "WebSocket:".dimmed(),
        info.ws_url.green()
    );
    println!(
        "  {} {}",
        "Local IP:".dimmed(),
        local_ip.yellow()
    );
    println!();
    println!(
        "{}",
        "Make sure your phone is on the same WiFi network.".dimmed()
    );
    println!();

    Ok(())
}

/// Display inline QR code for a session (smaller, for embedding in terminal)
pub fn display_session_qr(info: &ConnectionInfo) {
    println!();
    println!(
        "  {} {}",
        "📱".to_string(),
        "Scan to connect from mobile:".cyan().bold()
    );

    if let Ok(code) = QrCode::new(info.to_qr_data().as_bytes()) {
        let string = code
            .render::<char>()
            .quiet_zone(true)
            .module_dimensions(2, 1)
            .build();

        // Indent the QR code
        for line in string.lines() {
            println!("  {}", line);
        }
    }

    println!();
    println!(
        "  {} {}",
        "Or connect to:".dimmed(),
        info.ws_url.green()
    );
    println!();
}
