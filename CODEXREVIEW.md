# MobileCLI Comprehensive Sync Review (Codex)

## Scope and Goal
This review focuses on **desktop + mobile sync parity**, especially chat display consistency, thinking/progress indicators, tool approvals/results, and multi-device state correctness. Relay networking is only referenced when it affects display or sync semantics. Primary goal: **if it appears on desktop, it must appear on mobile** with correct ordering and state.

## Source Context Used
- `REVIEW_CONTEXT.md`
- `RALPH_CODE_REVIEW.md`
- `RALPH_SYNC_FIX.md`
- `SYNC_INVESTIGATION.md`

## High-Level Architecture (Observed)

### Desktop → Mobile (Primary Display Source)
1. PTY output + JSONL watcher produce events
2. Tauri events emitted: `jsonl-activity`, `activity`, `new-message`, `waiting-for-input`, `pty-output`
3. `ws.rs` broadcasts to WebSocket clients
4. Mobile `useSync.handleMessage()` ingests and merges into Zustand store
5. `TerminalView` + `ActivityFeed` render activities + waiting states

### Mobile → Desktop (Input Path)
1. Mobile `TerminalView.handleSend()` → `useSync.sendInput()`
2. WebSocket `send_input` message → `ws.rs` → `app.emit("send-input")`
3. `lib.rs` listener runs input coordinator → PTY `send_input` / `send_raw_input`
4. PTY writes input to CLI and updates JSONL
5. JSONL watcher emits activity back to mobile

### Primary Data Sources
- **JSONL watcher** for authoritative Claude activities
- **PTY events** for live streaming/thinking and tool approval detection
- **Database** as fallback for non-Claude CLIs or history

## Key UI/Sync Components Reviewed

### Mobile
- `mobile/hooks/useSync.ts` – WebSocket sync, merges, filtering, tool approval, waiting state, local activity creation
- `mobile/app/session/[id].tsx` – Processing state logic for spinner, subscription lifecycle
- `mobile/components/TerminalView.tsx` – Input state, approvals, sync input to desktop
- `mobile/components/ActivityFeed.tsx` – Rendering of activities, tool blocks, thinking indicators
- `mobile/hooks/patterns.ts` – Shared filtering and prompt detection

### Desktop
- `desktop/src-tauri/src/ws.rs` – WebSocket messages and broadcast routing
- `desktop/src-tauri/src/lib.rs` – send-input event handler, input coordinator integration, input-state clear
- `desktop/src-tauri/src/pty.rs` – PTY lifecycle, thinking detection, waiting-for-input detection, JSONL watcher setup
- `desktop/src-tauri/src/jsonl.rs` + `jsonl_watcher.rs` – JSONL parsing + activity emission
- `desktop/src/components/Terminal.tsx` – input-state sync logic and terminal focus
- `desktop/src/components/ChatView.tsx` – history display and resume path

## Primary Sync Risks and Findings

### 1) **Aggressive Filtering Risks on Mobile (Potential Output Loss)**
**Observation**: `useSync.addActivity()` and the DB message-to-activity conversion apply extensive filtering on PTY content and even on DB message content. Many patterns remove lines that can legitimately represent tool output, post hooks, and other assistant responses.

**Risks**:
- Tool outputs like “done created folder” can be filtered as noise (especially if short or patterned).
- Subagent / MCP output can be misclassified as “tool output” or “hook content” and dropped.
- The filter set is large, overlapping with “thinking” and “tool output” markers, which can hide legitimate assistant responses.

**Where**:
- `mobile/hooks/useSync.ts` filtering in `addActivity()` and `messages` handler.
- `mobile/hooks/patterns.ts` `isWelcomeHeaderContent()` may classify some non-header content as header if it contains hook or UI patterns.

**Impact**: Missing assistant output or tool results on mobile, especially around subagent streams and hook messages.

### 2) **Thinking Indicator & Status Mixing**
**Observation**: `pty.rs` emits streaming thinking activities based on heuristics. The mobile client clears “thinking” on `activity` or `new_message`, and also uses `waiting_for_input` to stop processing. There are multiple overlapping clears and replacements.

**Risks**:
- “Thinking” can stick if actual responses are filtered out by content filters.
- “Thinking” can be cleared too early if a duplicate activity is rejected.
- Progress lines that look like thinking may get classified as output and swallowed or remain as the only visible output.

**Specific symptom tie-in**: “thinking swirl hanging after Claude is done” + “subagents showing as thinking” align with the current heuristic approach and filter interactions.

### 3) **Race Between Local Activities and Server Activities**
**Observation**: `useSync` uses aggressive deduplication and replacement logic. It merges DB activities with real-time activities and also replaces PTY streaming activities when JSONL arrives. This is correct in intent but fragile if IDs/content mismatch.

**Risks**:
- Local `user_prompt` activity can be deduped by content and then lost before server echoes back.
- JSONL replacement depends on `toolName` + `activity_type` matching (not UUID in some cases), which can fail for subagents or certain tool outputs.

**Mitigations present**: Recent fixes to preserve local activities and queue `send_input` are in place (from `RALPH_SYNC_FIX.md`). But still fragile due to heuristic merging.

### 4) **Multi-Device Debounce in InputCoordinator**
**Observation**: Input coordinator queues input from different senders within 500ms, using sender ID to gate execution.

**Risks**:
- Rapid interactions across devices can queue input and cause delay or ordering mismatches.
- If queued input is dropped by upstream or not flushed, mobile input may appear “sent” but not executed.

### 5) **Input State Echo / Stale Prompt Risk**
**Observation**: `TerminalView` and `useSync` suppress stale input states with a 2-second local guard and 5-second subscription grace period.

**Risks**:
- Stale input can still appear if delays exceed the grace period.
- Aggressive clearing might remove legitimate pending input on slow connections.

### 6) **History vs Live Feed Divergence**
**Observation**: Mobile uses **activities** as the primary display; messages are converted into activities for history. This is correct, but content filtering of DB messages is stricter than activity filtering and may suppress content differently than live stream.

**Risk**: History rendering not matching live chat output, leading to mismatch between “General chat” and history views.

### 7) **Tool Approval State Blocking**
**Observation**: Tool approval detection has a “block until Claude outputs again” mechanism + 30s timeout fallback. This avoids duplicate modal spam but may suppress legitimate consecutive tool approvals or interleaved tool approvals by subagents.

**Risk**: Approval dialog can fail to appear if blocked after one approval and subsequent tool approvals happen before Claude emits new activity.

### 8) **PTY vs JSONL Response Cohesion**
**Observation**: JSONL activities are authoritative for Claude; PTY is still used for tool approvals, waiting_for_input, and streaming thinking. This is correct but fragile if the JSONL watcher misses entries or delays appear.

**Risk**: When JSONL lags, the system depends on PTY output + filtering which can be inconsistent. This can explain delayed output or missing output on mobile while desktop is already updated.

## Symptom Mapping to Likely Causes

### “Mobile input not displaying in chat after being sent”
Likely contributors:
- Local add succeeded but was later removed by dedup/merge logic in `activities` handler.
- `send_input` message was dropped on WS disconnect (previous bug; now queued). If still happening, connection status or queue flush issues could be at fault.

### “Delays in output syncing from desktop to mobile”
Likely contributors:
- JSONL watcher emits only after file update/flush; delay can be due to buffering or file watcher polling.
- Activity filtering removes interim PTY outputs so the user sees “nothing” until JSONL arrives.

### “Thinking swirl hanging after Claude done”
Likely contributors:
- Real content filtered, so thinking never cleared.
- `waiting_for_input` might not arrive if parser doesn’t detect prompt (see `OutputParser.check_waiting_for_input` patterns).

### “Subagents outputs swallowed by thinking”
Likely contributors:
- PTY thinking detection classifies status lines as thinking and `addActivity` replaces other thinking; subagent tool outputs might be classified as thinking or filtered as tool outputs depending on prefix.

### “Tool outputs missing or truncated”
Likely contributors:
- Filtering logic in `addActivity` suppresses content like `(No content)`, “PostToolUse hooks”, or tool result lines that look like code.
- Tool result merging relies on `toolName`; if toolName missing (fixed in JSONL), some tool results may still be merged incorrectly or discarded.

## Detailed Review Notes (Key Hotspots)

### Mobile: `useSync.ts`
- **Strengths**: JSONL activities bypass PTY filters; local message/activities added optimistically; waiting state and modal logic are robust.
- **Risk**: Filters are broad and may drop valid assistant output. Many patterns are tuned to Claude Code but can match legitimate text or subagent output. Dedup logic is complex and may be too aggressive when multi-device edits are involved.
- **Critical path**: `addActivity()` and `messages` handler.

### Mobile: `SessionScreen` processing logic
- Uses waiting state, thinking activity, and last activity type to infer “processing”.
- Has fallback timers (15s for thinking, 10s for user_prompt) to clear stuck state.
- If actual content gets filtered, processing state may linger or drop incorrectly.

### Mobile: `TerminalView`
- Clears input immediately and syncs empty input to server to prevent stale prompt.
- Tool approval modal is controlled via waiting state with `userRespondedToApproval` local override.
- If `waiting_for_input` is misclassified, modal can flicker or not appear.

### Desktop: `ws.rs` + `lib.rs`
- Proper broadcast of `activity`, `jsonl-activity`, `waiting_for_input`, `new-message` events.
- `ClientMessage::SendInput` emits `new-message` for user and triggers PTY write.
- Input-state is relayed to mobile on subscribe and on typing changes.

### Desktop: `pty.rs`
- Thinking detection emits streaming activities based on heuristics and spinner detection.
- Tool approval detection relies on prompt content and waiting-for-input detection.
- JSONL watcher is created at session start; conversation ID is generated upfront.

### Desktop: `parser.rs`
- Prompt detection is heuristic and uses pattern lists; prompt content includes recent buffer.
- If prompt detection fails, waiting state won’t emit and mobile may keep thinking active.

## Multi-Device / Multi-Session Concerns

### Debounce behavior
- Inputs from different devices within 500ms may be delayed. This is correct to avoid clobbering but can appear as lag or missing input during rapid multi-device usage.

### Shared WS state in mobile
- `useSync` maintains global WebSocket with `globalPendingMessages`. This is efficient but could lead to ordering issues if multiple sessions are active and reconnect occurs mid-send.

### Session subscribe timing
- `subscribeToSession` is only called on focus and only once per session ID. If a session changes status while the screen is open and WS reconnects, re-subscription might not fire (depending on connection logic). This can lead to missing updates for open sessions after reconnect.

## Key Areas Needing Follow-Up Testing
1. **Local activity retention** under rapid reconnection + get_activities merge.
2. **Tool output visibility** under long tool results and post hooks.
3. **Subagent output display** (Task tool output with subagent names, long output sequences).
4. **Thinking indicator transitions** for long-running tool sessions and prompt-driven approvals.
5. **Multi-device input ordering** (mobile + desktop within <500ms).

## Recommendations (Non-Code for Review)

### Priority A: Guardrails for Filtering
- Add structured logging/telemetry to log filtered content (specifically which filter dropped a line). This will pinpoint which pattern is suppressing valid output.
- Consider a “debug mode” toggle to bypass filters entirely for QA sessions.

### Priority B: JSONL / Activity Ordering
- Confirm JSONL watcher latency and ensure PTY activities are not filtered too aggressively while JSONL hasn’t yet arrived.
- Consider queueing PTY activities longer and pruning once JSONL arrives to avoid “gap” periods.

### Priority C: Subagent Output Fidelity
- Ensure `toolName` and `toolParams` are preserved for Task/MCP tools from JSONL output and render logic recognizes them consistently.
- Validate `parseTaskParams` handles all subagent types and does not mislabel or drop content.

### Priority D: Waiting/Thinking State Harmonization
- Verify `waiting_for_input` emission in `parser.rs` across all CLI types and ensure that `thinking` is cleared on actual content events consistently.
- Ensure `toolApprovalBlocked` does not suppress sequential approvals from subagents.

## Proposed Code Change Plan (Do Not Implement Yet)

### 1) Replace filter logic with a structured “allowlist + trace” pipeline
**Goal**: Prevent missing real output by making filtering deterministic, auditable, and reversible.

**Suggested implementation**:
- Introduce a `FilterDecision` struct in `mobile/hooks/useSync.ts` that returns `{ action: 'keep' | 'drop', reason: string, source: 'pty' | 'jsonl' }` for each filter.
- Centralize PTY filters into `mobile/hooks/patterns.ts` with explicit unit tests, rather than inline logic in `addActivity()`.
- Add a dev-only `SYNC_DEBUG` flag that appends dropped content into a debug list in Zustand (hidden UI panel).

**Files**: `mobile/hooks/useSync.ts`, `mobile/hooks/patterns.ts`, `mobile/utils/debugSync.ts` (new helper).

### 2) Add a dedicated “unfiltered activity shadow list” for QA sessions
**Goal**: Ensure we can compare raw PTY/JSONL activity to what UI renders.

**Suggested implementation**:
- Store a parallel `rawActivities` array per session in Zustand (only in debug mode).
- In ActivityFeed, add a toggle to show raw vs filtered for QA sessions.

**Files**: `mobile/hooks/useSync.ts`, `mobile/components/ActivityFeed.tsx`.

### 3) Reduce false positives in `isWelcomeHeaderContent` and PTY filters
**Goal**: Stop legitimate text from being classified as header/tool output.

**Suggested implementation**:
- Remove `HOOK_PATTERN` from header filtering, and treat hook output as visible activity unless explicitly tagged as system/hook metadata in JSONL.
- Loosen “box drawing” filter by requiring >80% box chars and minimum line length.
- Stop filtering `(No content)` entirely when it appears inside a tool_result (it is still a valid tool response).

**Files**: `mobile/hooks/patterns.ts`, `mobile/hooks/useSync.ts` (PTY filter branches).

### 4) Strengthen JSONL/PTY merge consistency
**Goal**: Avoid missing tool/subagent outputs when JSONL events arrive late or with missing tool_name.

**Suggested implementation**:
- When JSONL `uuid` arrives, replace matching PTY by `uuid` if present; if missing, fall back to tool_name + content prefix match.
- Keep PTY activities for a short “grace period” even after JSONL arrives and only prune after JSONL confirms content.

**Files**: `mobile/hooks/useSync.ts`.

### 5) Explicitly track “thinking lifecycle” per session
**Goal**: Avoid stuck or swallowed thinking indicators.

**Suggested implementation**:
- Add `thinkingState: { id, startedAt, source }` into Zustand and explicitly clear it on any `text/tool_result` or `waiting_for_input` event.
- Disallow PTY thinking replacement if a JSONL activity was just received within the last N ms (prevents churn).

**Files**: `mobile/hooks/useSync.ts`, `mobile/app/session/[id].tsx`.

### 6) Improve subagent / MCP tool rendering fidelity
**Goal**: Show subagent output as tool blocks instead of thinking or text blocks.

**Suggested implementation**:
- In `jsonl.rs`, annotate Task tool activities with `toolParams` that include `subagent_type` for consistent rendering.
- In `ActivityFeed`, enhance `parseTaskParams` to detect `subagent_type` across more MCP patterns.

**Files**: `desktop/src-tauri/src/jsonl.rs`, `mobile/components/ActivityFeed.tsx`.

### 7) Make waiting/approval state transitions deterministic
**Goal**: Prevent tool approval modals from being suppressed or stuck.

**Suggested implementation**:
- Track a per-session `approvalSequenceId` that increments on each detected approval. Use it to prevent accidental suppression by `toolApprovalBlocked`.
- Clear `toolApprovalBlocked` on `waiting_for_input` when new approval content differs by signature, not only when activities arrive.

**Files**: `mobile/hooks/useSync.ts`.

### 8) Multi-device input ordering & visibility validation
**Goal**: Ensure mobile input shows immediately and stays visible across merges.

**Suggested implementation**:
- Add a unique `client_msg_id` in mobile send_input messages; echo it back in `ws.rs` so mobile can reconcile local activities with server activity reliably.
- Skip content-based dedup for user messages when a matching `client_msg_id` exists.

**Files**: `mobile/hooks/useSync.ts`, `desktop/src-tauri/src/ws.rs`.

### Most critical file flows
- `mobile/hooks/useSync.ts`: `sendInput` → `send` → WS; `handleMessage` → `addActivity`/`setWaitingState`/`setInputState`.
- `desktop/src-tauri/src/ws.rs`: `ClientMessage::SendInput` → Tauri `send-input` event; `jsonl-activity`/`activity` broadcast.
- `desktop/src-tauri/src/pty.rs`: PTY processing and `waiting-for-input` emit; JSONL watcher setup.
- `desktop/src-tauri/src/jsonl_watcher.rs`: JSONL activity emission.

## Conclusion Summary
The sync stack is mature but **fragile** due to heavy filtering and heuristic-based merging. Most user-reported problems map to the interaction between:
- PTY activity heuristics (thinking/progress),
- JSONL timing,
- Mobile filtering/dedup,
- Waiting state transitions.

The fixes noted in `RALPH_SYNC_FIX.md` address several key gaps, but the remaining risk is mostly **visibility loss**, not transport loss. The system appears able to transmit messages, yet the **mobile display layer discards or suppresses them** in edge cases. The highest leverage next step is to instrument filtering and ensure mobile parity for tool/subagent output, even at the cost of temporarily showing more “noise.”

---

## Pending: Suggested QA Checklist (Manual)
1. Send a message from mobile while desktop typing (within 500ms) and verify order.
2. Long-running tool call with streaming thinking → ensure spinner clears when tool result arrives.
3. Subagent Task tool: verify it renders as tool block with subagent name, not as thinking.
4. Tool approval followed by immediate second approval – ensure modal appears again.
5. Open old session history – compare desktop history and mobile ActivityFeed for parity.
6. Toggle network (disconnect/reconnect) while sending input – ensure queued message flushes and appears.
