# MobileCLI Thinking/Loading State Investigation Guide

## Overview

The mobile app has issues with how it displays Claude's "thinking" state. The problems manifest as:
1. **Stuck loading** - thinking indicator stays visible when it shouldn't
2. **Hook output misclassified** - messages like "Running stop hooks... 2/6" appear as thinking content
3. **Delayed thinking indicator** - 1-2 second delay after user sends message before thinking appears
4. **Display corruption** - garbled text like "Fermenting...", "bliruning", "thinking)" suffix

## Your Task

Review the relevant code files and document your findings. I need you to trace the data flow and identify:
1. Where thinking state is determined
2. How hook output gets misclassified as thinking
3. Why the thinking indicator might get stuck
4. What causes the display corruption

---

## Key Files to Review

### Mobile App (Mac: ~/Desktop/Projects/MobileCLI/mobile/)

#### 1. `hooks/relayStore.ts`
**Focus areas:**
- How WebSocket messages are received and processed
- The `thinkingContent` state and how it's set/cleared
- Look for any regex or string matching that identifies "thinking" content
- Check message type handling (assistant, tool, thinking, etc.)

**Questions to answer:**
- What conditions set `thinkingContent`?
- What conditions clear `thinkingContent`?
- Is there a timeout mechanism if thinking gets stuck?
- How does it differentiate between actual thinking and other assistant output?

#### 2. `hooks/patterns.ts`
**Focus areas:**
- Pattern definitions for parsing Claude output
- Regex patterns that might match hook output incorrectly
- How "thinking" state is detected from raw terminal output

**Questions to answer:**
- What patterns identify thinking vs. regular output?
- Could patterns like "Running.*hooks" accidentally match thinking regex?
- Are there patterns for hook output that should exclude it from thinking?

#### 3. `components/TerminalView.tsx`
**Focus areas:**
- How thinking indicator is rendered
- What props/state control the thinking display
- Any text processing before display

**Questions to answer:**
- What component renders the thinking indicator?
- Is there sanitization of thinking content before display?
- Could concatenation issues happen here?

#### 4. `hooks/useSync.ts`
**Focus areas:**
- Sync mechanism between desktop and mobile
- How session state updates propagate
- Message ordering and timing

**Questions to answer:**
- Could race conditions cause delayed thinking indicator?
- Is thinking state synced separately from messages?

---

### Desktop App (Linux: /home/bigphoot/Desktop/Projects/MobileCLI/desktop/)

#### 5. `src-tauri/src/ws.rs`
**Focus areas:**
- WebSocket message broadcasting
- How terminal output is parsed and sent to clients
- Thinking state detection on desktop side

**Questions to answer:**
- Does desktop detect thinking state, or is it mobile-only?
- What message format is sent for thinking content?
- Are hook status messages sent as a specific message type?

#### 6. `src-tauri/src/lib.rs`
**Focus areas:**
- PTY output handling
- Message classification (thinking, tool, assistant, etc.)
- How Claude output is parsed before broadcasting

**Questions to answer:**
- Where is terminal output classified by type?
- Is there logic to detect "Running hooks" messages?
- What determines if output is "thinking" vs. regular?

---

## Specific Patterns to Look For

### Hook Output That's Misclassified
These strings appear in thinking when they shouldn't:
```
"Running PostToolUse hooks"
"Ran 6 stop hooks"
"Stop hook error: Failed with non-blocking status code"
"runing stop hooks... 2/6"
"0/6 done"
```

### Corruption Patterns
Look for what could cause:
- `"Fermenting..."` - random prefix appearing
- `"bliruning"` - text concatenation/overlap
- `"thinking)"` - suffix not being cleaned

### State Transitions to Trace
1. User sends message → thinking should appear immediately
2. Claude starts thinking → thinking indicator shows
3. Claude outputs tool call → thinking should hide
4. Tool runs → hooks run → hook output should NOT show as thinking
5. Claude responds → thinking should be cleared

---

## Investigation Checklist

### relayStore.ts
- [ ] Document the `thinkingContent` state type and initial value
- [ ] List all places where `thinkingContent` is set
- [ ] List all places where `thinkingContent` is cleared
- [ ] Check for any debouncing/throttling on thinking updates
- [ ] Note any regex patterns used to detect thinking

### patterns.ts
- [ ] List all exported patterns
- [ ] Identify which patterns relate to thinking detection
- [ ] Check if any patterns could match hook output
- [ ] Note any exclusion patterns (things that should NOT be thinking)

### TerminalView.tsx
- [ ] Find the thinking indicator component/element
- [ ] Document props that control thinking display
- [ ] Check for any string manipulation of thinking content
- [ ] Look for any useEffect that might cause stale state

### ws.rs
- [ ] Document message types sent over WebSocket
- [ ] Find where terminal output is classified
- [ ] Check if "thinking" is a message type or embedded in content
- [ ] Look for any filtering of hook output

### lib.rs
- [ ] Find PTY output handler
- [ ] Document how output types are determined
- [ ] Check for Claude-specific parsing (thinking markers)
- [ ] Look for hook detection logic

---

## Desired Output Format

Please provide your findings in this format:

```markdown
## File: [filename]

### Current Behavior
[What the code currently does]

### Issues Found
[Specific problems identified]

### Relevant Code Snippets
[Key code sections with line numbers]

### Suggested Investigation Points
[Areas that need deeper review or seem problematic]
```

---

## Context: What "Thinking" Should Be

Claude Code shows thinking in the terminal as:
- Appears when Claude is processing (before output starts)
- Shows as animated/streaming text
- Should ONLY contain Claude's internal reasoning
- Should NOT contain:
  - Hook output ("Running hooks...", "Ran X hooks")
  - Tool status messages
  - Error messages from hooks
  - System status updates

The mobile app should mirror this behavior - showing thinking only when Claude is actively thinking, not when hooks are running or tools are executing.

---

## Priority Order

1. **HIGHEST**: Why does thinking get stuck? (blocks notification system)
2. **HIGH**: Why is hook output showing as thinking?
3. **MEDIUM**: Why is there delay before thinking appears?
4. **LOW**: Display corruption issues (cosmetic)

Focus your investigation in this order. The stuck thinking state is the critical blocker.
