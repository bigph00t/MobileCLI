# Comprehensive Investigation - 2026-01-17

## Context

User has been working on this for 2 days. Two parallel ralph loop sessions made changes that broke the app. User explicitly requested: **"please, patiently and carefully, review the changes that were made. See what the fuck went wrong carefully and patiently."**

Three reported issues:
1. Claude's output no longer showing on mobile
2. Thinking indicator stuck showing "thinking)"
3. Tool call modal required 3 clicks for "yes" to send

Additionally: **Keyboard input not syncing between devices**

---

## Git History Analysis

### Desktop/Rust Changes (main repo)

```
b82f656 Fix thinking) artifact in PTY thinking detection     ← My fix today
a1783af [Rust] Update progress log - all 5 issues complete
c06a41a [Rust] Phase 5: Emit processing started immediately  ← CRITICAL CHANGE
f620ffb [Rust] Phase 4: WebSocket event reliability with replay queue
787e047 [Rust] Phase 3: Fix waiting_for_input suppression by hook output
4ff0c44 [Rust] Phase 2: Fix hook output misclassified as thinking
532351a [Rust] Phase 1: OpenCode file watcher implementation
```

### Mobile Changes (mobile/ repo)

```
5f3e8302 Fix thinking) artifact in PTY thinking detection
7c1159df [Mobile] Phase 6: Delay thinking indicator for desktop prompts
5c309f77 [Mobile] Phase 5: Filter hook content from thinking activities
355ff782 [Mobile] Phase 4: CLI-aware tool approval modal parsing
45b55ad3 [Mobile] Phase 3: Model display fix
adb1b37d [Mobile] Phase 2: CLI-aware activity filtering          ← CRITICAL CHANGE
0656a550 [Mobile] Phase 1: Thinking state hard timeout
```

---

## Issue 1: "thinking)" Artifact

### Root Cause (IDENTIFIED AND FIXED)

**Location**: `desktop/src-tauri/src/pty.rs:206-212`

**Before (broken):**
```rust
let clean_content = if let Some(paren_pos) = thinking_content.find('(') {
    thinking_content[..paren_pos].trim().to_string()
} else {
    thinking_content.trim().to_string()  // ← Did NOT strip trailing )
};
```

**After (fixed):**
```rust
let clean_content = if let Some(paren_pos) = thinking_content.find('(') {
    thinking_content[..paren_pos].trim().to_string()
} else {
    // Strip trailing ) if present (handles "thinking)" from malformed content)
    thinking_content.trim_end_matches(')').trim().to_string()
};
```

**How it happened**: When PTY output contains malformed content like `"thinking)"` (no opening paren but has trailing paren), the code only handled the case where `(` was found. The trailing `)` was left intact.

**Status**: FIXED in commit b82f656

---

## Issue 2: Tool Modal 3-Click Bug

### Root Cause (IDENTIFIED AND FIXED)

**The Chain of Events:**

1. User clicks "Yes" on tool approval modal
2. `handleApproval()` in ToolApprovalModal.tsx sends approval
3. `toolApprovalBlocked[sessionId] = true` to prevent re-showing modal
4. Desktop receives approval, sends to Claude
5. **Rust Phase 5 (c06a41a)**: Immediately emits "Processing..." thinking activity
6. Mobile receives "Processing..." activity
7. **OLD FIX10 code**: Saw ANY activity and unblocked tool approvals
8. But old tool approval was still in activity list
9. With tool approvals unblocked, modal re-appeared
10. Steps 1-9 repeat (total 3 clicks)

**Location**: `mobile/hooks/useSync.ts:2304-2314`

**Before (broken):**
```typescript
// FIX10 unblock was OUTSIDE the isActualContent check
// ANY activity would trigger unblock, including "Processing..." thinking
```

**After (fixed):**
```typescript
if (isActualContent && data.content && data.content.trim()) {
  // FIX10: UNBLOCK tool approvals when we see ACTUAL Claude activity output
  // CRITICAL: Only unblock on actual content, NOT on thinking activities
  const actStoreUnblock = useSyncStore.getState();
  if (actStoreUnblock.toolApprovalBlocked[data.session_id]) {
    logger.log(' FIX10: ACTUAL content detected - UNBLOCKING tool approvals:', data.activity_type);
    useSyncStore.getState().setToolApprovalBlocked(data.session_id, false);
  }
  // ... rest of content handling
}
```

**Key insight**: The Rust Phase 5 change (immediate "Processing..." emission) was GOOD for UX (instant feedback) but it exposed a bug in FIX10's unblock logic.

**Status**: FIXED by moving unblock inside `isActualContent` check

---

## Issue 3: Claude Text Not Showing on Mobile

### Investigation

**Activity Flow:**
```
1. Claude outputs text → PTY captures it
2. pty.rs detect_and_emit_thinking() → emits "activity" Tauri event
3. ws.rs receives Tauri event → broadcasts to WebSocket
4. Mobile useSync.ts receives WebSocket message
5. addActivity() called → FILTERS ARE APPLIED HERE
6. Activity stored in Zustand state
7. ActivityFeed.tsx renders → MORE FILTERS APPLIED HERE
```

### Filter Points (Two Layers)

**Layer 1: useSync.ts addActivity() (lines 377-600)**

For PTY source (`!isJsonlSource`), these filters apply:
- Box-drawing characters (Claude-specific)
- Hook error messages
- Single-character content
- "L |" box artifacts (Claude-specific)
- Prompt echoes `>` or `❯` (Claude-specific)
- "Try" suggestions (Claude-specific)
- Tool approval responses (1, 2, 3, y, n)
- "No recent activity"
- Model info lines (Opus 4.5, Sonnet, etc.)
- Version strings
- Organization/email lines
- Path-only lines (Claude-specific)
- ASCII art/logo characters (Claude-specific)
- "? for shortcuts" (Claude-specific)
- Desktop UI hints (Claude-specific)
- Numbered code lines
- Line numbers only
- "(No content)"
- Python traceback lines
- Import/code lines

**Layer 2: ActivityFeed.tsx .filter() (lines 813-833)**

For text activities:
- `isWelcomeHeaderContent(content, cliType)` → filters welcome/header content
- `isFileContentResult(content)` → filters file read output

### The CLI-Aware Change (Mobile Phase 2)

**Commit adb1b37d** made filters CLI-aware:

```typescript
// Before: All filters applied to all CLIs
// After: Many filters only apply when isClaudeCli === true

const session = state.sessions.find(s => s.id === sessionId);
const cliType = session?.cliType || 'claude'; // Default to claude
const isClaudeCli = cliType === 'claude';
```

**The fix I made in ActivityFeed.tsx:821:**
```typescript
// Pass cliType for CLI-aware filtering
if (isWelcomeHeaderContent(content, cliType)) {
```

**BUT WAIT - This might not be the issue!**

Looking at `isWelcomeHeaderContent` in patterns.ts:254-257:
```typescript
export function isWelcomeHeaderContent(content: string, cliType?: CliType): boolean {
  // ...
  const isClaudeCli = !cliType || cliType === 'claude';
```

When `cliType` is NOT passed (undefined): `!undefined` = `true`, so `isClaudeCli = true`
When `cliType` IS passed as 'claude': `cliType === 'claude'` = `true`, so `isClaudeCli = true`

**BOTH CASES RESULT IN THE SAME BEHAVIOR FOR CLAUDE!**

So passing `cliType` to `isWelcomeHeaderContent` doesn't change anything for Claude sessions - the Claude-specific filters were already being applied.

### Real Root Cause Investigation Needed

If the filtering logic is the same with or without cliType for Claude, then **the fix I made to ActivityFeed.tsx likely didn't fix anything for Claude sessions**.

Possible actual causes:
1. **WebSocket delivery issue** - activities not reaching mobile
2. **Session cliType not being set correctly** - session.cliType might be undefined
3. **JSONL watcher not running** - activities not being emitted at all
4. **Timing/race condition** - activities arriving before subscription

### TODO: Verify These Points

1. Check if sessions have cliType properly set
2. Check WebSocket message delivery
3. Check JSONL watcher initialization
4. Add logging to trace activity flow end-to-end

---

## Issue 4: Keyboard Input Not Syncing

### Investigation

**Input Sync Flow:**
```
Desktop types → input_state event → WebSocket → Mobile receives
Mobile receives → useSync.ts case 'input_state' → setInputState()
```

**Location**: `mobile/hooks/useSync.ts:2391-2436`

**Potential blockers in the code:**

1. **Subscription Grace Period** (lines 2396-2404):
```typescript
const subscriptionTime = sessionSubscriptionTimes[data.session_id];
if (subscriptionTime && inputText.trim()) {
  const timeSinceSubscription = Date.now() - subscriptionTime;
  if (timeSinceSubscription < SUBSCRIPTION_GRACE_PERIOD_MS) {
    logger.log(' Input state IGNORED - within subscription grace period');
    break;  // ← INPUT IGNORED
  }
}
```

2. **Already Sent Check** (lines 2407-2428):
```typescript
const alreadySent = recentUserActivities.some(
  (a: Activity) => a.content.trim() === inputText.trim()
);
if (alreadySent) {
  logger.log(' Input state skipped - matches recent user activity');
  // Sends empty input state
  break;  // ← INPUT IGNORED
}
```

### Hypothesis

The "Already Sent Check" might be too aggressive. If the same text is typed again (common when debugging), it gets filtered out as "already sent".

The "Subscription Grace Period" might also be filtering legitimate input during the early moments after connecting.

### TODO: Check These Values

- What is `SUBSCRIPTION_GRACE_PERIOD_MS`? (Need to find the constant)
- Is the grace period too long?
- Is the "already sent" check matching false positives?

---

## Summary of What Was Actually Fixed vs What Needs More Work

### CONFIRMED FIXED:

1. **"thinking)" artifact** - pty.rs strip trailing paren
2. **3-click tool modal** - FIX10 unblock only on actual content

### NEEDS MORE INVESTIGATION:

3. **Claude text not showing** - My ActivityFeed.tsx fix might be a no-op for Claude. Need to trace actual data flow.

4. **Keyboard input not syncing** - Grace period and "already sent" checks might be too aggressive. Need to examine SUBSCRIPTION_GRACE_PERIOD_MS value and logging.

---

## Files Modified in This Session

### Desktop (committed + pushed):
- `desktop/src-tauri/src/pty.rs` - thinking) fix

### Mobile (merged to master):
- `hooks/useSync.ts` - FIX10 unblock fix
- `components/ActivityFeed.tsx` - cliType parameter (may not help Claude)

---

## Issue 4: Keyboard Input Not Syncing - DETAILED ANALYSIS

### Input Sync Flow

```
Desktop user types
    ↓
Terminal.tsx: emitInputState() (line 122-132)
    ↓
Tauri event: "input-state" with {sessionId, text, cursorPosition}
    ↓
ws.rs: listen("input-state") (line 766)
    ↓
ws.rs: broadcasts ServerMessage::InputState to all WebSocket clients
    ↓
Mobile: WebSocket receives message
    ↓
useSync.ts: case 'input_state' (line 2391)
    ↓
useSync.ts: FILTER 1 - Subscription grace period (5 seconds)
    ↓
useSync.ts: FILTER 2 - Already sent check
    ↓
setInputState() → Zustand state update
    ↓
session/[id].tsx: sessionInputState = inputStates[id]
    ↓
TerminalView: syncedInput prop
    ↓
TerminalView useEffect (line 237-275)
    ↓
FILTER 3 - 2-second local input cooldown
    ↓
FILTER 4 - Already sent check (again!)
    ↓
setInputText() → UI update
```

### Four Defensive Filters (Possibly Too Many!)

**Filter 1: Subscription Grace Period** (useSync.ts:2396-2404)
```typescript
const SUBSCRIPTION_GRACE_PERIOD_MS = 5000; // 5 SECONDS!
const subscriptionTime = sessionSubscriptionTimes[data.session_id];
if (subscriptionTime && inputText.trim()) {
  const timeSinceSubscription = Date.now() - subscriptionTime;
  if (timeSinceSubscription < SUBSCRIPTION_GRACE_PERIOD_MS) {
    logger.log(' Input state IGNORED - within subscription grace period');
    break;  // ← IGNORED FOR 5 SECONDS AFTER SUBSCRIBING
  }
}
```

**Filter 2: Already Sent Check in useSync** (useSync.ts:2407-2428)
```typescript
const alreadySent = recentUserActivities.some(
  (a: Activity) => a.content.trim() === inputText.trim()
);
if (alreadySent) {
  logger.log(' Input state skipped - matches recent user activity');
  // Clears input to empty!
  setInputState(sessionId, { text: '', cursorPosition: 0 });
  break;  // ← IGNORED AND CLEARED
}
```

**Filter 3: 2-Second Local Input Cooldown** (TerminalView.tsx:244-245)
```typescript
const timeSinceLocalInput = Date.now() - lastLocalInputTime.current;
if (timeSinceLocalInput > 2000) {
  // ... allow sync
} else {
  logger.log(' Skipping desktop sync - local activity within 2s');
  // ← IGNORED IF USER TYPED WITHIN 2 SECONDS
}
```

**Filter 4: Already Sent Check in TerminalView** (TerminalView.tsx:253-264)
```typescript
const alreadySent = recentUserActivities.some(
  (a) => a.content.trim() === syncedInput.text.trim()
);
if (alreadySent) {
  logger.log(' Skipping desktop sync - matches recent user activity');
  if (inputText === syncedInput.text) {
    setInputText('');  // ← CLEARS INPUT!
  }
  return;  // ← IGNORED
}
```

### Problems Identified

1. **5-second grace period is too long** - User can't see desktop typing for 5 full seconds after opening a session

2. **Double "already sent" checks** - Same check in both useSync.ts AND TerminalView.tsx is redundant and potentially buggy

3. **Aggressive clearing** - Both checks CLEAR the input state when they think it's stale, which could cause flicker/confusion

4. **No distinction between "stale" and "repeated"** - If user types same text twice (common when debugging), it gets filtered

### Recommended Fixes

1. Reduce `SUBSCRIPTION_GRACE_PERIOD_MS` from 5000ms to 1000ms or 500ms
2. Remove one of the duplicate "already sent" checks
3. Don't clear input state when filtering - just skip the update
4. Add timestamp comparison instead of just content comparison

---

## Fixes Applied (2026-01-17 Session 2)

### Fix 1: Keyboard Auto-Pop (FIXED)
**File**: `mobile/components/TerminalView.tsx:286-294`

**Problem**: Keyboard automatically popped up when opening a conversation.

**Fix**: Commented out the auto-focus useEffect that was calling `inputRef.current?.focus()` on mount.

### Fix 2: Thinking State Hanging (FIXED)
**File**: `mobile/components/ActivityFeed.tsx:814-823`

**Problem**: Thinking indicator was showing "in the middle above Claude's output" and not clearing.

**Fix 2a**: Added filter to exclude non-streaming thinking activities:
```typescript
if (activity.activityType === 'thinking') {
  if (!activity.isStreaming) {
    return false;  // Filter out completed thinking
  }
  return true;
}
```

**File**: `mobile/hooks/useSync.ts:2350-2394`

**Fix 2b**: Fixed JSONL replacement path to also clear streaming thinking activities. The issue was that when JSONL activities replaced PTY activities, the code bypassed `addActivity()` which normally clears thinking. Added explicit thinking cleanup in the JSONL replacement path.

### Fix 3: State Not Auto-Refreshing (ADDRESSED)
The thinking state fixes above should also help with this. When thinking activities are properly cleared, the `waiting_for_input` event will set `waitingState.waitType = 'awaiting_response'` which triggers the "Ready." indicator.

---

## Next Steps

1. Test the fixes on Mac (changes synced)
2. Verify notifications are working
3. Consider reducing the 5-second subscription grace period to 500ms-1000ms
4. Remove duplicate already-sent filtering if input sync issues persist

---

## Relevant Code Locations

| Component | File | Lines | Purpose |
|-----------|------|-------|---------|
| Thinking detection | pty.rs | 70-236 | Detect and emit thinking activities |
| Processing emit | pty.rs | 499-512, 1230-1243 | Phase 5 immediate Processing... |
| Activity filters | useSync.ts | 377-600 | PTY noise filtering |
| Display filters | ActivityFeed.tsx | 813-833 | Welcome/file content filtering |
| Welcome patterns | patterns.ts | 254-340 | isWelcomeHeaderContent |
| Input sync | useSync.ts | 2391-2436 | Input state handling |
| FIX10 unblock | useSync.ts | 2304-2314 | Tool approval unblocking |

---

## Session 3 Fixes (2026-01-17)

### Issue 5: waiting_for_input Not Properly Recognized

**Symptoms**:
- Session list shows "claude is working" when Claude is actually done
- `awaiting_response` state not persisting

**Root Cause**: Two places in useSync.ts were clearing `awaiting_response` state when they should have preserved it (same as they preserve `tool_approval`):

1. **Line 1916-1918**: Assistant message handler cleared `awaiting_response`
2. **Line 2295-2296**: Activity handler cleared `awaiting_response`

Both were checking `waitType !== 'tool_approval'` but NOT `awaiting_response`.

**Fix**: Changed both checks to preserve BOTH `tool_approval` AND `awaiting_response`:

```typescript
// Before:
if (msgWaitingState?.waitType !== 'tool_approval') {
  useSyncStore.getState().setWaitingState(data.session_id, null);
}

// After:
if (msgWaitingState?.waitType !== 'tool_approval' && msgWaitingState?.waitType !== 'awaiting_response') {
  useSyncStore.getState().setWaitingState(data.session_id, null);
}
```

**Files Modified**:
- `mobile/hooks/useSync.ts` lines 1914-1922 (assistant message handler)
- `mobile/hooks/useSync.ts` lines 2293-2303 (activity handler)

### Issue 6: Notifications Not Firing

**Symptoms**:
- Tool approval notification not firing when app is backgrounded
- Awaiting response notification not firing

**Root Cause**: Race condition in notification suppression logic. The check `activeViewingSessionId === sessionId` could return true even when the user was leaving the app, because the AppState change event hadn't fired yet.

**Timeline**:
1. User sends message → `activeViewingSessionId = sessionId`
2. User starts leaving app (swipes to home)
3. Tool approval arrives → notification check: `activeViewingSessionId === sessionId` → TRUE
4. Notification skipped!
5. App state changes to background → `setActiveViewingSession(null)` (too late)

**Fix**: Added AppState.currentState check to ensure notifications fire when app is not active:

```typescript
// Before:
if (activeViewingSessionId === sessionId) {
  return; // Skip notification
}

// After:
const isAppActive = AppState.currentState === 'active';
if (activeViewingSessionId === sessionId && isAppActive) {
  return; // Only skip if BOTH conditions met
}
```

**Files Modified**:
- `mobile/hooks/useNotifications.ts` - Added AppState import
- `mobile/hooks/useNotifications.ts` lines 132-137 (showToolApprovalNotification)
- `mobile/hooks/useNotifications.ts` lines 158-163 (showAwaitingResponseNotification)
- `mobile/hooks/useNotifications.ts` lines 194-207 (showWaitingNotification)
