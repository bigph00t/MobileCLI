# MobileCLI Desktop

Control Claude Code, Gemini CLI, Codex, and OpenCode from your phone while your desktop runs the sessions.

Website: https://mobilecli.app  
Docs: https://mobilecli.app/docs  
Releases: https://github.com/bigph00t/MobileCLI/releases

## Screenshots (placeholders)

Add images to `docs/images/` and they will render here.

![Desktop overview](docs/images/desktop-overview.png)
![Pairing QR screen](docs/images/desktop-qr.png)
![Mobile terminal view](docs/images/mobile-terminal.png)

## What is MobileCLI

MobileCLI is a desktop host + mobile controller. Your CLI sessions run on your desktop, and the mobile app is a secure remote keyboard, display, and approval UI. Nothing runs in the cloud by default.

## Key features

- Desktop-hosted AI CLI sessions with a mobile companion.
- Live terminal view with real-time output and resizing.
- Tool approvals and clarifying questions via quick actions.
- QR-based pairing and reconnects.
- Local network or Tailscale connectivity.

## How it works

1. Launch the desktop app on your dev machine.
2. Open Settings -> Connectivity and show the QR code.
3. Scan with the mobile app to pair and control sessions.

## Supported CLIs

| CLI | Status | Notes |
| --- | --- | --- |
| Claude Code | Full Support | Primary CLI, all features |
| Gemini CLI | Full Support | Session persistence, tool approval |
| Codex | Full Support | Session management, tool approval |
| OpenCode | Full Support | Session management |

## Repo layout

- `desktop/` - Tauri desktop app (frontend + backend bundled).
- `shared/` - Shared types and helpers.

The mobile app and the marketing website are maintained in separate repos.

## Development

Prereqs: Node.js 18+, Rust 1.70+, Tauri CLI 2.0

```bash
cd desktop
npm install
npm run tauri dev
```

## Build desktop installers

Local build (builds for your current OS only):

```bash
cd desktop
./build.sh
```

Outputs are in `desktop/src-tauri/target/release/bundle/` (dmg, exe, deb, AppImage).

## Release automation

GitHub Actions builds installers for macOS, Windows, and Linux on version tags.
The website download page pulls the latest GitHub release assets at build time.

## Security and privacy

MobileCLI is self-hosted by default. Your sessions run locally on your machine and connect directly to your phone over LAN or Tailscale. No relay service is required.

## License

MIT
