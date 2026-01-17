# Other CLI Support Investigation

## Overview

This document details the investigation of multi-CLI support issues in MobileCLI, specifically focusing on OpenCode, Codex, and Gemini CLI integration problems.

---

## Issue Summary

### Issues Observed (Screenshots 2026-01-17)

1. **OpenCode session not syncing to mobile** - Session created on desktop doesn't appear on mobile until app restart
2. **Chat not working properly** - Mobile shows "Processing..." when OpenCode already responded
3. **Wrong model displayed** - Shows "Claude 3.5 Sonnet" when OpenCode is using "GPT-5.2 Codex"
4. **Status text hardcoded** - Fixed: "Claude working..." now shows CLI-specific name

---

## Root Cause Analysis

### Critical Finding: No File Watcher for OpenCode

**Location:** `desktop/src-tauri/src/pty.rs:781-789`

```rust
CliType::OpenCode => {
    // OpenCode uses a distributed file system that's more complex to watch
    // For now, skip OpenCode file watching (would need multiple directory watchers)
    tracing::info!(
        "OpenCode session {} - file watching not yet implemented",
        session_id
    );
    None
}
```

**Impact:** Without a file watcher, OpenCode sessions rely ONLY on:
1. PTY terminal output parsing (which may be unreliable)
2. No JSONL log parsing for activities/messages
3. No conversation ID persistence

### Watcher Implementation Status

| CLI | Watcher | File | Status |
|-----|---------|------|--------|
| Claude Code | `JsonlWatcher` | `jsonl_watcher.rs` | ✅ Full support |
| Gemini CLI | `GeminiWatcher` | `gemini_watcher.rs` | ✅ Full support |
| Codex | `CodexWatcher` | `codex_watcher.rs` | ✅ Full support |
| OpenCode | **NONE** | - | ❌ NOT IMPLEMENTED |

---

## Issue 1: Session Not Syncing

### Session Creation Flow

1. User clicks "New Session" on desktop
2. `create_session` command in `lib.rs:140-188` creates DB entry
3. Emits `session-created` event (line 161-172)
4. WebSocket broadcasts `SessionCreated` to mobile (ws.rs:532-547)
5. Mobile receives and adds session (useSync.ts:1352-1364)

**This flow appears correct for all CLI types.** The `session-created` event includes `cliType` field.

### Potential Issues

1. **Relay encryption timing** - If relay isn't fully connected when session is created, message may be lost
2. **WebSocket race condition** - Mobile may not have WebSocket open during initial session creation
3. **Mobile subscription state** - Mobile may not be subscribed when event arrives

### Investigation Needed

- [ ] Add logging to trace `session-created` event from desktop → relay → mobile
- [ ] Check if relay is connected when OpenCode session is created
- [ ] Verify mobile WebSocket is open and subscribed

---

## Issue 2: Chat Not Working (Processing... Stuck)

### Problem

Mobile shows "Processing..." after user sends message, but OpenCode already responded on desktop.

### Root Cause Analysis

**Parser relies on response markers:**

```rust
// parser.rs:184
CliType::OpenCode => ('●', '│'),   // OpenCode uses similar markers to Claude
```

**But OpenCode's actual output format may differ:**
- OpenCode may use different Unicode characters
- OpenCode may have different terminal escape sequences
- Output patterns may not match Claude's markers

### Why Claude Works But OpenCode Doesn't

| Aspect | Claude | OpenCode |
|--------|--------|----------|
| Log files | JSONL in `~/.claude/projects/` | Different location/format |
| File watcher | ✅ Watches JSONL | ❌ Not implemented |
| Parser markers | Verified working | Assumed similar (unverified) |
| Response detection | Via file + PTY | PTY only |

### Investigation Needed

- [ ] Capture actual OpenCode terminal output to verify marker characters
- [ ] Determine if OpenCode has log files that could be watched
- [ ] Check OpenCode's output format documentation

---

## Issue 3: Wrong Model Displayed

### Problem

Mobile header shows "Claude 3.5 Sonnet" for an OpenCode session using "GPT-5.2 Codex"

### Code Location

`mobile/app/session/[id].tsx:401`
```tsx
{currentModel || modelOptions[0]?.name || cliType}
```

### Root Cause

1. `currentModel` state is `null` (not detected from session)
2. Falls back to `modelOptions[0]?.name`
3. For OpenCode, first option is "Claude 3.5 Sonnet"

```tsx
// [id].tsx:33-38
opencode: [
    { id: 'claude-3-5-sonnet', name: 'Claude 3.5 Sonnet' },  // ← First option
    { id: 'gpt-4o', name: 'GPT-4o' },
    { id: 'gemini-pro', name: 'Gemini Pro' },
],
```

### Fix Options

1. **Parse model from terminal output** - Look for model indicator in OpenCode startup
2. **Don't show model for non-Claude CLIs** - Only Claude has reliable model detection
3. **Show CLI name instead** - Display "OpenCode" rather than guessing model

### Recommended Fix

```tsx
// Show CLI name instead of first model option for non-Claude CLIs
const displayModel = session?.cliType === 'claude'
    ? (currentModel || modelOptions[0]?.name)
    : cliInfo.name;
```

---

## Issue 4: Status Text (FIXED)

### Desktop Fix (Sidebar.tsx)

```tsx
// Added CLI_NAMES mapping
const CLI_NAMES: Record<string, string> = {
  claude: 'Claude',
  gemini: 'Gemini',
  codex: 'Codex',
  opencode: 'OpenCode',
};

// Dynamic status text
const cliName = CLI_NAMES[session.cliType] || 'CLI';
const statusConfig = {
  working: { color: 'bg-[#9ece6a]', text: `${cliName} working...` },
  // ...
};
```

### Mobile Fix (index.tsx)

```tsx
// Moved cliInfo lookup before statusConfig
const cliInfo = CLI_TYPES.find(c => c.id === item.cliType) || CLI_TYPES[0];

// Dynamic status text
working: { color: colors.success, symbol: '●', subtext: `${cliInfo.name} working...` },
```

---

## OpenCode Log File Investigation

### What We Need to Find

To implement proper OpenCode support, we need to determine:

1. **Does OpenCode write log files?**
   - Check `~/.opencode/` directory structure
   - Look for JSONL, JSON, or other log formats

2. **What is the log format?**
   - Message structure
   - Activity/tool call tracking
   - Conversation ID storage

3. **Where are conversations stored?**
   - Per-project or global
   - File naming convention

### Potential Locations to Check

```bash
~/.opencode/
~/.opencode/logs/
~/.opencode/conversations/
~/.opencode/sessions/
$PROJECT_DIR/.opencode/
```

---

## Implementation Plan (When Ready)

### Phase 1: Investigation
- [ ] SSH into machine with OpenCode installed
- [ ] Run OpenCode session and examine output
- [ ] Find log file location and format
- [ ] Document terminal output patterns

### Phase 2: File Watcher (if logs exist)
- [ ] Create `opencode_watcher.rs` similar to `codex_watcher.rs`
- [ ] Implement log file parsing
- [ ] Add conversation ID detection
- [ ] Test activity/message extraction

### Phase 3: Parser Improvements
- [ ] Verify/fix response marker characters
- [ ] Add OpenCode-specific thinking detection
- [ ] Test "waiting for input" detection

### Phase 4: Mobile Fixes
- [ ] Fix model display logic
- [ ] Improve session sync reliability
- [ ] Add OpenCode-specific UI considerations

---

## Comparison: How Claude Works vs OpenCode

### Claude Code (Working)

```
User sends message
    ↓
PTY captures terminal output → Parser extracts thinking/response
    ↓
JSONL file updated → JsonlWatcher detects change
    ↓
Activities extracted from JSONL → Emitted to mobile
    ↓
Mobile shows complete, accurate activity feed
```

### OpenCode (Broken)

```
User sends message
    ↓
PTY captures terminal output → Parser may miss response markers
    ↓
NO FILE WATCHER → Activities/messages not reliably detected
    ↓
Mobile stuck on "Processing..." or missing data
```

---

## Questions for Future Investigation

1. Does OpenCode support `--json` output mode?
2. Is there an OpenCode API or protocol we could use instead of terminal parsing?
3. Can we request log file support from OpenCode maintainers?
4. Should we fall back to "basic mode" for unsupported CLIs (just terminal view)?

---

## Files Modified in This Investigation

### Fixed

- `desktop/src/components/Sidebar.tsx` - CLI-specific status text
- `mobile/app/(tabs)/index.tsx` - CLI-specific status text

### Need Changes

- `mobile/app/session/[id].tsx` - Model display logic
- `desktop/src-tauri/src/pty.rs` - OpenCode watcher implementation
- New file: `desktop/src-tauri/src/opencode_watcher.rs` - If logs exist

---

## Summary

The core issue is that **OpenCode has no file watcher implementation**, making it rely entirely on PTY terminal parsing which is unreliable. Claude and Gemini work well because they have dedicated watchers that parse their log files for accurate activity/message extraction.

**Recommendation:** Before implementing fixes, investigate OpenCode's log file structure. If no logs exist, consider:
1. Request log file support from OpenCode team
2. Implement "basic mode" for unsupported CLIs (terminal-only, no activity feed)
3. Document OpenCode as "experimental" with limited features
