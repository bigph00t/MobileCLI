# Ralph Loop Changes Log

Append-only log. Describe code changes and rationale.

## Template

### YYYY-MM-DD HH:MM
- Iteration: <number>
- Files touched:
  - <path>
- Changes:
  - <bullet>
- Rationale:
  - <bullet>
- Risks/Notes:
  - <bullet>

---

### 2026-01-18 02:05
- Iteration: 1
- Files touched:
  - `desktop/src-tauri/src/pty.rs`
  - `desktop/src-tauri/src/ws.rs`
  - `desktop/src-tauri/src/relay.rs`
  - `desktop/src-tauri/src/jsonl.rs`
  - `desktop/src-tauri/src/jsonl_watcher.rs`
  - `desktop/src-tauri/src/claude_history.rs`
  - `mobile/hooks/useSync.ts`
  - `mobile/hooks/useNotifications.ts`
  - `mobile/components/TerminalView.tsx`
  - `RalphLoop_LONGRUN.md`
- Changes:
  - Added `waitType` in waiting-for-input emit (desktop) and propagated to WS + relay.
  - Notifications now honor explicit waitType and distinguish tool/plan/question.
  - Sanitized plan-mode tags (`<command-*>`, `<local-command-*>`) while preserving human text ("Enabled plan mode").
  - Filtered base64-like tool results in JSONL + history pipelines.
  - JSONL watcher now emits `summary` fields to mobile.
  - Mobile activity ingestion records `summary` and uses CLI-aware filtering for server activities.
  - Mobile send now guards return key and prevents newline sends (single-line input).
- Rationale:
  - Fix plan mode markup leakage, missing tool approvals, and incorrect "ready" notifications.
  - Prevent base64 garbage from history and ensure tool summaries render cleanly.
- Risks/Notes:
  - waitType detection in PTY is heuristic; JSONL/tool flows should still be authoritative.
  - Added regex dependency already present in Cargo.toml.
