# Ralph Loop: Rust Backend Fixes

## Session Identity

You are working on **MobileCLI's Rust backend** (Tauri 2.0). This is a parallel workstream - another session is handling mobile TypeScript. **DO NOT touch mobile files.**

## Your Files (Exclusive Ownership)

```
desktop/src-tauri/src/pty.rs           - PTY management, thinking detection
desktop/src-tauri/src/parser.rs        - Output parsing, waiting detection
desktop/src-tauri/src/lib.rs           - App entry, commands
desktop/src-tauri/src/ws.rs            - WebSocket server
desktop/src-tauri/src/opencode_watcher.rs  - NEW FILE TO CREATE
desktop/src-tauri/src/codex_watcher.rs     - Reference for patterns
desktop/src-tauri/src/gemini_watcher.rs    - Reference for patterns
desktop/src-tauri/src/jsonl_watcher.rs     - Reference for patterns
```

---

## Git Workflow (CRITICAL - Follow This Exactly)

### Initial Setup (Do This First!)

```bash
cd /home/bigphoot/Desktop/Projects/MobileCLI

# Ensure you're on main and up to date
git checkout main
git pull origin main 2>/dev/null || true

# Create your working branch
git checkout -b fix/rust-backend

# Verify you're on the right branch
git branch --show-current
# Should output: fix/rust-backend
```

### Commit Strategy

**Commit after EACH issue/task is complete, not at the end.** This provides:
- Rollback points if something breaks
- Clear history for the reviewer
- Parallel visibility (the other session can see your progress)

**Commit message format:**
```
[Rust] Phase N: Short description

- Bullet point of specific change
- Another specific change
- Files modified: file1.rs, file2.rs
```

**Example commits:**
```bash
git add desktop/src-tauri/src/opencode_watcher.rs desktop/src-tauri/src/pty.rs
git commit -m "[Rust] Phase 1: OpenCode file watcher implementation

- Created opencode_watcher.rs with session/message/part file watching
- Added OpenCode variant to CliWatcher enum in pty.rs
- Replaced None return with watcher initialization at pty.rs:789
- Files modified: opencode_watcher.rs (new), pty.rs"
```

### Commit Checkpoints

Commit at these specific points:

| After Completing | Commit Message Start |
|------------------|---------------------|
| Issue 1: OpenCode Watcher | `[Rust] Phase 1: OpenCode watcher` |
| Issue 2: Hook Thinking Fix | `[Rust] Phase 2: Hook output exclusion` |
| Issue 3: Waiting Detection | `[Rust] Phase 3: Waiting detection fix` |
| Issue 4: WebSocket Reliability | `[Rust] Phase 4: WebSocket event queue` |
| Issue 5: Processing Event | `[Rust] Phase 5: Immediate processing event` |
| Bug fixes / adjustments | `[Rust] Fix: <description>` |

### View Your Progress

```bash
# See all commits on your branch
git log --oneline fix/rust-backend

# See what files you've changed
git diff --name-only main...fix/rust-backend

# See full diff
git diff main...fix/rust-backend
```

### If Something Goes Wrong

```bash
# Undo last commit but keep changes
git reset --soft HEAD~1

# Discard all uncommitted changes (DANGEROUS)
git checkout -- .

# Go back to a specific commit
git log --oneline  # Find the commit hash
git reset --hard <hash>
```

### DO NOT

- ❌ Push to origin (keep local until reviewed)
- ❌ Merge main into your branch (we'll handle conflicts at the end)
- ❌ Commit mobile/ files (you don't own them)
- ❌ Make huge commits with all changes at once

### Conflict Prevention

Your branch only touches files in `desktop/src-tauri/src/`. The mobile session only touches `mobile/`. There should be ZERO conflicts. If you see a conflict, STOP and ask for help.

---

## Parallel Session Awareness

Another Claude session is working on `fix/mobile-stack` branch with these files:
```
mobile/hooks/useSync.ts
mobile/hooks/patterns.ts
mobile/components/TerminalView.tsx
mobile/components/ToolApprovalModal.tsx
mobile/app/session/[id].tsx
```

**Do not touch these files.** If you need changes in mobile code to support your Rust changes, document it in your commit message:

```
[Rust] Phase 4: WebSocket event queue

- Added event queuing for session_created
- NOTE FOR MOBILE SESSION: Mobile should call get_all_activities on reconnect
```

---

## Critical Issues to Fix (Priority Order)

### Issue 1: OpenCode Has NO File Watcher (CRITICAL)

**Symptom**: OpenCode sessions don't sync to mobile. Mobile shows "Processing..." forever.

**Root Cause**:
```rust
// pty.rs:781-789
CliType::OpenCode => {
    // OpenCode uses a distributed file system that's more complex to watch
    // For now, skip OpenCode file watching (would need multiple directory watchers)
    tracing::info!(
        "OpenCode session {} - file watching not yet implemented",
        session_id
    );
    None  // <-- THIS IS THE PROBLEM
}
```

**OpenCode Storage Format** (verified on this machine):
```
~/.local/share/opencode/storage/
├── session/<project_hash>/ses_*.json     # Session metadata
├── message/<session_id>/msg_*.json       # Message metadata
└── part/msg_<message_id>/prt_*.json      # Actual text content
```

**Example Files**:
- Session: `~/.local/share/opencode/storage/session/895979672f5f2c6c10f5b424415f6220ab3c08c9/ses_432ebb918ffeXO5bSjbUgqj8j0.json`
- Message: Contains `role: "assistant"` or `role: "user"`, plus timing
- Part: Contains `type: "text"` and actual content

**Your Task**:
1. Create `opencode_watcher.rs` following `codex_watcher.rs` patterns
2. Watch all three directories for changes
3. Parse JSON to extract:
   - Sessions → emit `session-created` events
   - Messages + Parts → emit `activity` events
4. Add `OpenCode(OpenCodeWatcher)` variant to `CliWatcher` enum in pty.rs:23
5. Replace the `None` at pty.rs:789 with watcher initialization

**Think About**:
- How to correlate message.json with part.json files (they share message ID)
- Whether to use polling or inotify/notify crate
- How to handle rapid file updates without spam

---

### Issue 2: Hook Output Misclassified as Thinking (HIGH)

**Symptom**: "Running stop hooks... 2/6" appears in thinking indicator.

**Root Cause** (pty.rs:141-154):
```rust
// Check for dynamic progress messages (lines ending with ... that look like status)
if !is_thinking && content_to_check.ends_with("...") && content_to_check.len() < 100 {
    // This matches hook output like "Running stop hooks..."
    let has_special_chars = content_to_check.chars().any(|c| matches!(c, ...));
    if !has_special_chars {
        is_thinking = true;  // <-- FALSE POSITIVE
    }
}
```

**Also** (pty.rs:156-166):
```rust
if lower.contains("thinking") || lower.contains("thought for") || lower.contains("esc to interrupt") {
    is_thinking = true;  // Hook errors can contain these strings
}
```

**Your Task** (pty.rs:70-200 `detect_and_emit_thinking`):
1. Add hook exclusion filter at the START of the function:
   ```rust
   // FIRST: Reject any line that looks like hook output
   let lower = trimmed.to_lowercase();
   let hook_keywords = ["hook", "hooks", "posttooluse", "pretooluse",
                        "stop hook", "ran ", "/6", "error:", "failed"];
   if hook_keywords.iter().any(|k| lower.contains(k)) {
       continue;  // Not thinking, skip this line entirely
   }
   ```
2. Tighten the `...` heuristic: require EITHER spinner prefix OR thinking word, not just `...`
3. Add line buffering to avoid partial fragment emissions

**Think About**:
- What other hook patterns exist beyond the ones listed?
- Could user content accidentally match hook patterns? (probably fine, hooks are system-generated)
- Should we log rejected lines for debugging?

---

### Issue 3: `waiting_for_input` Suppressed by Stale Thinking (HIGH)

**Symptom**: Mobile stays stuck on "Processing..." after Claude finishes.

**Root Cause** (parser.rs:206-240):
```rust
pub fn check_waiting_for_input(&mut self, text: &str) -> bool {
    // ...
    // If chunk contains any thinking pattern, waiting detection fails
    // But stale thinking text from previous output can poison the chunk
}
```

**Your Task** (parser.rs):
1. Find where `is_still_thinking` is checked in waiting detection
2. Add exemption for hook output (same patterns as Issue 2)
3. Only check "still thinking" on the LAST LINE of the chunk, not entire text
4. Consider: emit `waiting_for_input` anyway if prompt is detected, regardless of thinking patterns

**Think About**:
- What's the actual lifecycle? When does thinking end vs when should waiting fire?
- Could we use a state machine instead of substring matching?

---

### Issue 4: WebSocket Event Reliability (MEDIUM)

**Symptom**: Sessions don't appear on mobile until app restart.

**Root Cause**: Events can be lost if mobile isn't subscribed during relay initialization.

**Your Task** (ws.rs):
1. Find `session_created` broadcast (around line 532-547)
2. Add event queuing: if no clients connected, queue the event
3. On client reconnect, send queued events
4. Add `get_all_activities` handler that mobile can call on reconnect

**Think About**:
- Should the queue be bounded? What if thousands of events queue up?
- Should we just rely on mobile calling `get_sessions` on reconnect?

---

### Issue 5: Emit Processing Started Immediately (MEDIUM)

**Symptom**: 1-2 second delay before thinking appears for desktop-originated prompts.

**Location**: pty.rs around `send_input` function

**Your Task**:
1. Find where user input is written to PTY
2. Immediately emit a "processing started" activity BEFORE waiting for PTY output
3. This gives mobile instant feedback

```rust
// In send_input or equivalent:
let _ = app.emit("activity", serde_json::json!({
    "sessionId": session_id,
    "type": "thinking",
    "content": "Processing...",
    "isStreaming": true,
    "source": "local",
}));
// Then write to PTY
```

---

## Development Workflow

### Build & Test
```bash
cd /home/bigphoot/Desktop/Projects/MobileCLI/desktop
cargo check           # Fast syntax check
npm run tauri dev     # Full dev build with hot reload
```

### Test OpenCode Watcher
```bash
# Terminal 1: Run desktop app
cd desktop && npm run tauri dev

# Terminal 2: Create OpenCode session
opencode /tmp/test-project

# Terminal 3: Watch for events
# Look at desktop app console for "session-created" and "activity" logs
```

---

## Introspective Prompts

After each phase, ask yourself:

1. **Did I consider edge cases?**
   - What if OpenCode storage is on a different path?
   - What if files are written atomically (temp file then rename)?
   - What if session is created but no messages yet?

2. **Did I avoid regressions?**
   - Does Claude Code still work correctly?
   - Does Codex still work?
   - Does Gemini still work?

3. **Is the code clean?**
   - Can I refactor common patterns between watchers?
   - Are error messages helpful for debugging?
   - Did I add appropriate logging?

4. **What could go wrong in production?**
   - Memory leaks from file watchers?
   - Race conditions between PTY and watcher?
   - What if storage directory doesn't exist yet?

---

## Acceptance Criteria

- [ ] OpenCode sessions appear on mobile within 2 seconds of creation
- [ ] OpenCode responses appear on mobile (not stuck on "Processing...")
- [ ] Hook output never appears in thinking indicator
- [ ] `waiting_for_input` fires reliably after Claude finishes
- [ ] Claude/Codex/Gemini still work (no regressions)
- [ ] Thinking indicator appears within 200ms of desktop input

---

## Append Your Progress

After completing each task, append to this file:

```markdown
---
## Progress Log

### [Date Time] - Task Name
**Status**: Complete/In Progress/Blocked
**Files Modified**:
**Key Changes**:
**Issues Found**:
**Next Steps**:
**Commit**: `git log -1 --oneline`
```

This creates a running log for handoff or continuation.

---

## Start Here (Follow This Order Exactly)

### Step 0: Git Setup (MANDATORY FIRST STEP)
```bash
cd /home/bigphoot/Desktop/Projects/MobileCLI
git checkout main
git checkout -b fix/rust-backend
git branch --show-current  # Verify: should show fix/rust-backend
```

### Step 1: Orientation (Read First, Don't Edit Yet)
```bash
# Read existing watcher patterns
cat desktop/src-tauri/src/codex_watcher.rs
cat desktop/src-tauri/src/gemini_watcher.rs

# Read thinking detection
sed -n '70,200p' desktop/src-tauri/src/pty.rs

# Read waiting detection
sed -n '206,240p' desktop/src-tauri/src/parser.rs
```

### Step 2: Work Through Issues in Order
1. **Issue 1**: OpenCode watcher (highest impact, unlocks mobile sync)
2. **Issue 2**: Hook thinking exclusion (improves UX)
3. **Issue 3**: Waiting detection fix (fixes stuck state)
4. **Issue 4**: WebSocket reliability (prevents lost events)
5. **Issue 5**: Processing event (faster feedback)

### Step 3: After EACH Issue
```bash
# 1. Test the change
npm run tauri dev  # or cargo check for quick syntax

# 2. Commit your work
git add -A
git status  # Verify only your files are staged
git commit -m "[Rust] Phase N: Description"

# 3. Log your progress (append to this file)
```

### Step 4: Final Verification
```bash
# Show all your commits
git log --oneline main..fix/rust-backend

# Show all files changed
git diff --name-only main...fix/rust-backend

# Full diff for review
git diff main...fix/rust-backend > /tmp/rust-changes.diff
```

---

## Quick Reference

```bash
# Build check (fast)
cd desktop && cargo check

# Dev mode (full)
cd desktop && npm run tauri dev

# Your branch
git checkout fix/rust-backend

# See your commits
git log --oneline -10

# Commit template
git commit -m "[Rust] Phase N: Title

- Change 1
- Change 2
- Files: file1.rs, file2.rs"
```

---

Good luck! Remember:
- **Git first**: Create branch before any edits
- **Commit often**: After each issue, not at the end
- **Stay in lane**: Only touch desktop/src-tauri/src/*.rs files
- The mobile session is handling TypeScript - focus entirely on Rust
