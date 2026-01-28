# MobileCLI

Stream your terminal sessions to your phone. Control Claude Code, Gemini CLI, Codex, and other AI coding assistants from anywhere.

Website: https://mobilecli.app
Releases: https://github.com/bigph00t/MobileCLI/releases

## What is MobileCLI

MobileCLI is a lightweight CLI tool that streams your terminal sessions to your phone. Run any command-line tool on your computer and view/control it from the mobile app. Perfect for AI coding assistants that need approval for tool use.

## Key features

- Stream any terminal session to your phone
- Real-time terminal output with ANSI color support
- Tool approval notifications and quick actions
- QR-based pairing for easy setup
- Local network or Tailscale connectivity
- No cloud required - everything runs locally

## Quick start

```bash
# Install
cargo install mobilecli

# One-time setup (shows QR code for mobile pairing)
mobilecli --setup

# Run any command with mobile streaming
mobilecli
```

## How it works

1. Run `mobilecli --setup` to start the daemon and show a QR code
2. Scan with the mobile app to pair
3. Run `mobilecli` in any terminal to stream that session to your phone
4. View output and send input from your phone

## Repo layout

- `cli/` - Rust CLI tool and daemon
- `mobile/` - React Native mobile app (Expo)
- `docs/` - Documentation
- `website/` - Marketing website (Astro)

## Development

### CLI

```bash
cd cli
cargo build
cargo run -- --setup
```

### Mobile app

```bash
cd mobile
npm install
npx expo start
```

## Security and privacy

MobileCLI is self-hosted. Your sessions run locally on your machine and connect directly to your phone over LAN or Tailscale. No relay service or cloud infrastructure is required.

## License

MIT
