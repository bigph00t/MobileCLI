# MobileCLI - Review Context

## What It Is
A mobile companion app for AI coding assistants (Claude Code, Gemini CLI, Codex, OpenCode). Users run these CLI tools on their desktop and access/control them from their phone.

## Architecture
```
Phone App (Expo/React Native)
    ↓ WebSocket
Desktop App (Tauri/Rust)
    ↓ PTY + JSONL parsing
AI CLI (claude, gemini, codex, opencode)
```

## Core Goals
1. **Remote Access** - Monitor and interact with coding sessions from anywhere
2. **Push Notifications** - Get notified when AI needs approval or input
3. **Multi-CLI Support** - One app for Claude, Gemini, Codex, and OpenCode
4. **Session Persistence** - Parse JSONL logs for conversation history

## Key Components

### Desktop (Tauri + Rust)
- PTY management for CLI processes
- JSONL file watching for real-time updates
- WebSocket server for mobile sync
- SQLite for session storage

### Mobile (Expo + React Native)
- Session list with CLI type grouping
- Real-time activity feed
- Tool approval handling (yes/no prompts)
- QR code pairing for connection setup

## Current Focus
- JSONL architecture (replacing raw PTY parsing)
- Mobile input submission
- Real-time sync reliability
- Onboarding UX (welcome modals, connection guide)

## Review Areas of Interest
- Architecture decisions
- Security model (auth tokens, WebSocket)
- UX patterns for CLI interaction on mobile
- Multi-CLI abstraction approach
