# MobileCLI UX Investigation - January 17, 2026

## Issues to Fix

### Issue 1: "Claude is ready for input" Shows Prematurely
**Status**: 🟢 Fixed
**Priority**: High
**Description**: When user sends a new prompt, the "Claude is ready for input" banner appears for a second BEFORE thinking starts, instead of thinking starting immediately.

**Root Cause Found**:
The debounce mechanism in TerminalView.tsx (lines 203-204) was **declared but never implemented**:
```typescript
const [showAwaitingIndicator, setShowAwaitingIndicator] = useState(false);
const awaitingDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
```
These were declared but never used! The indicator used an inline timestamp check.

**Fixes Applied**:
1. ✅ Added clearing of waitingState in `subscribeToSession` (useSync.ts line 2648-2650)
2. ✅ Implemented proper debounce using useEffect (TerminalView.tsx lines 262-293)
3. ✅ Updated render condition to use `showAwaitingIndicator` state (TerminalView.tsx line 666)

---

### Issue 2: "Claude is ready" Appears Randomly During Conversation
**Status**: 🟢 Fixed (same as Issue 1)
**Priority**: High
**Description**: The "Claude is ready" indicator pops up and disappears randomly throughout the conversation.

**Root Cause Found**:
Same as Issue 1 - no proper debounce mechanism. The inline timestamp check passed for any state >500ms old.

**Fixes Applied**: Same as Issue 1 - proper debounce now prevents random appearances.

---

### Issue 3: "Claude is ready" Not Reliably Showing When Done
**Status**: 🟢 Fixed (previous session fixes + current debounce)
**Priority**: High
**Description**: When Claude actually finishes, indicator doesn't always show.

**Root Cause Found**:
1. Waiting state was being cleared in multiple places (assistant message handler, activity handler)
2. Previous session fixes (lines 1918 and 2299) preserve `awaiting_response` state

**Fixes Applied**:
- Previous session: Modified message/activity handlers to preserve `awaiting_response`
- Current session: Proper debounce ensures indicator shows reliably after 500ms

---

### Issue 4: Thinking Indicator Inconsistency/Jumpiness
**Status**: 🟢 Fixed (by debounce fix)
**Priority**: Medium
**Description**: Thinking indicator is inconsistent and jumpy.

**Root Cause Found**:
The perceived jumpiness was primarily caused by the awaiting indicator flashing, not the thinking indicator itself. The thinking indicator system works correctly:
1. `sendInput` adds "Processing..." activity (line 2614-2622)
2. Desktop events update/replace thinking activities
3. `waiting_for_input` clears thinking (line 2124-2137)

**Fix Applied**: The proper debounce for the awaiting indicator resolves the perceived jumpiness because:
- No more awaiting indicator flashing before/after thinking
- Clean transition: thinking → (500ms delay) → awaiting indicator
- Immediate hide when thinking starts

---

### Issue 5: Base64 Image Data in Thinking Portion
**Status**: 🟢 Fixed
**Priority**: Medium
**Description**: Base64 image data appears in activity content when conversation is reloaded.

**Root Cause Found**:
No filter for base64 data in content filtering.

**Fix Applied**:
Added base64 content filter in useSync.ts (lines 398-416):
- Filters `data:image/jpeg;base64,...` format
- Filters raw base64 strings (100+ chars, 90%+ base64 chars)

---

### Issue 6: File Attachment Needs Trailing Space
**Status**: 🟢 Fixed
**Priority**: Low
**Description**: After file attachment is inserted, user cannot immediately type.

**Root Cause Found**:
Lines 468, 496, 533, 567 in TerminalView.tsx missing trailing space after path.

**Fix Applied**:
Updated all 4 locations to add trailing space:
```typescript
setInputText((prev) => (prev ? `${prev} ${path} ` : `${path} `));
```

---

## Progress Log

### Session: January 17, 2026
- Created investigation document
- Analyzed all 6 issues reported by user
- Fixed Issue 6: Trailing space after file attachments
- Fixed Issue 5: Base64 content filter
- Fixed Issues 1-3: Proper debounce for awaiting indicator
- Verified Issue 4: Thinking jumpiness was symptom of awaiting indicator issues

### Files Modified:
1. `mobile/components/TerminalView.tsx`
   - Added proper debounce useEffect for awaiting indicator (lines 262-293)
   - Updated render condition to use debounced state (line 666)
   - Added trailing space to file attachments (lines 469, 498, 536, 571)

2. `mobile/hooks/useSync.ts`
   - Added base64 content filter (lines 398-416)
   - Added clearing of waitingState in subscribeToSession (lines 2648-2650)

---

## Key Files to Examine

1. **mobile/components/TerminalView.tsx** - Main UI, sendInput, thinking indicator display
2. **mobile/hooks/useSync.ts** - WebSocket message handling, state management
3. **mobile/hooks/useSyncStore.ts** - Zustand store for waitingStates, thinkingStates
4. **mobile/app/session/[id].tsx** - Session screen, may have state handling
5. **mobile/components/ActivityCard.tsx** - Activity display, content filtering

---

### Issue 7: Custom Plugin Tool Approval Detection
**Status**: 🟢 Fixed
**Priority**: Medium
**Description**: Custom plugins (like `/plan`) may use non-standard tool approval prompts that won't be detected by the existing hardcoded patterns.

**Example**: `/plan` command shows:
```
Would you like to proceed?
> 1. Yes, clear context and auto-accept edits (shift+tab)
  2. Yes, and manually approve edits
  3. Yes, auto-accept edits
```

**Root Cause Found**:
Pattern detection was based on hardcoded text patterns like `'1. yes'`, `'do you want to proceed'`. Custom plugins with different option text (e.g., "1. Run the migration") wouldn't be detected.

**Fix Applied**:
Added `hasInteractivePromptStructure()` function in patterns.ts that uses structural analysis:

**Key detection criteria (to avoid false positives from regular lists):**
1. Options must be at the END of content (not embedded)
2. Options must be SHORT (< 100 chars each) - regular list items are often 200+ chars
3. Must have a question or prompt phrase BEFORE options
4. Selection indicator (`>`) is a strong positive signal

**Files Modified:**
- `mobile/hooks/patterns.ts` - Added `hasInteractivePromptStructure()` and `PROMPT_PHRASES`

---

## Investigation Notes

(Notes will be added here as investigation progresses)
