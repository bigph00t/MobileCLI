# MobileCLI Thinking/Loading Investigation (Fix Proposals)

This report follows the requested per-file format and focuses on the highest priority: why thinking/loading can hang, then hook output misclassification, delay before thinking appears, and display corruption. It also includes cross-file observations where the behavior is determined by interaction between mobile and desktop.

---

## File: mobile/hooks/relayStore.ts

### Current Behavior
Relay persistence for encrypted relay mode. Holds relay config and connection state; no thinking logic.

### Issues Found
- No direct thinking/loading logic here; the file appears in the original checklist but is not relevant to thinking state.

### Relevant Code Snippets
- `useRelayStore` state shape and load: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/relayStore.ts:35`
- `loadRelayConfig` sets `isLoaded`: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/relayStore.ts:103`

### Suggested Investigation Points
- Confirm checklist accuracy: thinking state is not managed here. If relay behavior affects “loading hanging,” it is only via delayed WebSocket delivery (see `useSync.ts`).

---

## File: mobile/hooks/patterns.ts

### Current Behavior
Centralized pattern matching for filtering CLI output, tool approvals, trust prompts, and welcome headers. HOOK_PATTERN filters hook-related lines from display only via `isWelcomeHeaderContent`.

### Issues Found
- Hook output filtering is only used for “welcome header” filtering, not for thinking detection. This means hook lines can still be classified as thinking elsewhere (desktop PTY detection).
- `HOOK_PATTERN` is not used in any thinking detection path, so hook lines like "Running stop hooks... 2/6" can still be emitted as thinking from PTY.

### Relevant Code Snippets
- `HOOK_PATTERN` definition: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/patterns.ts:191`
- `isWelcomeHeaderContent` uses `HOOK_PATTERN` to filter display: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/patterns.ts:272`

### Suggested Investigation Points
- If hook lines should never be “thinking,” consider adding explicit hook exclusion in the desktop thinking detector (pty.rs) rather than only filtering display on mobile.
- Consider adding a dedicated `isHookOutput` helper and use it in any incoming “thinking” activity filtering, so hook output can’t show as thinking even if emitted.

---

## File: mobile/components/TerminalView.tsx

### Current Behavior
Renders ActivityFeed, tool approval card, input, and an “awaiting response” indicator. The awaiting indicator is suppressed if a streaming thinking activity exists.

### Issues Found
- If a streaming thinking activity is stuck, the awaiting indicator never appears, hiding the “ready for input” state. This amplifies the perception of “hanging.”
- No sanitization of thinking content here, so corrupt thinking text flows directly from activities.

### Relevant Code Snippets
- Awaiting indicator suppression when any streaming thinking exists: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/components/TerminalView.tsx:55`
- Tool approval card tied to `waitingState`: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/components/TerminalView.tsx:44`

### Suggested Investigation Points
- When thinking state is cleared in `useSync`, ensure the ActivityFeed also removes the thinking activity, or the UI will continue to block the awaiting indicator.
- If thinking text corruption is a known issue, consider sanitizing in ActivityFeed or in the activity ingestion pipeline (see `useSync.ts`).

---

## File: mobile/hooks/useSync.ts

### Current Behavior
This is the primary mobile state pipeline. It creates local “Processing…” thinking on `sendInput`, ingests `activity`, `waiting_for_input`, `pty_output`, and `new_message` events, and attempts to clear thinking based on real content or timeouts.

### Issues Found
- **Stuck thinking (highest priority):**
  - Local “Processing…” is created immediately on `sendInput`. It is only cleared when:
    - A non-thinking activity is added (`addActivity` for non-thinking), or
    - A `waiting_for_input` event arrives, or
    - The `scheduleThinkingAutoClear` is scheduled (only when `addActivity` receives a non-thinking activity).
  - If the desktop never emits activities or the incoming activities are filtered out as duplicates/noise, the local thinking remains.
  - `scheduleThinkingAutoClear` is only invoked when a non-thinking activity is added; it doesn’t fire if the only outputs are filtered or missing.

- **Hook output misclassified as thinking:**
  - Mobile does not detect thinking from raw PTY output, but relies on desktop “thinking” activities. When hook output is misdetected as thinking on desktop, it is shown on mobile.
  - Filtering in `addActivity` does not exclude hook lines from “thinking” activities.

- **Delayed thinking indicator:**
  - For prompts sent from desktop, the mobile client does not add local “Processing…” because `sendInput` is not invoked. It waits for PTY thinking events from desktop (which are delayed until the CLI prints status).

- **Display corruption:**
  - Mobile never sanitizes `thinking` content; it displays whatever the desktop emits. Partial chunks or malformed status lines can become activities.

### Relevant Code Snippets
- Local “Processing…” thinking activity creation in `sendInput`: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:2494`
- Thinking auto-clear scheduling only triggered on non-thinking activity: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:633`
- Thinking state cleared on waiting-for-input: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:2039`
- JSONL thinking activities are discarded: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:68`
- Tool approval detection in activity/pty_output: `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:1944`

### Suggested Investigation Points
- Consider clearing local “Processing…” after a timeout even if no non-thinking activity is added (move auto-clear to `sendInput` or schedule immediately on send).
- Consider clearing thinking when `waiting_for_input` arrives even if it is not matched to `tool_approval` (already done), but ensure the event is reliably emitted (see `parser.rs`).
- Add a guard to drop “thinking” activities that contain hook output signatures to prevent contamination.
- Add a fallback timer when the user sends input (e.g., if no activity arrives within N seconds, clear thinking and show awaiting-response or reconnect UI).

---

## File: desktop/src-tauri/src/ws.rs

### Current Behavior
Broadcasts PTY output, JSONL activities, and `waiting-for-input` events to mobile clients. It does not parse or classify thinking itself; it forwards activities emitted by PTY and JSONL watchers.

### Issues Found
- No direct stuck-loading logic; however, the broadcast strategy means mobile depends on upstream activity emission. If PTY/JSONL activity emission is delayed, mobile’s thinking state is delayed.

### Relevant Code Snippets
- PTY output broadcast: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/ws.rs:61`
- Activity broadcast: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/ws.rs:623`
- JSONL activity broadcast: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/ws.rs:668`
- Waiting-for-input broadcast: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/ws.rs:502`

### Suggested Investigation Points
- Confirm that `activity` events are emitted for all CLIs (including Codex) when real content arrives. Missing activity events are the most likely reason for stuck thinking on mobile.

---

## File: desktop/src-tauri/src/lib.rs

### Current Behavior
App entry point and commands. Emits `new-message`, `waiting-cleared`, and session events. It doesn’t classify thinking; it sends events from PTY and JSONL handlers.

### Issues Found
- No direct thinking detection here. However, tool approval responses can clear waiting state (`waiting-cleared`), which can suppress modal UI and potentially shift the system into an “awaiting” state while thinking remains.

### Relevant Code Snippets
- `send_raw_input` emits `waiting-cleared`: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/lib.rs:279`
- `send_tool_approval` emits `waiting-cleared`: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/lib.rs:343`

### Suggested Investigation Points
- Ensure that `waiting-cleared` is followed by either activity or waiting-for-input events to keep mobile state consistent.

---

## File: desktop/src-tauri/src/pty.rs

### Current Behavior
Detects thinking/progress lines from cleaned PTY output and emits them as streaming `thinking` activities. It considers “thinking words,” lines ending in “…”, “thinking” substring, and spinner prefixes.

### Issues Found
- **Hook output misclassified as thinking:**
  - Any line ending in `...` and under 100 chars is treated as thinking if it lacks special chars. This matches hook output such as:
    - `running stop hooks... 2/6`
    - `0/6 done` (if emitted in a spinner line)
  - The detector also treats lines with “thinking”, “thought for”, or “esc to interrupt” as thinking. Hook output can contain such fragments depending on output formatting.

- **Display corruption:**
  - The detector runs line-by-line on raw PTY chunks, which can contain partial fragments. If a chunk includes broken strings (e.g., a mid-word split), the detector will emit those fragments. This can result in “bliruning”-style corruption.
  - It only strips parenthetical content if `(` is present, leaving suffixes like `thinking)` intact if `(` was lost or trimmed before this pass.

### Relevant Code Snippets
- Thinking detection and emission: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:70`
- `...`-ending heuristic: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:141`
- “thinking”/“esc to interrupt” heuristic: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:156`
- `clean_content` parenthetical trimming: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:181`

### Suggested Investigation Points
- Add explicit exclusions for hook output lines (`hook`, `hooks`, `PostToolUse`, `stop hooks`, `Ran X hooks`) before `is_thinking` becomes true.
- Add a “stability” check (e.g., require a full line ending with newline) or a minimum of alphabetic characters to avoid partial fragment emissions.
- Strip trailing `)` when matching “thinking” or status words without `(` present to avoid `thinking)` artifacts.

---

## File: desktop/src-tauri/src/parser.rs

### Current Behavior
Parses PTY output to detect “waiting for input” state and manages response buffering. It uses CLI-specific thinking patterns to avoid emitting `waiting-for-input` while Claude is still thinking.

### Issues Found
- **Stuck loading (priority 1):**
  - `check_waiting_for_input` refuses to emit a waiting event if the current chunk includes any “thinking” pattern. If the PTY chunk includes old “thinking” text (or a hook line that contains “thinking” or “esc to interrupt”), the `waiting-for-input` event is suppressed. Mobile then never receives the event to clear thinking, leaving a spinner stuck.

- **Delay before thinking appears (priority 3):**
  - For desktop-sent prompts, mobile receives a “thinking” activity only after PTY status text is printed. If that status text is delayed (which is common), the mobile UI appears idle for 1–2 seconds.

### Relevant Code Snippets
- Thinking patterns used for “still thinking” detection: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/parser.rs:104`
- `check_waiting_for_input` uses `is_still_thinking` to suppress waiting: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/parser.rs:240`

### Suggested Investigation Points
- Consider checking “still thinking” only on fresh, *current* status lines (not the entire chunk), or by requiring the line to match a known thinking status format instead of substring matches.
- Consider emitting `waiting-for-input` even if a stale thinking substring appears, when a prompt is detected and response buffer is non-empty.

---

## File: desktop/src-tauri/src/jsonl.rs

### Current Behavior
Parses Claude JSONL content blocks, including `thinking` blocks, and emits activities. JSONL thinking is present but mobile filters it.

### Issues Found
- Mobile filters JSONL thinking entirely (in `useSync.ts`), so thinking only appears from PTY detection. This ties the mobile thinking indicator to PTY heuristics (and their errors), not structured JSONL output.

### Relevant Code Snippets
- JSONL `thinking` block extraction: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/jsonl.rs:337`

### Suggested Investigation Points
- Consider emitting a short JSONL “thinking” signal as a structured activity (even if content is hidden), then letting mobile show an indicator without untrusted PTY heuristics.

---

# Cross-File Root Causes and Fix Proposals

## 1) Stuck Loading (Highest Priority)
**Likely root causes:**
- Mobile adds a local “Processing…” thinking activity on send, but clears it only when non-thinking activities arrive or `waiting_for_input` fires.
- PTY filtering or parser suppression can prevent both activity emission and `waiting_for_input`, leaving the local thinking activity stuck.

**Proposals:**
- Add a hard timeout after `sendInput` (e.g., 3–5 seconds) to clear local thinking and optionally show a “still working” or reconnect hint.
- Ensure `waiting-for-input` is emitted even if `thinking` patterns appear in the same chunk, when a prompt is detected and the parser is in `WaitingForAssistant` or `AssistantResponding`.
- Clear local thinking whenever a `waiting-cleared` or `waiting-for-input` event arrives, regardless of tool approval state.

## 2) Hook Output Misclassified as Thinking
**Likely root causes:**
- Desktop PTY detector treats any `...` line as thinking, which includes hook progress lines.

**Proposals:**
- Add hook keyword exclusions in `detect_and_emit_thinking` before the `...` or spinner heuristics.
- Only treat `...` lines as thinking if they start with a known thinking word or a spinner prefix.

## 3) Delay Before Thinking Appears
**Likely root causes:**
- For desktop-originated prompts, mobile does not create local thinking, so it waits for PTY thinking emission. PTY status can arrive 1–2 seconds later.

**Proposals:**
- Emit a minimal `thinking` activity immediately on the desktop when user input is sent (similar to mobile’s local “Processing…”), then replace it with actual thinking text when PTY/JSONL arrives.
- Alternatively, send a “processing started” event on user input and let mobile show a generic indicator until real thinking arrives.

## 4) Display Corruption
**Likely root causes:**
- PTY thinking detection runs on partial chunks, emitting fragment strings.
- Parenthetical trimming only handles `(` but not trailing `)` without `(`.

**Proposals:**
- Buffer PTY lines until a newline boundary before evaluating for thinking.
- Require a minimum ratio of alphabetic characters and/or require a known thinking prefix or spinner symbol.
- Sanitize emitted thinking text (strip trailing punctuation artifacts like `)` when `(` is absent).

---

# Additional Verification Steps (Suggested)
- Add logging around the “local thinking” lifecycle in `useSync.ts` to log when it is created and cleared, including the event that triggered the clear.
- Capture PTY output around hook execution to confirm which line is being matched as thinking.
- Record whether `waiting-for-input` is suppressed by `is_still_thinking` in `parser.rs` when hook output contains thinking-like substrings.

---

# Deeper, More Specific Fix Proposals (Actionable)

## 5) Make Thinking Clear Even When No Activities Arrive
**Problem detail:** `sendInput` creates a local streaming `thinking` activity, but it is only cleared when a non-thinking activity is added or when a `waiting_for_input` event arrives. If the upstream activity emission fails (e.g., OpenCode has no JSONL watcher, or PTY events are filtered), the spinner persists indefinitely.

**Concrete changes:**
- Schedule an immediate auto-clear at the time of `sendInput`, not only after a non-thinking activity arrives. Use the same `scheduleThinkingAutoClear` mechanism but call it in `sendInput` right after adding the local `thinking` activity.
- Add a “safety” clear when `waiting_cleared` arrives (this already clears waiting state but does not always clear thinking activities). If `waiting_cleared` arrives without a subsequent activity, the spinner can persist.
- Add a secondary long timeout (e.g., 10–15s) after `sendInput` that clears thinking and shows a reconnect hint if no activity or waiting state appears.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:2494` (local “Processing…” creation)
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:68` (thinking state handling)

---

## 6) Prevent Hook Output from Ever Becoming Thinking
**Problem detail:** hook progress lines match the `...` heuristic in `detect_and_emit_thinking` and are emitted as thinking content, which then appears in mobile. This is the main reason for hook output being misclassified.

**Concrete changes:**
- Add a fast negative filter before `is_thinking` becomes true:
  - Reject lines containing `hook`, `hooks`, `posttooluse`, `stop hooks`, `ran X hooks`, `hook error`, or common hook counters like `\d+/\d+`.
- Tighten the `...` heuristic: require a spinner prefix or a known “thinking word” **and** a minimum alphabetic ratio.
- Require a full line boundary for thinking detection (buffer until a newline) to avoid partial fragments.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:70`

---

## 7) Reduce Delay Before Thinking Appears for Desktop-Originated Prompts
**Problem detail:** When input is sent from desktop, mobile doesn’t insert a local “Processing…” activity. It waits for PTY status text, which often arrives 1–2 seconds later.

**Concrete changes:**
- Emit a synthetic “processing started” activity on the desktop immediately after user input is written to PTY. This is equivalent to the local mobile indicator and bridges the delay.
- Alternatively, emit a `waiting_state` transition to a new state (e.g., `processing`) and let mobile show a generic spinner until the first activity arrives.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:829` (`send_input` already knows when user input is sent)

---

# OpenCode + Other CLI Sync Issues (Deep Dive)

## Findings from Local CLI Testing
- `opencode` is installed at `~/.opencode/bin/opencode`.
- `opencode run --format json "Hello from MobileCLI test"` outputs structured NDJSON events:
  - `type: step_start`, `type: text`, `type: step_finish`.
- This output is **not** used in the current MobileCLI PTY flow, which spawns `opencode [project]` (TUI) and relies on JSONL watchers or PTY parsing.

## Why OpenCode Responses Don’t Show on Mobile
**Root cause #1: No JSONL watcher for OpenCode.**
- `JsonlWatcher` exists for Claude, Codex, Gemini only. There is no OpenCode watcher, so no `jsonl-activity` events are emitted.

**Root cause #2: PTY message parsing was disabled after JSONL redesign.**
- In `pty.rs`, the `OutputParser` is used only for `waiting_for_input`, not to emit text/tool activities for non-Claude CLIs. The code explicitly states that `parse_activities()` and `extract_message()` are no longer called.
- This means OpenCode’s PTY output never becomes `activity` or `new_message` events, so mobile never gets “text” content. The only PTY-derived activity is “thinking” from `detect_and_emit_thinking`, which is why the mobile UI gets stuck in loading.

**Root cause #3: Prompt and marker mismatch.**
- `parser.rs` uses Claude/Gemini markers (`●`, `⎿`, `▶`) that may not match OpenCode’s TUI output. This breaks both response extraction and `waiting_for_input` detection.

### Recommended OpenCode-Specific Fixes
**Option A: Use OpenCode JSON output for structured events (preferred).**
- Use `opencode run --format json` to produce NDJSON events and map them to MobileCLI activities (`step_start` → `tool_start`, `text` → `text`, `step_finish` → `tool_result`).
- This gives a stable, structured data stream and avoids brittle PTY parsing.
- OpenCode docs explicitly support `--format json` and `opencode serve`/`attach` for API-style use. See: https://opencode.ai/docs/cli/

**Option B: Re-enable PTY parsing for non-JSONL CLIs.**
- For OpenCode (and any CLI without a JSONL watcher), re-enable `OutputParser::extract_message()` or a simplified PTY → activity conversion.
- Condition this on `cli_type == OpenCode` so it does not interfere with Claude’s JSONL-first design.
- Emit `activity` and `new-message` events so mobile gets the assistant response and clears thinking.

**Option C: “Raw terminal mirror” fallback.**
- If structured parsing is unreliable, add a fallback mode where PTY lines are emitted as `text` activities (minimal filtering) for OpenCode sessions.
- This satisfies the requirement in `cli.md` to mirror the CLI output closely and avoids losing responses entirely.

### Target Locations (OpenCode Flow)
- Spawn command for OpenCode TUI: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:298`
- Resume command for OpenCode: `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:1016`
- PTY parsing suppression (“no parse_activities/extract_message”): `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:606`
- OpenCode CLI docs for JSON output and server mode: https://opencode.ai/docs/cli/

---

# Additional CLI Coverage Gaps (Codex/Gemini/OpenCode)

## Codex
- Codex has a watcher (`codex_watcher`) and JSONL parsing, but PTY “thinking” detection is Claude-specific and may misclassify Codex’s progress output.
- Ensure Codex-specific thinking words are updated in `parser.rs` and consider a Codex-specific PTY heuristic (if JSONL is missing or delayed).

## Gemini
- Gemini has a JSON watcher (`gemini_watcher`), but waiting-for-input detection uses Claude-style prompts. If Gemini uses a different prompt marker in newer versions, `waiting_for_input` could still fail, leaving stale thinking on mobile.

## OpenCode
- No watcher exists, and JSONL is not part of OpenCode’s CLI output.
- OpenCode has a structured JSON mode (`opencode run --format json`) that is better suited for mobile sync than parsing the TUI.

---

# Proposed Next Experiments (Optional)
- Capture raw PTY output for OpenCode TUI and save it to a log file to identify prompt markers and output structure.
- Implement a temporary “debug capture” that emits `pty_output` lines as activities for OpenCode only (verify mobile display works end-to-end).
- Prototype an OpenCode JSON mode integration by running `opencode run --format json` and mapping the event stream to activities.

---

# Addendum: Review of `OtherCLIS.md` + Additional Findings

## Summary of New Evidence
`OtherCLIS.md` correctly points out that OpenCode lacks a watcher and therefore relies exclusively on PTY parsing. The critical detail discovered during additional investigation is that OpenCode **does** have a structured storage format under `~/.local/share/opencode/` with JSON files for sessions, messages, and parts. This can be used to implement a first‑class watcher similar to `JsonlWatcher`, without needing terminal parsing.

### Newly Verified OpenCode Storage Locations (Local)
- Session metadata:
  - `~/.local/share/opencode/storage/session/<project_hash>/ses_*.json`
- Message metadata:
  - `~/.local/share/opencode/storage/message/<session_id>/msg_*.json`
- Message parts (actual assistant/user text):
  - `~/.local/share/opencode/storage/part/msg_<message_id>/prt_*.json`
- Example (from local test run):
  - Session: `/home/bigphoot/.local/share/opencode/storage/session/895979672f5f2c6c10f5b424415f6220ab3c08c9/ses_432ebb918ffeXO5bSjbUgqj8j0.json`
  - Message meta (assistant): `/home/bigphoot/.local/share/opencode/storage/message/ses_432ebb918ffeXO5bSjbUgqj8j0/msg_bcd14470c001UV1RKxkZXktGnC.json`
  - Part text (example from another session): `/home/bigphoot/.local/share/opencode/storage/part/msg_bc8d62cfa001fxPkM9XBD4lbZy/prt_bc8d62cfb001HKhLN95dYD0O88.json`

### Why This Matters
The lack of OpenCode watcher support is **not** a terminal‑format problem; it’s a data ingestion gap. We can parse OpenCode’s storage files directly, just like JSONL for Claude. This should make OpenCode session sync robust and remove the “Processing…” hang caused by missing activities.

---

# OpenCode Watcher Proposal (Data‑Driven, Not PTY)

## 1) Watch Session Files (Session List Sync)
**Problem:** OpenCode sessions sometimes do not appear on mobile until restart. This may be because session creation is not being detected promptly or because relay timing drops the event.

**Fix:** Add a watcher for `~/.local/share/opencode/storage/session/**/ses_*.json` and emit `session-created` or `session-updated` events when new session files appear.

**Notes:**
- Each session file includes `directory` and `title`. These map directly to session name and project path.
- Use `projectID`/hash to group sessions.
- For multi-project scenarios, the watcher should monitor all session directories, not just the active project.

## 2) Watch Message/Part Files (Message + Activity Sync)
**Problem:** OpenCode responses are missing on mobile (no text activities) because PTY parsing is disabled.

**Fix:** Watch for new message metadata in `storage/message/<session_id>/msg_*.json` and then load corresponding `storage/part/msg_<message_id>/prt_*.json` for text content.

**Mapping strategy:**
- `msg_*.json` provides role (`user`/`assistant`) and timing.
- `prt_*.json` provides actual `text` content and part type.
- Build MobileCLI `Activity` entries:
  - `assistant` + `part.type == text` → `ActivityType::Text`
  - `user` + `part.type == text` → `ActivityType::UserPrompt`
- Map tool parts (if present) to `tool_start`/`tool_result` as available (OpenCode tool outputs are stored in `tool-output/` as separate files).

## 3) Detect Model/Provider for UI Display
**Problem:** Mobile shows wrong model for OpenCode sessions (defaults to Claude model).

**Fix:** Extract `providerID`/`modelID` from the OpenCode message metadata (`msg_*.json`).
- Example: `providerID: "opencode"`, `modelID: "gemini-3-pro"`.
- This can populate the session model display without guessing.

---

# Additional Root Causes (Beyond Watchers)

## A) PTY Parsing is Disabled for Non-Claude CLIs
`pty.rs` explicitly avoids parsing activities after the JSONL redesign, which means **all non-Claude CLIs** without watchers effectively have no activity feed.

**Implication:**
- OpenCode fails completely (no watcher).
- Codex and Gemini should work because they do have watchers, but if the watcher fails to initialize or find files, they will also silently fail to produce activities.

**Suggested fallback:**
- If watcher fails to initialize, re-enable a minimal PTY fallback pipeline for that CLI (text-only, minimal filtering) to avoid blank activity feeds.

## B) Waiting‑for‑Input Detection May Mask OpenCode Output
`parser.rs` uses hardcoded marker assumptions for OpenCode (`●`, `│`), but these are unverified. If OpenCode does not use these markers, `waiting_for_input` may never fire, which means:
- mobile may never leave “Processing…” even after the response appears.

**Suggested fallback:**
- For OpenCode, detect waiting state using prompt regex like `^\s*>\s*$` or line starts with `❯` *without* requiring marker matches.
- Optionally use OpenCode TUI-specific prompt hints if identifiable via log or storage.

---

# Extra Evidence: OpenCode Has Real Data Stores

This was verified directly on this machine:
- `~/.local/share/opencode/storage/session` contains per-session JSON.
- `~/.local/share/opencode/storage/message` contains per-session message metadata.
- `~/.local/share/opencode/storage/part` contains message part text.

This strongly suggests a direct “OpenCodeWatcher” can be built without relying on PTY parsing or JSON mode.

---

# Additional Suggested Investigations (Outside-the-Box)

## 1) OpenCode Database `opencode.db`
There is a SQLite DB at `~/.opencode/opencode.db`. It may already contain normalized session/message data and could be easier to query than file watchers. If so, a DB watcher (or polling) could replace file watching.

## 2) Tool Output Mapping
OpenCode’s `tool-output/` directory stores tool outputs (likely referenced by message parts). If we read these, we could display tool results in ActivityFeed similarly to Claude JSONL tools.

## 3) Log Stream for Prompt Markers
OpenCode log files exist in `~/.local/share/opencode/log/*.log`. These may contain prompt markers or structured logs that could be used to identify “waiting” transitions without PTY heuristics.

---

# Proposed Architecture Update (Multi‑CLI Robustness)

## CLI Support Matrix (Recommended)
- **Tier 1 (File watcher support):** Claude (JSONL), Gemini (JSON), Codex (JSONL), OpenCode (JSON storage).
- **Tier 2 (Fallback PTY):** Any CLI without a working watcher should emit minimal `text` activities from PTY, with strict filtering disabled.
- **Tier 3 (Raw terminal mirror):** If PTY parsing fails, display raw PTY lines as activities to avoid blank sessions.

## Why This Works
- Removes CLI-specific parsing brittleness.
- Ensures mobile always sees *something*, preventing “Processing…” from hanging.
- Keeps Claude in JSONL-first mode but protects other CLIs from silent failure.

---

# Actionable Next Steps (Most Valuable)

1) Implement `opencode_watcher.rs` using storage paths above.
2) Add a watcher failure fallback that enables PTY text emission.
3) Use OpenCode message metadata to fix model display (no guessing).
4) Read OpenCode `tool-output/` for tool result display.

---

# Additional Issues + Unoptimized Systems (Modal/Waiting/State)

## Tool Approval Modal Can Stall or Mask Real State
**Problem detail:** The tool approval modal relies on `waitingState` (`waitType: tool_approval`) and a set of guard flags (`handledToolApproval`, `toolApprovalBlocked`). Several flows can clear waiting state without clearing the associated `thinking` activity, or can block tool approvals longer than intended. When this happens, the UI stays stuck in a modal or spinner state even though the CLI has moved on.

**Observed risks:**
- `waiting_cleared` event clears `waitingState` but not local thinking. If the next activity never arrives (or is filtered), the spinner persists.
- `toolApprovalBlocked` is only unblocked when activity arrives; if activity never arrives (OpenCode without watcher), approvals remain blocked and the modal can fail to appear for subsequent prompts.
- Tool approval prompt detection is done in multiple channels (`pty_output`, `activity`, `waiting_for_input`). If a prompt is detected in one channel and the others are ignored, the UI can become inconsistent.

**Improvements:**
- Always clear local streaming thinking when `waiting_cleared` arrives (even if no activity follows). This prevents the modal from hiding a stuck spinner.
- Add a timeout to unblock `toolApprovalBlocked` even if no activity arrives.
- Consolidate tool approval detection into a single canonical path (e.g., prefer `waiting_for_input` and fall back to PTY only if needed) to avoid duplicate or conflicting state.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:2068`
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:1944`

---

## Session Subscription Grace Period Can Drop First Response
**Problem detail:** The “subscription grace period” ignores input state updates for a few seconds after subscription. If a session emits initial content (e.g., OpenCode fast responses) during this window, the mobile app may not show that the input was handled and can look stuck.

**Improvements:**
- Reduce grace period or make it conditional on CLI type.
- Only ignore input_state if it matches stale content (already sent), not unconditionally during grace period.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:2311`

---

## Activity Filtering Can Hide Real Output
**Problem detail:** The activity ingestion filters aggressively. For OpenCode (and likely Gemini/Codex), this can drop real content because the filtering patterns are tuned for Claude’s formatting.

**Improvements:**
- Make filters CLI-aware and less strict for OpenCode/Codex/Gemini.
- Add a “raw PTY fallback” when filtering drops all content within N seconds after a user prompt.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:1501`

---

## Model Display Fallback Is Misleading
**Problem detail:** For OpenCode, the model display falls back to `modelOptions[0]` (Claude) when `currentModel` is null. This is incorrect and confuses debugging, because the user sees the wrong model even though the CLI is running a different provider.

**Improvements:**
- Only show `modelOptions[0]` for Claude; for other CLIs show the CLI name or parse modelID from OpenCode storage.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/app/session/[id].tsx:401`

---

## Relay/WebSocket Ordering and Lost Events
**Problem detail:** Session-created and activity events can be lost if the mobile app hasn’t subscribed yet, especially when relay encryption is still initializing. This matches the observed “session not appearing until restart.”

**Improvements:**
- On mobile reconnect, request a fresh session list unconditionally (`get_sessions` + `get_activities`) rather than relying solely on incremental events.
- Add a retry queue for `session_created` broadcast until at least one mobile client ACKs it (or simply rely on periodic refresh).

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:2524`
- `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/ws.rs:532`

---

# Broad, CLI-Specific Strategy (Creative + Proactive Pass)

## Core Thesis: CLI‑Specific Pipelines Should Be First‑Class
The current system largely assumes Claude‑style behavior and then tries to patch other CLIs into it. This creates brittle, silent failures. The system should explicitly model **CLI‑specific parsers, watchers, prompt detectors, and activity filters**.

**Recommended architecture shift:**
- Introduce a `CliProfile` object (per CLI) that defines:
  - watcher type (JSONL, JSON storage, PTY fallback)
  - response markers (start/continuation)
  - waiting prompt patterns
  - tool approval patterns
  - thinking detection rules
  - activity filters / sanitizers
  - model/provider extraction rules

This would prevent “Claude‑assumptions” from leaking into OpenCode/Codex/Gemini flows.

---

## 1) CLI‑Specific Tool Approval Rules (Modal Behavior)
**Problem:** Tool approval logic is currently generic. Different CLIs have different prompt phrasing and lifecycle semantics.

**Examples of CLI‑specific differences:**
- Codex uses numbered options (allow once, allow always).
- OpenCode permissions may not prompt at all (tools default to allow unless configured).
- Gemini has different confirmation phrasing.

**Proposed fix:**
- For each CLI, define a distinct `ToolApprovalProfile`:
  - `prompt_regexes` (for prompt extraction)
  - `auto_accept` rules (trust vs tool approvals)
  - `modal_copy` (UI text per CLI)
  - `input_response_mapping` (e.g., “1”, “2”, “y”, “n”)

**Benefit:** Avoids false positives and prevents the modal from appearing when it shouldn’t.

---

## 2) CLI‑Specific Activity Filters (Stop Claude Bias)
**Problem:** Current filters exclude “noise” based on Claude output patterns (thinking words, UI hints). This can erase valid output for other CLIs.

**Proposed fix:**
- For each CLI, define allow/deny lists for:
  - thinking lines
  - UI boilerplate
  - prompt markers
  - tool banners
- Default to permissive for unknown CLIs and only tighten filters once verified.

**Benefit:** Reduces the “no output” failure mode for OpenCode/Gemini/Codex.

---

## 3) CLI‑Specific Waiting Detection (Prompt Signals)
**Problem:** `parser.rs` uses Claude‑style prompt detection. If OpenCode or Gemini have different prompts, mobile never receives `waiting_for_input`.

**Proposed fix:**
- Track per‑CLI prompt patterns (start of line, colored prompt, custom glyphs).
- Where possible, detect waiting state via storage (OpenCode file timestamps) rather than PTY prompt heuristics.

**Benefit:** Stops “Processing…” stalls even when prompt markers are different.

---

## 4) Per‑CLI “Thinking” Semantics
**Problem:** The concept of “thinking” is not consistent across CLIs. OpenCode might not emit special thinking lines at all. Codex might emit a spinner or progress tokens.

**Proposed fix:**
- Represent “thinking” as a **state** rather than “string content” where possible.
- Use:
  - watcher events (start/finish) for JSON‑based CLIs
  - PTY heuristics only as fallback
- Avoid showing thinking text for CLIs that do not expose stable text.

**Benefit:** Eliminates corrupted thinking text and mismatched “working” UI.

---

## 5) Unified “Source of Truth” Priority Per CLI
**Problem:** Multiple sources (PTY, JSONL, JSON storage) can emit overlapping content, leading to duplicates or contradictions.

**Proposed fix:**
- Establish a priority order per CLI:
  - Claude: JSONL > PTY
  - Codex: JSONL > PTY
  - Gemini: JSON > PTY
  - OpenCode: JSON storage > PTY
- Only allow lower‑priority sources to fill gaps (no duplicates).

**Benefit:** Prevents inconsistent chat history and duplicate messages.

---

## 6) “Unknown CLI” Safe Mode
**Problem:** When a CLI is unsupported, the UI fails silently (no activities, stuck spinners).

**Proposed fix:**
- Add a safe fallback mode that:
  - emits raw PTY lines as activities
  - suppresses tool‑approval modal unless a clear prompt pattern is detected
  - sets a “limited mode” banner in UI

**Benefit:** Always shows output, even if parsing is crude.

---

## 7) CLI‑Specific Model Display Strategy
**Problem:** Model selection UI is generic; OpenCode can use multiple providers and models, but the UI defaults to Claude.

**Proposed fix:**
- Claude: show model from JSONL metadata.
- OpenCode: show model from storage message metadata (providerID/modelID).
- Gemini/Codex: use watcher metadata where available, otherwise show CLI name.

**Benefit:** Accurate UI, better debugging.

---

## 8) CLI‑Specific Session Resume Logic
**Problem:** Resume commands are hardcoded; OpenCode uses `-c` for “continue last session”, but does not use conversation IDs the same way. This may reattach the wrong session.

**Proposed fix:**
- On resume, map session ID to OpenCode storage session ID and use OpenCode’s session flags (`--session`) rather than `-c` where possible.
- If not available, show a “resume not supported” warning rather than silently reusing last session.

---

## 9) CLI‑Specific Tool Output Parsing
**Problem:** Tool outputs are rendered for Claude because JSONL provides structure. OpenCode tool output exists in `tool-output/` but is ignored.

**Proposed fix:**
- For OpenCode, watch `tool-output/` and map to `ActivityType::ToolResult`.
- For Gemini/Codex, reuse their JSON watchers for tool output where available.

---

## 10) Per‑CLI “First Response” Guarantees
**Problem:** The UI often waits for a particular kind of event, and if it doesn’t arrive, the session looks hung.

**Proposed fix:**
- For each CLI, define a “first response guarantee” rule:
  - If no text/tool activity arrives in N seconds after input, emit a fallback activity that says “Waiting for response…” and unblocks the UI.

---

# Second Pass: Additional Creative Risks + Fix Ideas

## A) Duplicate Event Streams (PTY vs Watchers)
**Risk:** If PTY output is still emitted while watcher is active, user may see duplicate messages or partial fragments.

**Mitigation:**
- For each CLI, disable PTY‑derived `activity` emission once watcher confirms it is active and synced.

## B) Race Conditions Across Relay + Local
**Risk:** Relay encryption initialization can drop early events, especially for sessions started immediately after app launch.

**Mitigation:**
- When mobile connects, request a full sync (sessions + recent activities) even if relay is “connected.”

## C) “Thinking” Overlap With Tool Calls
**Risk:** Tool runs may overlap with thinking events; both could appear as active simultaneously causing UI clutter.

**Mitigation:**
- Use a single “activity lane” priority: tool_start > thinking > text. When tool starts, temporarily hide thinking unless no tool text arrives.

## D) UI Layout Assumes Claude Output Width
**Risk:** OpenCode/Gemini output can be wider or use different markup; ActivityFeed rendering can clip or wrap poorly.

**Mitigation:**
- Add CLI‑specific monospace/overflow settings and test with long tool outputs.

## E) Overreliance on “Spinner Words”
**Risk:** Hardcoded thinking words in `parser.rs` can accidentally match user content (“I was thinking...”), falsely toggling states.

**Mitigation:**
- Only treat spinner lines as thinking if they match known prefixes (spinner glyph or color reset) rather than substring matches.

---

# Third Pass: Direct Answers to Your Question (Are We Using CLI‑Specific Systems?)

**Short answer:** No, not enough. The current system is still largely Claude‑centric, with OpenCode/Codex/Gemini bolted on via generic heuristics. The lack of CLI‑specific watchers, filters, prompt detection, and tool modal rules is a primary source of stuck loading and missing chat.

**High‑impact fix:** Create explicit per‑CLI profiles and unify all detection logic through those profiles. If we do this, the mobile UI will stop assuming Claude output patterns and will handle each CLI correctly by design, not by heuristics.

---

# Extended Scan: Additional Issues + Creative Fixes (Pass 1)

## A) Tool Approval Modal Parsing Is Claude‑Biased
**Problem detail:** `ToolApprovalModal.tsx` parses prompts assuming Claude’s box‑drawing layout and tool naming. OpenCode/Gemini/Codex tool prompts may not follow this structure, so the modal can show empty/incorrect tool details.

**Observed risks:**
- `parseToolPrompt` is not CLI‑aware; tool type/command extraction uses Claude‑style prompts only.
- For OpenCode, tool approvals may be “bracket style” but the parsed command could be blank, leading to unhelpful modal.

**Fix ideas:**
- Create CLI‑specific `parseToolPrompt` functions (Claude vs OpenCode vs Codex/Gemini).
- For OpenCode, use tool metadata from storage (`tool-output/` or parts) rather than prompt text.

**Target location:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/components/ToolApprovalModal.tsx:50`

---

## B) CLI‑Specific Approval UI Logic Is Partial
**Problem detail:** The modal switches button layout for OpenCode, but the underlying `getApprovalInput` and prompt detection logic is still generic and assumes numeric options for most CLIs.

**Fix ideas:**
- Centralize approval behavior in a CLI profile so that `ToolApprovalModal` is only a view, not a decision engine.
- Ensure approval responses are mapped per CLI (e.g., OpenCode may need arrow/enter semantics rather than numeric input).

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/components/TerminalView.tsx:339`
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/patterns.ts:82`

---

## C) `isWelcomeHeaderContent` Is Not CLI‑Aware
**Problem detail:** `isWelcomeHeaderContent` uses global patterns (Claude/Gemini/Codex words) and removes content broadly. This can remove real content from OpenCode or future CLIs.

**Fix ideas:**
- Make header filtering CLI‑aware with a smaller safe list per CLI.
- For OpenCode, skip header filtering unless explicitly matched to known banner patterns.

**Target location:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/patterns.ts:252`

---

## D) Activity Filtering Is Aggressive and Claude‑Specific
**Problem detail:** The DB → activity conversion filters out markers like `●`, `⎿`, `esc to interrupt`, box drawing, etc. This is correct for Claude PTY output but can wrongly delete valid OpenCode/Codex text.

**Fix ideas:**
- Only apply these filters to `cliType === 'claude'` (or to PTY‑sourced data).
- Use a “filter profile” keyed by `cliType`.

**Target location:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/hooks/useSync.ts:1521`

---

## E) Messages vs Activities Are Diverging for OpenCode
**Problem detail:** `ws.rs` supports `GetMessages` for Claude/Codex/Gemini but explicitly falls back to DB for OpenCode, which doesn’t ingest OpenCode storage. This means historical chat is missing on reconnect, even if the storage has it.

**Fix ideas:**
- Implement OpenCode storage parsing in `ws.rs` and `get_activities` endpoints, not just watchers.
- Add OpenCode parsing to `GetMessages` so history is fetched from storage instead of DB fallback.

**Target location:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/ws.rs:1004`

---

## F) CLI‑Specific Model Switching Logic Is Limited
**Problem detail:** `getModelChangeCommand` is likely Claude‑specific; OpenCode/Gemini/Codex may use different model switching semantics, so mobile UI may send invalid commands.

**Fix ideas:**
- Only show model selection for CLIs where command semantics are verified.
- For OpenCode, use storage metadata and config rather than sending in‑session commands.

**Target location:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/mobile/app/session/[id].tsx:47`

---

## G) CLI Command Launch Assumptions
**Problem detail:** OpenCode is launched with `opencode <project>`, which starts the TUI. The system does not support OpenCode’s `--format json` or `serve/attach` modes, which would produce structured output more reliably.

**Fix ideas:**
- Add a `cliMode` setting per CLI (TUI vs JSON vs API). For OpenCode, prefer JSON or `serve/attach`.
- Expose this as a per‑session advanced option.

**Target locations:**
- `/home/bigphoot/Desktop/Projects/MobileCLI/desktop/src-tauri/src/pty.rs:298`

---

# Extended Scan: Additional Issues + Creative Fixes (Pass 2)

## H) OpenCode Storage Is Multi‑Source, But No Unified Index
**Problem detail:** Messages are split across `storage/message` and `storage/part`, and tool output lives in `tool-output`. Without a unified index, activity ordering might break, especially across reconnects.

**Fix ideas:**
- Build an OpenCode “activity assembler” that reads message meta + parts + tool outputs and merges by timestamp.
- Store assembled activities in DB for quick retrieval and reuse.

---

## I) Approval Patterns Risk False Positives
**Problem detail:** Generic approval patterns ("allow this", "do you want to proceed") could appear in normal assistant text, causing the modal to fire incorrectly.

**Fix ideas:**
- Require prompt markers (box drawing or known prompt line structure) alongside the text match.
- Use `waiting_for_input` events as the only trigger for modal, PTY detection only as fallback.

---

## J) CLI Type Inference Is Too Lenient
**Problem detail:** Many places default to `cliType || 'claude'`, which can cause OpenCode data to be filtered as Claude output.

**Fix ideas:**
- Require explicit CLI type when constructing sessions and when ingesting activities/messages.
- If CLI type is unknown, mark as `unknown` rather than `claude` to avoid Claude‑specific filters.

---

## K) Dedupe Logic Assumes Stable IDs
**Problem detail:** `addActivity` uses ID‑based dedupe and content‑based dedupe. For OpenCode storage, IDs are different from PTY IDs; this can drop legitimate messages or keep stale ones.

**Fix ideas:**
- Scope dedupe by source + CLI type.
- If watcher source is authoritative, ignore PTY duplicates by priority rather than content equality.

---

## L) CLI‑Specific Prompt Content Length Limits
**Problem detail:** `MAX_CONTENT_LENGTH` truncation can chop tool outputs or streaming content for non‑Claude CLIs that return long raw output, leading to incomplete context on mobile.

**Fix ideas:**
- Make truncation limits configurable per CLI and per activity type.
- For raw PTY fallback, allow larger payloads or paginate.

---

## M) Silent Failure When Watcher Initialization Fails
**Problem detail:** If a watcher fails to initialize (path not found, permissions), system does not fall back to PTY or notify the mobile UI.

**Fix ideas:**
- Emit a `watcher_failed` event to mobile and switch to fallback mode automatically.
- Log the fallback activation for debugging and display a banner in UI.

---

# Extended Scan: Additional Issues + Creative Fixes (Pass 3)

## N) Cross‑CLI State Mixing (Raw Activity vs Filtered Activity)
**Problem detail:** `addRawActivity` stores raw PTY content alongside filtered activity. If the filtered activity is dropped, the raw version stays hidden unless another UI shows it. This means valid data might exist but is never shown.

**Fix ideas:**
- For unsupported CLIs, display raw activity feed instead of filtered feed (opt‑in per CLI).

---

## O) Potential Misuse of `waiting_for_input` For Tool Approvals
**Problem detail:** The parser treats any prompt as waiting; but for CLIs that auto‑approve tools, `waiting_for_input` might be used for other interactions. This can trigger modal incorrectly.

**Fix ideas:**
- Require tool‑approval pattern *plus* explicit tool name or permission banner.
- Add CLI‑specific parsing in `detectInputWaitType`.

---

## P) “CLI Names” Are Hardcoded, Not Configurable
**Problem detail:** `CLI_TYPES` is hardcoded; new CLIs will default to Claude behavior without intentional design.

**Fix ideas:**
- Move CLI profiles to config with extensibility hooks.
- Allow an “experimental CLI” config that uses raw PTY mode by default.

---

# One More Pass: Meta‑Fixes That Unlock Many Issues

## 1) Centralize CLI Profiles in One Module
- Replace scattered CLI conditionals with a single config object used by mobile + desktop.
- Prevent subtle inconsistencies (e.g., patterns.ts says OpenCode uses bracket prompts while parser.rs assumes Claude markers).

## 2) Add a “Sync Health” Dashboard
- Track per session: watcher active, last activity timestamp, last waiting_for_input, last PTY chunk.
- Expose this to mobile as a debug screen so stuck states are diagnosable without logs.

## 3) Make the UI Explicit About Limited Modes
- When using fallback or raw PTY mode, show a subtle banner: “Limited parsing mode (OpenCode)”.
- This reduces confusion and surfaces why the tool modal or activity feed may look different.
