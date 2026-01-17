# MobileCLI

**Control Claude Code and Gemini CLI from anywhere.**

MobileCLI lets you manage your AI coding sessions from your phone, tablet, or secondary computer. Start a session on your desktop and continue from wherever you are.

🌐 **Website:** [mobilecli.app](https://mobilecli.app)
📖 **Documentation:** [mobilecli.app/docs](https://mobilecli.app/docs)
⬇️ **Download:** [mobilecli.app/download](https://mobilecli.app/download)

## Features

- **Multi-Device Sync** - View and interact with sessions from any connected device in real-time
- **QR Code Pairing** - Scan a QR code to securely connect your mobile device
- **Host & Client Modes** - Run as a host (manages sessions) or client (connects to host)
- **Relay Server** - Access sessions from anywhere, no port forwarding needed
- **End-to-End Encryption** - All communication is encrypted
- **Multi-CLI Support** - Works with Claude Code and Gemini CLI
- **Native Apps** - Desktop (macOS, Windows, Linux) and mobile (iOS, Android)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Your Network                                │
│                                                                          │
│   ┌──────────────┐         ┌──────────────┐         ┌──────────────┐   │
│   │   Mobile     │         │   Desktop    │         │   Desktop    │   │
│   │   (Client)   │◄───────►│   (Host)     │◄───────►│   (Client)   │   │
│   └──────────────┘         └──────┬───────┘         └──────────────┘   │
│                                   │                                     │
│                            ┌──────▼──────┐                              │
│                            │ Claude Code │                              │
│                            │ Gemini CLI  │                              │
│                            └─────────────┘                              │
└─────────────────────────────────────────────────────────────────────────┘

                                    │
                        ┌───────────▼───────────┐
                        │   Relay Server        │  (optional)
                        │   (MobileCLI Cloud)   │
                        └───────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
             ┌──────────┐   ┌──────────┐   ┌──────────┐
             │  Mobile  │   │  Laptop  │   │  Tablet  │
             │  Client  │   │  Client  │   │  Client  │
             └──────────┘   └──────────┘   └──────────┘
```

## Quick Start

### 1. Download MobileCLI

Download from [mobilecli.app/download](https://mobilecli.app/download):
- **macOS:** .dmg (Apple Silicon or Intel)
- **Windows:** .exe installer
- **Linux:** AppImage or .deb
- **Mobile:** App Store (iOS) or Play Store (Android)

### 2. Run the Setup Wizard

Launch MobileCLI and complete the setup wizard:
1. Choose **Host Mode** (on your main development machine)
2. MobileCLI will auto-detect Claude Code and Gemini CLI
3. Generate a pairing QR code

### 3. Connect Your Mobile Device

1. Install the MobileCLI app on your phone
2. Tap "Scan QR Code" and scan the code from step 2
3. Your devices are now paired!

### 4. Start Coding

- Create a new session on the host
- Watch it appear on your mobile device
- Send messages and approve tool use from anywhere

## Remote Access with Relay

For access outside your local network, enable the relay server:

1. Go to **Settings → Relay** on your host
2. Enable "Relay Connection"
3. Re-scan the QR code on your mobile (it now includes relay URL)
4. Access from any network!

[Learn more about relay setup](https://mobilecli.app/docs/relay-setup)

## Supported CLIs

MobileCLI works with:

| CLI | Status | Notes |
|-----|--------|-------|
| **Claude Code** | ✅ Full Support | Primary CLI, all features |
| **Gemini CLI** | ✅ Full Support | Session persistence, tool approval |
| **Codex** | 🧪 Experimental | Basic session management |
| **OpenCode** | 🧪 Experimental | Basic session management |

## Tech Stack

| Component | Technology |
|-----------|------------|
| Desktop Framework | Tauri 2.0 |
| Desktop Frontend | React + TypeScript + Tailwind |
| Desktop Backend | Rust (tokio async runtime) |
| Mobile Framework | Expo SDK 52 |
| State Management | Zustand |
| Relay Server | Rust (tokio + tokio-tungstenite) |

## Building from Source (Desktop Only)

If you want to build the desktop app yourself:

### Prerequisites

- Node.js 18+
- Rust 1.70+
- Tauri CLI 2.0

### Build

```bash
cd desktop
npm install
npm run tauri build
```

Outputs:
- macOS: `.dmg` and `.app`
- Windows: `.exe` installer
- Linux: `.AppImage` and `.deb`

## Security

MobileCLI prioritizes security:

- **End-to-End Encryption** - Messages encrypted before leaving your device
- **Token Authentication** - Secure pairing with unique tokens
- **Zero Knowledge Relay** - Relay server cannot read your messages
- **TLS Transport** - All connections use TLS 1.3
- **No Cloud Storage** - Your code and conversations stay on your devices

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.

## License

MIT

---

Built with ❤️ for developers who code from anywhere.
