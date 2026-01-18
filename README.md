# MobileCLI

**Control Claude Code, Gemini CLI, and Codex from anywhere.**

MobileCLI lets you manage your AI coding sessions from your phone, tablet, or secondary computer. Start a session on your desktop and continue from wherever you are.

🌐 **Website:** [mobilecli.app](https://mobilecli.app)
📖 **Documentation:** [mobilecli.app/docs](https://mobilecli.app/docs)
⬇️ **Download:** [mobilecli.app/download](https://mobilecli.app/download)

## Features

- **Multi-Device Sync** - View and interact with sessions from any connected device in real-time
- **QR Code Pairing** - Scan a QR code to connect your mobile device
- **Local & Remote Access** - Connect via local network or Tailscale VPN
- **Multi-CLI Support** - Works with Claude Code, Gemini CLI, Codex, and OpenCode
- **Tool Approval** - Approve or reject tool calls from your phone
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
│                            │ Codex       │                              │
│                            └─────────────┘                              │
└─────────────────────────────────────────────────────────────────────────┘

                    Remote Access via Tailscale VPN
┌─────────────────────────────────────────────────────────────────────────┐
│                             Your Tailnet                                 │
│                                                                          │
│   ┌──────────────┐                              ┌──────────────┐        │
│   │   Desktop    │◄────── Encrypted Tunnel ────►│   Mobile     │        │
│   │   (Host)     │                              │   (Client)   │        │
│   └──────────────┘                              └──────────────┘        │
└─────────────────────────────────────────────────────────────────────────┘
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
2. MobileCLI will auto-detect Claude Code, Gemini CLI, Codex, and OpenCode
3. Generate a pairing QR code

### 3. Connect Your Mobile Device

1. Install the MobileCLI app on your phone
2. Tap "Scan QR Code" and scan the code from step 2
3. Your devices are now paired!

### 4. Start Coding

- Create a new session on the host
- Watch it appear on your mobile device
- Send messages and approve tool use from anywhere

## Remote Access with Tailscale

For access outside your local network, use Tailscale VPN:

1. Install [Tailscale](https://tailscale.com/download) on both devices
2. Sign in and connect to your Tailnet
3. Go to **Settings → Connectivity → Tailscale** on desktop
4. Scan the Tailscale QR code from your mobile app
5. Access from any network with secure, encrypted tunnels!

## Supported CLIs

MobileCLI works with:

| CLI | Status | Notes |
|-----|--------|-------|
| **Claude Code** | ✅ Full Support | Primary CLI, all features |
| **Gemini CLI** | ✅ Full Support | Session persistence, tool approval |
| **Codex** | ✅ Full Support | Session management, tool approval |
| **OpenCode** | ✅ Full Support | Session management |

## Tech Stack

| Component | Technology |
|-----------|------------|
| Desktop Framework | Tauri 2.0 |
| Desktop Frontend | React + TypeScript + Tailwind |
| Desktop Backend | Rust (tokio async runtime) |
| Mobile Framework | Expo SDK 52 |
| State Management | Zustand |

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

- **Local Network First** - Direct WebSocket connections on your LAN
- **Tailscale Integration** - Secure remote access via WireGuard-based VPN
- **No Cloud Required** - Your code and conversations stay on your devices
- **Token Authentication** - Secure pairing with unique tokens

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.

## License

MIT

---

Built with ❤️ for developers who code from anywhere.
