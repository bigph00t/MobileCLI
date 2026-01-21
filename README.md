# MobileCLI

Control Claude Code, Gemini CLI, and Codex from your phone while your desktop runs the sessions.

Website: https://mobilecli.app
Docs: https://mobilecli.app/docs
Download: https://mobilecli.app/download

## What it does

- Host AI CLI sessions on your desktop and continue them on mobile.
- View session status, working directory, and activity in real time.
- Send messages and approve tool calls without leaving your desk.
- Pair devices quickly with a QR code.
- Connect on local network or via Tailscale.

## How it works

1. Run the Desktop app on your main dev machine.
2. Open Settings (gear icon) -> Connectivity and generate a QR code.
3. Scan the QR in the MobileCLI app to sync sessions instantly.

## Connection options

- Local network: same Wi-Fi, lowest latency.
- Tailscale: connect from anywhere on your Tailnet.
- Direct device-to-device; no relay server.

## Supported CLIs

| CLI | Status | Notes |
| --- | --- | --- |
| Claude Code | Full Support | Primary CLI, all features |
| Gemini CLI | Full Support | Session persistence, tool approval |
| Codex | Full Support | Session management, tool approval |
| OpenCode | Full Support | Session management |

## Repo layout

- desktop/ - Tauri desktop host/client app.
- mobile/ - Expo mobile client.
- website/ - Marketing site.
- shared/ - Shared types and helpers.
- relay/ - Legacy relay service (not used in production).

## Development

### Desktop

Prereqs: Node.js 18+, Rust 1.70+, Tauri CLI 2.0

```bash
cd desktop
npm install
npm run tauri dev
```

### Mobile

Prereqs: Node.js 18+, Expo CLI

```bash
cd mobile
npm install
npm run start
```

## License

MIT
