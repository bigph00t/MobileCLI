# MobileCLI

**Control Claude Code and Gemini CLI from anywhere.**

MobileCLI lets you manage your AI coding sessions from your phone, tablet, or secondary computer. Start a session on your desktop and continue from wherever you are.

🌐 **Website:** [mobilecli.dev](https://mobilecli.dev)
📖 **Documentation:** [mobilecli.dev/docs](https://mobilecli.dev/docs)
⬇️ **Download:** [mobilecli.dev/download](https://mobilecli.dev/download)

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

Download from [mobilecli.dev/download](https://mobilecli.dev/download):
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

[Learn more about relay setup](https://mobilecli.dev/docs/relay-setup)

## Development

### Prerequisites

- Node.js 18+
- Rust 1.70+
- Tauri CLI 2.0
- Expo CLI (for mobile)

### Desktop App

```bash
cd desktop
npm install
npm run tauri dev
```

### Mobile App

```bash
cd mobile
npm install
npx expo start
```

### Relay Server

```bash
cd relay
cargo run
```

### Website

```bash
cd website
npm install
npm run dev
```

## Project Structure

```
MobileCLI/
├── desktop/                    # Tauri desktop app
│   ├── src/                    # React frontend
│   │   ├── components/         # UI components
│   │   └── hooks/              # State management
│   └── src-tauri/              # Rust backend
│       └── src/
│           ├── config.rs       # Configuration persistence
│           ├── db.rs           # SQLite database
│           ├── pty.rs          # PTY management
│           ├── jsonl.rs        # JSONL log parsing
│           ├── ws.rs           # WebSocket server
│           ├── relay.rs        # Relay client
│           ├── client_mode.rs  # Client mode logic
│           └── input_coordinator.rs  # Multi-device input
├── mobile/                     # Expo mobile app
│   ├── app/                    # Expo Router screens
│   ├── components/             # UI components
│   └── hooks/                  # State & sync
├── relay/                      # Rust relay server
│   └── src/
│       └── main.rs
├── website/                    # Astro marketing site
│   └── src/
│       └── pages/
│           └── docs/           # Documentation
└── shared/                     # Shared TypeScript types
    ├── types.ts
    └── protocol.ts
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| Desktop Framework | Tauri 2.0 |
| Desktop Frontend | React + TypeScript + Tailwind |
| Desktop Backend | Rust (tokio async runtime) |
| PTY Management | portable-pty |
| Database | SQLite (rusqlite) |
| WebSocket | tokio-tungstenite |
| Mobile Framework | Expo SDK 52 |
| Mobile Navigation | Expo Router |
| Mobile Styling | NativeWind (Tailwind) |
| State Management | Zustand |
| Relay Server | Rust (tokio + tokio-tungstenite) |
| Website | Astro |

## Building for Production

### Desktop

```bash
cd desktop
npm run tauri build
```

Outputs:
- macOS: `.dmg` and `.app`
- Windows: `.exe` installer
- Linux: `.AppImage` and `.deb`

### Mobile

```bash
cd mobile
eas build --platform ios
eas build --platform android
```

### Relay Server

```bash
cd relay
cargo build --release
```

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
