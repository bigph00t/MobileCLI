# MobileCLI Ralph Loop Prompt (2026-01-18)

You are Claude running in a Ralph loop. This file is your **repeated master prompt**. Your job is to iteratively fix the MobileCLI product to a “foolproof and seamless” standard across desktop + mobile. This is **not** a one-pass change: you must loop until every item is complete, verified, and resilient to future CLI changes or plugin variations.

You must be thorough, evidence-based, and avoid assumptions. You must read relevant files and confirm actual behavior. If uncertain, add instrumentation, run tests, or add diagnostic logs. Each loop should push the product forward in small, verified increments.

---

## Core Mission

Deliver a truly adaptive CLI platform that links the desktop coding session to mobile in a seamless, robust, and future-proof way. The UI must remain stable across different CLIs, evolving output formats, custom plugins, and unexpected prompt structures.

“Working product” means:
- Tool approvals are always correctly detected and actionable.This includes Plan acceptance, clarifying questions, etc.
- Thinking/processing state is consistent and never flickers wrongly.
- Input sync works across devices without conflicts.
- Resume + permission prompts are auto-accepted reliably.
- Clarifying prompts are shown correctly and never misidentified.
- Tool summary displays are accurate and concise.
- Status indicators never mislead users.

---

## Ralph Loop Workflow (must follow)

### 1) Initialize the loop (every iteration)
- Re-read this file and confirm which checklist items remain incomplete.
- Decide a **small set of fixes** (1–3) for this iteration.
- Identify affected files before coding.
- Confirm expected behavior changes.

### 2) Deeply verify before changing
- Read current implementation and confirm how it behaves **right now**.
- Confirm whether the bug is due to:
  - wrong detection logic
  - race conditions across event streams
  - stale states not being cleared
  - UI assumptions (2-option vs 3-option prompts)
- If the fix is uncertain, add temporary debug logging rather than guessing.

### 3) Implement with precision
- Apply the minimal change that fixes the specific behavior.
- Keep UI consistent with existing design system unless explicitly changing it.
- Avoid breaking cross-CLI compatibility.

### 4) Verify immediately
- Run tests or manual checks for the affected path.
- Confirm no regressions in related behaviors (see verification section below).

### 5) Track progress
Maintain and update these files (append-only):
- `RalphLoop_PROGRESS.md` (high-level status each iteration)
- `RalphLoop_CHANGES.md` (technical changes + reasoning)
- `RalphLoop_FINDINGS.md` (new discoveries, open questions)
- `RalphLoop_TESTS.md` (tests/run + manual verification notes)

**If these files do not exist, create them.** Append timestamped sections only.

### 6) Git discipline (strongly encouraged)
- Use `git status` and `git diff` to track changes.
- Keep changes scoped per iteration.
- Commit only if explicitly asked by the user; otherwise, use diffs.

---

## Global Principles

1) **No assumptions**. If the behavior is unclear, instrument or reproduce it.
2) **One source of truth** for UI states (avoid triple detection sources).
3) **CLI type awareness everywhere** (Claude/Gemini/Codex/OpenCode).
4) **Graceful fallback**: if parsing fails, show raw output safely.
5) **No fragile string hacks**. Prefer structured parsing and durable patterns.

---

## Master Checklist (complete all)

> For each item below, produce code changes, validation notes, and add evidence to `RalphLoop_TESTS.md`.

### 1. File picker: create folder (desktop + mobile)
**Goal:** In both mobile and desktop file pickers, a user can create a new folder during new session creation.

Checklist:
- Desktop: provide an in-app “Create Folder” step for new sessions.
- Desktop: validate path safely and report errors gracefully.
- Mobile: ensure “New Folder” works in all select modes.
- Mobile: disable “Select current path” when selectMode is `file` and no file is selected.
- Confirm new folder is immediately visible after creation.

Relevant files:
- `desktop/src/components/Sidebar.tsx`
- `desktop/src/App.tsx`
- `desktop/src-tauri/src/lib.rs`
- `mobile/components/DirectoryBrowser.tsx`
- `mobile/components/TerminalView.tsx`

---

### 2. Resume sessions: auto-accept settings warnings
**Goal:** Resumed and new sessions always autoaccept trust/permissions warnings on desktop and mobile.

Checklist:
- Add `claude_skip_permissions` to resume payload (client → ws → pty).
- Ensure `pty.rs` uses this override for resume.
- Verify `waiting_for_input` trust prompts are still autoaccepted on mobile.
- Add timing guard: if resume event and trust prompt appear within 1s, auto-accept.
- Ensure tool approvals are not autoaccepted.

Relevant files:
- `desktop/src-tauri/src/pty.rs`
- `desktop/src-tauri/src/ws.rs`
- `mobile/hooks/useSync.ts`

---

### 3. Tool approval modal: dynamic options & layout
**Goal:** Tool approvals render correctly for any number of options; >3 options must display vertically.

Checklist:
- Replace inline ToolApprovalCard with ToolApprovalModal (or merge features).
- Parse arbitrary option lists from prompt content.
- Render >3 options vertically with consistent spacing.
- Match CLI-specific input mapping per option.
- Ensure summary and raw views toggle correctly.
- Handle non-standard plugin prompt formats gracefully.

Relevant files:
- `mobile/components/ToolApprovalCard.tsx`
- `mobile/components/ToolApprovalModal.tsx`
- `mobile/hooks/patterns.ts`
- `mobile/components/TerminalView.tsx`

---

### 4. Plan mode exit / stuck state
**Goal:** Exiting plan mode never breaks chat. Detect and handle exit signals cleanly.

Checklist:
- Detect `exitplanmode` and related plan markers in parsing.
- Introduce `waitType: 'plan_mode'` or equivalent, and cleanly exit.
- Ensure tool approvals and waiting states remain intact after plan exit.
- Add failsafe: if a tool signature is detected but modal not shown, reset after short timeout.

Relevant files:
- `mobile/hooks/patterns.ts`
- `mobile/hooks/useSync.ts`
- `desktop/src-tauri/src/parser.rs`

---

### 5. Status updates while typing
**Goal:** Status never changes to “Claude working” while user is typing or before sending.

Checklist:
- Ensure “Processing…” is emitted only on actual send, not on local input state changes.
- Introduce or display a “User typing” indicator instead of “working”.
- Prevent “awaiting response” from appearing during typing.
- Confirm mobile/desktop status are consistent and stable.

Relevant files:
- `desktop/src-tauri/src/pty.rs`
- `desktop/src/components/Sidebar.tsx`
- `mobile/hooks/useSync.ts`

---

### 6. Input sync: desktop → mobile newline issue
**Goal:** Input sync preserves text and cursor correctly across devices.

Checklist:
- Sync cursor position from mobile to desktop and desktop to mobile.
- Use controlled `selection` in mobile `TextInput` to align cursor.
- Avoid sending input in a way that inserts newline when remote sends.
- Ensure input sync doesn’t interfere with sending (no duplicate input).

Relevant files:
- `desktop/src/components/Terminal.tsx`
- `desktop/src-tauri/src/ws.rs`
- `mobile/components/TerminalView.tsx`

---

### 7. Thinking stream stability
**Goal:** Thinking indicator persists through tool calls and only ends when Claude is actually done.

Checklist:
- Do not clear thinking on tool_start/tool_result or tool approval display.
- Clear thinking only on confirmed assistant text output.
- Remove flicker caused by duplicate waiting states.
- Maintain a monotonic `thinkingSequenceId` or equivalent for consistency.
- Ensure spinner remains at bottom while tool calls appear above it.

Relevant files:
- `mobile/hooks/useSync.ts`
- `mobile/components/ActivityFeed.tsx`
- `desktop/src-tauri/src/parser.rs`

---

### 8. Clarifying questions modal
**Goal:** Clarifying questions are detected and shown in a distinct UI.

Checklist:
- Add waitType `clarifying_question`.
- Implement a new clarifying prompt modal with vertical options.
- Display question text + options from prompt content.
- Ensure it doesn’t conflict with tool approval detection.

Relevant files:
- `mobile/hooks/patterns.ts`
- `mobile/hooks/useSync.ts`
- `mobile/components/TerminalView.tsx`

---

### 9. Tool modal accept stuck
**Goal:** Tool approval acceptance always dismisses modals and unblocks future approvals.

Checklist:
- Clear `toolApprovalBlocked` on `waiting_cleared`.
- Add 1–2s debounce after a response to prevent duplicate prompts.
- Ensure only one detection stream sets tool_approval (prefer waiting_for_input).

Relevant files:
- `mobile/hooks/useSync.ts`
- `desktop/src-tauri/src/lib.rs`

---

### 10. Checklist output misidentified
**Goal:** Markdown checklists render as user-facing output and are never filtered as system/tool data.

Checklist:
- Tighten `SYSTEM_REF_PATTERNS` to match only real system markers.
- If content matches markdown checklist, skip system filtering.
- Ensure checklist is rendered cleanly in ActivityFeed.

Relevant files:
- `mobile/hooks/patterns.ts`
- `mobile/components/ActivityFeed.tsx`

---

### 11. Tool summary shows junk
**Goal:** “Show summary” displays clean, structured tool intent, not noisy prompt text.

Checklist:
- Emit JSONL summary entries instead of skipping them.
- Add a new ActivityType for summaries or render as text blocks.
- Use structured summary in tool modal when available.

Relevant files:
- `desktop/src-tauri/src/jsonl.rs`
- `mobile/components/ToolApprovalModal.tsx`
- `mobile/components/ToolApprovalCard.tsx`

---

## Cross-cutting Architecture Improvements

### A) Consolidate tool-approval detection
- Prefer `waiting_for_input` as primary source.
- `pty_output` should be a fallback if no `waiting_for_input` within a short timeout.
- Avoid triple detection of tool approvals.

### B) State machine per session
- Implement a small FSM: idle → waiting_for_assistant → tool_approval → awaiting_response → idle.
- Eliminate racing clears between tool approvals, waiting, and thinking.

### C) Centralize filters
- Consolidate content filters into one shared pipeline for mobile.
- Use CLI-type aware logic in all filters.

---

## Verification Checklist (every iteration)

You must verify for the changes you touched:

1. New session → create folder → select folder → session starts.
2. Resume session → trust prompt autoaccepted.
3. Tool approval prompt with 4+ options displays vertically.
4. Tool approval accept closes modal and does not get stuck.
5. Clarifying question prompt shows distinct modal and correct options.
6. Thinking indicator persists across tool calls and only clears on assistant text.
7. Input sync preserves cursor, no accidental newline or double-send.
8. Checklist markdown is displayed in ActivityFeed.
9. Tool summary shows clean content.

Append each verification attempt to `RalphLoop_TESTS.md` with outcome.

---

## Logging/Instrumentation Guidelines

- If detection is unstable, add temporary debug logs.
- Remove logs once behavior stabilizes (or guard with dev flags).
- Keep logs focused: sessionId, waitType, input type, prompt signature.

---

## Contribution Hygiene

- Prefer targeted diffs over massive refactors unless necessary.
- Keep CLI awareness explicit; never assume Claude-only behavior.
- When adding new pattern logic, include tests or at least debug instrumentation.

---

## Starting Point: Critical Issues to Prioritize

Start with these first if nothing else is in progress:
1) Thinking stream stability (Issue #7)
2) Tool approval modal + dynamic options (Issue #3)
3) Resume autoaccept + trust prompt reliability (Issue #2)
4) Input sync cursor fixes (Issue #6)

---

## Final Objective

The system must be resilient to:
- CLI output changes
- custom hooks and plugins
- alternate prompt layouts
- timing/race conditions
- multiple devices editing simultaneously

If the system fails under any of these, the loop must continue.
