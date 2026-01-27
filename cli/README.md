# MobileCLI - Stream Any Terminal to Your Phone

A simple CLI tool that streams any terminal session to your phone. No separate app window needed - just run `mobilecli` and your terminal is instantly accessible from your mobile device.

## Installation

```bash
# Build from source
cd cli
cargo build --release

# Install to your path
cp target/release/mobilecli ~/.local/bin/
```

## Usage

```bash
# Start your shell with mobile streaming
mobilecli

# Run a specific command
mobilecli htop
mobilecli claude
mobilecli vim myfile.txt

# Name your session (shows up in mobile app)
mobilecli -n "Work Terminal"
mobilecli -n "AI Chat" claude

# Specify a port
mobilecli -p 9850

# Skip QR code display
mobilecli --no-qr
```

## Commands

| Command | Description |
|---------|-------------|
| `mobilecli` | Start your default shell with streaming |
| `mobilecli <cmd>` | Run a command with streaming |
| `mobilecli status` | Show active streaming sessions |
| `mobilecli pair` | Generate QR code for mobile pairing |

## Options

| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | Name for this session (shown in mobile app) |
| `-p, --port <PORT>` | WebSocket port (default: auto 9847-9857) |
| `--no-qr` | Don't show QR code on startup |

## How It Works

1. When you run `mobilecli`, it spawns your command (or shell) in a PTY
2. A WebSocket server starts on your local network
3. A QR code is displayed for easy mobile pairing
4. All terminal output streams to connected mobile clients
5. Input from mobile is sent back to the terminal

## Mobile App

Scan the QR code with the MobileCLI mobile app or connect manually using the WebSocket URL shown.

## Session Management

Sessions are tracked in `~/.mobilecli/sessions.json`. Use `mobilecli status` to see active sessions:

```bash
$ mobilecli status
● 2 active session(s):

  → claude (15m)
    WebSocket: ws://localhost:9847
    Command: claude (PID: 12345)
    Directory: /home/user/project

  → Work Terminal (5m)
    WebSocket: ws://localhost:9848
    Command: bash (PID: 12346)
    Directory: /home/user/work
```

## Protocol

The WebSocket server uses a JSON protocol compatible with the MobileCLI mobile app:

### Client → Server

- `send_input` - Send keyboard input
- `pty_resize` - Resize terminal (cols, rows)
- `get_sessions` - List available sessions
- `rename_session` - Rename a session
- `ping` - Heartbeat

### Server → Client

- `welcome` - Connection established
- `session_info` - Session details
- `pty_bytes` - Terminal output (base64)
- `sessions` - List of sessions
- `session_renamed` - Rename confirmation
- `pong` - Heartbeat response

## License

MIT
