# Sync Investigation Notes

## Architecture Understanding

### Data Flow (Mobile → Desktop)
```
Mobile sendInput()
  → WebSocket send({ type: 'send_input', session_id, text })
  → ws.rs receives ClientMessage::SendInput
  → Emits Tauri event "send-input"
  → lib.rs listens for "send-input" event
  → input_coordinator processes input
  → mgr.send_input() sends to PTY
  → Also broadcasts "new-message" event for user message
```

### Data Flow (Desktop → Mobile)
```
PTY output
  → parser.rs / jsonl_watcher.rs processes output
  → Emits Tauri events: "new-message", "activity", "pty-output"
  → ws.rs subscribes to these events
  → Broadcasts to all connected WebSocket clients
  → Mobile receives in handleMessage()
  → Updates Zustand store
```

## Key Files

| File | Purpose |
|------|---------|
| `mobile/hooks/useSync.ts` | Mobile sync logic, WebSocket handling, state management |
| `desktop/src-tauri/src/ws.rs` | WebSocket server, message handling, event broadcasting |
| `desktop/src-tauri/src/lib.rs` | Main Tauri app, event handlers, send-input processing |
| `desktop/src-tauri/src/pty.rs` | PTY management, Claude process lifecycle |
| `desktop/src-tauri/src/parser.rs` | CLI output parsing (PTY-based) |
| `desktop/src-tauri/src/jsonl_watcher.rs` | JSONL file watching for structured output |

## Identified Issues

### 1. Input Sync (Mobile → Desktop) - BROKEN
**Symptom**: Messages sent from mobile don't reach Claude

**Investigation Notes**:
- Mobile's `sendInput()` correctly adds local message + activity (lines 1884-1908)
- Sends `send_input` message type over WebSocket (line 1924-1929)
- Desktop receives in ws.rs and emits "send-input" event
- lib.rs handles "send-input" and calls `mgr.send_input()`
- Also broadcasts "new-message" event back to all clients

**Potential Issues**:
- [ ] Check if WebSocket connection is actually established
- [ ] Check if session_id is correct format
- [ ] Check input_coordinator debouncing
- [ ] Check PTY send_input implementation
- [ ] Check if message is being filtered out somewhere

### 2. Input Disappearing on Mobile
**Symptom**: Message sent from mobile disappears from screen

**Investigation Notes**:
- Mobile adds message locally in sendInput() (lines 1891-1897)
- Message added to store via addMessage()
- addMessage has deduplication by ID and content (lines 206-220)

**Potential Issue**:
- When server echoes back the message, it might be replacing or not matching the local one
- Content-based dedup checks `m.role === message.role && m.content === message.content`
- Could be trimming mismatch or timing issue

### 3. Loading Stuck After Claude Finishes
**Symptom**: Loading indicator stays visible after Claude finishes

**Investigation Notes**:
- Streaming thinking activity is added when user sends input (lines 1910-1921)
- Thinking activity should be cleared when "real content arrives" (line 466-470)
- Also cleared on new_message from assistant (lines 1386-1401)

**Potential Issues**:
- [ ] Check if assistant message is being filtered out
- [ ] Check if thinking activity is being properly removed
- [ ] Check waiting state clearing

### 4. Missing Tool Outputs
**Symptom**: Tool output like "done created folder" doesn't appear on mobile

**Investigation Notes**:
- Tool outputs come through as activities with type `tool_result`
- Or as text in `new_message` events
- Mobile has extensive filtering in addActivity (lines 282-441)

**Potential Issues**:
- [ ] Check if tool outputs are being broadcast from desktop
- [ ] Check if tool outputs are being filtered on mobile side
- [ ] Check activity type mapping

### 5. Stuck Status Messages
**Symptom**: Shows "meandering...running stop hooks thinking" instead of just "meandering"

**Investigation Notes**:
- Thinking activities are streaming activities (isStreaming: true)
- Should be replaced when new thinking arrives (lines 454-463)

**Potential Issues**:
- [ ] Multiple thinking activities being created without proper replacement
- [ ] Old thinking not being cleared
- [ ] Concatenation of status messages

### 6. One-Way Sync
**Symptom**: Desktop → Mobile works, but Mobile → Desktop doesn't

This is essentially Issue #1 - the input path from mobile to desktop is broken.

## Key Findings

### The Full Input Path (Mobile → Desktop)
1. User types in `TerminalView.tsx` input field
2. User presses Send button → `handleSend()` called
3. `handleSend()` calls `onSendMessage(text)` which is `sendInput` from useSync
4. `sendInput` in useSync.ts:
   - Clears waiting state
   - Adds local message + activity for immediate UX
   - Adds thinking activity as processing indicator
   - Calls `send({ type: 'send_input', session_id, text, raw })`
5. `send()` sends JSON over WebSocket
6. ws.rs receives `ClientMessage::SendInput`
7. ws.rs emits Tauri event "send-input"
8. lib.rs listens for "send-input" event
9. lib.rs spawns async task → `input_coordinator.submit_input()`
10. If can execute: `mgr.send_input(&sid, &txt).await`
11. PTY writes to Claude's stdin

### The Full Output Path (Desktop → Mobile)
1. Claude writes to PTY stdout
2. parser.rs / jsonl_watcher.rs processes output
3. Emits Tauri events: "new-message", "activity", "jsonl-activity"
4. ws.rs listens for these events
5. ws.rs broadcasts `ServerMessage` to all connected clients via `broadcast_tx.send(msg)`
6. Mobile receives in `handleMessage()` in useSync.ts
7. Updates Zustand store via `addMessage()` / `addActivity()`
8. React re-renders with new state

### Activity Display
- `TerminalView` receives activities as prop
- Passes to `ActivityFeed` component for rendering
- Activities have types: thinking, tool_start, tool_result, text, user_prompt, etc.

## Potential Issues Identified

### Issue A: send_input Message Not Being Queued
In `send()` function (useSync.ts:1815-1846):
- If WebSocket is not open, only "subscribe", "get_messages", "get_activities", "get_sessions" are queued
- **send_input is NOT queued!** It just logs a warning and returns false
- This means if connection drops briefly, input is lost

### Issue B: Deduplication Logic May Filter Out Messages
In `addMessage()` (useSync.ts:200-230):
- Content-based deduplication: `m.role === message.role && m.content === message.content`
- When mobile adds locally AND server echoes back, there might be timing issues
- If echo arrives AFTER local add with different ID, it's deduplicated correctly
- But if there's a race condition, message could be lost

### Issue C: Processing Indicator Not Clearing
In session/[id].tsx (lines 208-264):
- `isProcessing` is tracked based on activities
- If last activity is `user_prompt` and recent (<30s), shows processing
- If thinking activity exists and isStreaming, shows processing
- Issue: If assistant response is filtered out, processing never clears

### Issue D: Activity Filtering May Be Too Aggressive
In `addActivity()` (useSync.ts:280-441):
- MANY filters for PTY activities
- Tool outputs starting with certain patterns are filtered
- If a legitimate assistant message matches a filter pattern, it won't show

### Issue E: Tool Result Outputs May Not Be Synced
Tool results come through as:
1. `activity` event with type `tool_result`
2. `new_message` with role `assistant`

But the extensive filtering in addActivity() may filter out tool outputs.

## Next Steps

1. Add debug logging to trace input path from mobile
2. Check WebSocket connection establishment
3. Verify session_id format between mobile and desktop
4. Check if PTY send actually writes to Claude's stdin
5. Trace tool output path from desktop to mobile
6. **Check if send_input should be queued when WebSocket is closed**
7. **Verify that activity filters aren't too aggressive**
8. **Check processing indicator clearing logic**

## Code Snippets to Remember

### Mobile sendInput function (useSync.ts:1872-1930)
```typescript
const sendInput = useCallback(
  async (sessionId: string, text: string, raw: boolean = false): Promise<boolean> => {
    // Clears waiting state
    setWaitingState(sessionId, null);

    // Adds local message/activity immediately for UX
    if (!raw && !isToolApprovalResponse(text)) {
      addMessage(sessionId, { ... });
      addActivity(sessionId, { ... }); // user_prompt
      addActivity(sessionId, { ... }); // thinking indicator
    }

    return await send({
      type: 'send_input',
      session_id: sessionId,
      text,
      raw,
    });
  }
);
```

### Desktop send-input handler (lib.rs:949-1041)
```rust
app.listen("send-input", move |event| {
    // Parses session_id, text, raw, sender_id
    // Spawns async task
    // Calls mgr.send_raw_input() or mgr.send_input()
    // Emits input-error if failed
});
```

### WebSocket broadcast (ws.rs:1024-1064)
```rust
ClientMessage::SendInput { session_id, text, raw } => {
    // Emits "send-input" for PTY
    app.emit("send-input", ...);

    // Broadcasts "new-message" for other clients
    if !raw {
        app.emit("new-message", ...);
    }

    // Returns ServerMessage::NewMessage
}
```
