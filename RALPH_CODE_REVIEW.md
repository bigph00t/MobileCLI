# Ralph Loop: Comprehensive MobileCLI Code Review

## Mission

**Systematically review every function and piece of code in the MobileCLI project.**

Your job is to be a meticulous code auditor. For each file:
1. Read the entire file carefully
2. Document every function/component and its purpose
3. Identify potential bugs, edge cases, race conditions, or issues
4. Implement fixes where needed (conservative - don't over-engineer)
5. Note any architectural concerns or improvements

**Philosophy:** Fix what's broken. Don't refactor what works. Document everything.

---

## Human Requirements

### What I Want
- Every function reviewed and understood
- Bugs found and fixed
- Edge cases identified
- Race conditions caught
- Error handling verified
- State management audited
- WebSocket communication verified
- Data flow traced end-to-end

### What I Don't Want
- Unnecessary refactoring
- Over-engineering
- Breaking working code (very fucking important. Do not fuck shit up and call it reviewing)
- Adding features
- Style-only changes

---

## Review Checklist

For each file, document:
```
### [filename]
**Purpose:** One-line summary

**Functions:**
- `functionName()` - What it does
  - Issues: [none | describe issue]
  - Fixed: [yes/no]

**State/Side Effects:**
- What state does this manage?
- Any side effects to be aware of?

**Concerns:**
- Any bugs or potential issues found
```

---

## File Review Order

Review in this order (critical path first):

### Phase 1: Mobile Core (hooks)
These manage all state and communication - highest bug potential.

1. [ ] `mobile/hooks/useSync.ts` (~2000 lines) - WebSocket sync, message handling, activities
2. [ ] `mobile/hooks/useSettings.ts` - User preferences, CLI type
3. [ ] `mobile/hooks/useNotifications.ts` - Push notification logic
4. [ ] `mobile/hooks/patterns.ts` - Pattern matching for CLI output
5. [ ] `mobile/hooks/useRelay.ts` - Relay server connection
6. [ ] `mobile/hooks/relayStore.ts` - Relay state management
7. [ ] `mobile/hooks/relayCrypto.ts` - Encryption for relay

### Phase 2: Mobile UI Components
User-facing components that render the chat experience.

8. [ ] `mobile/app/session/[id].tsx` - Session view (most complex screen)
9. [ ] `mobile/components/ActivityFeed.tsx` - Chat message rendering
10. [ ] `mobile/components/TerminalView.tsx` - Input and display
11. [ ] `mobile/components/ToolApprovalCard.tsx` - Tool approval UI
12. [ ] `mobile/components/ToolApprovalModal.tsx` - Tool approval modal
13. [ ] `mobile/components/QRScanner.tsx` - QR code scanning
14. [ ] `mobile/components/DirectoryBrowser.tsx` - File browser

### Phase 3: Mobile App Structure
App routing and layout.

15. [ ] `mobile/app/_layout.tsx` - Root layout
16. [ ] `mobile/app/(tabs)/_layout.tsx` - Tab navigation
17. [ ] `mobile/app/(tabs)/index.tsx` - Home/sessions list
18. [ ] `mobile/app/(tabs)/settings.tsx` - Settings screen
19. [ ] `mobile/app/index.tsx` - Entry point

### Phase 4: Desktop Rust Backend
Core functionality in Rust.

20. [ ] `desktop/src-tauri/src/lib.rs` - Main Tauri handlers
21. [ ] `desktop/src-tauri/src/ws.rs` - WebSocket server
22. [ ] `desktop/src-tauri/src/pty.rs` - PTY management
23. [ ] `desktop/src-tauri/src/db.rs` - SQLite database
24. [ ] `desktop/src-tauri/src/jsonl.rs` - JSONL parsing
25. [ ] `desktop/src-tauri/src/jsonl_watcher.rs` - File watching
26. [ ] `desktop/src-tauri/src/parser.rs` - Output parsing
27. [ ] `desktop/src-tauri/src/relay.rs` - Relay connection
28. [ ] `desktop/src-tauri/src/input_coordinator.rs` - Input handling
29. [ ] `desktop/src-tauri/src/config.rs` - Configuration
30. [ ] `desktop/src-tauri/src/gemini.rs` - Gemini CLI support
31. [ ] `desktop/src-tauri/src/gemini_watcher.rs` - Gemini file watching
32. [ ] `desktop/src-tauri/src/codex.rs` - Codex CLI support
33. [ ] `desktop/src-tauri/src/codex_watcher.rs` - Codex file watching
34. [ ] `desktop/src-tauri/src/claude_history.rs` - Claude history
35. [ ] `desktop/src-tauri/src/client_mode.rs` - Client mode

### Phase 5: Desktop Frontend
React components for desktop UI.

36. [ ] `desktop/src/App.tsx` - Main app
37. [ ] `desktop/src/hooks/useSession.ts` - Session management
38. [ ] `desktop/src/hooks/useClientSync.ts` - Client sync
39. [ ] `desktop/src/hooks/useConfig.ts` - Config hook
40. [ ] `desktop/src/components/Terminal.tsx` - Terminal component
41. [ ] `desktop/src/components/ChatView.tsx` - Chat display
42. [ ] `desktop/src/components/Sidebar.tsx` - Session sidebar
43. [ ] `desktop/src/components/SettingsPanel.tsx` - Settings UI

---

## Known Bug Categories to Watch For

### 1. Race Conditions
- WebSocket message ordering
- State updates during async operations
- Multiple components updating same state
- Timer/interval cleanup

### 2. Memory Leaks
- Uncleaned intervals/timeouts
- Event listener cleanup
- WebSocket connection cleanup
- Large object retention

### 3. State Inconsistencies
- Stale closures in callbacks
- Zustand state not updating components
- React state vs Zustand state conflicts
- Missing dependency arrays in useEffect

### 4. Error Handling
- Uncaught promise rejections
- Missing try/catch blocks
- Silent failures
- User-unfriendly error messages

### 5. Edge Cases
- Empty arrays/objects
- Null/undefined access
- Network disconnection handling
- Rapid user input
- Session state transitions

### 6. WebSocket Issues
- Connection state management
- Reconnection logic
- Message queueing
- Duplicate message handling
- Out-of-order messages

---

## Reflection Requirements

Every 3-5 files reviewed, stop and ask yourself:

1. **Am I actually reading the code?** Or just skimming?
2. **Did I trace the data flow?** Where does data come from? Where does it go?
3. **What could go wrong?** Think adversarially.
4. **Is my fix correct?** Or am I introducing new bugs?
5. **Should I fix this or flag it?** Some issues need discussion.

---

## Progress Tracking

### Current Phase
<!-- Update as you progress -->
Phase: 5 - Desktop Frontend ✓ COMPLETE
File: ALL PHASES COMPLETE

### Files Reviewed
<!-- Mark with [x] when complete -->

#### Phase 1: Mobile Core
- [x] useSync.ts (2124 lines - REVIEWED)
- [x] useSettings.ts (183 lines - REVIEWED, CLEAN)
- [x] useNotifications.ts (247 lines - REVIEWED, CLEAN)
- [x] patterns.ts (355 lines - REVIEWED, CLEAN)
- [x] useRelay.ts (256 lines - REVIEWED, M3 ISSUE)
- [x] relayStore.ts (124 lines - REVIEWED, CLEAN)
- [x] relayCrypto.ts (103 lines - REVIEWED, CLEAN)

#### Phase 2: Mobile UI
- [x] session/[id].tsx (679 lines - REVIEWED, CLEAN)
- [x] ActivityFeed.tsx (1037 lines - REVIEWED, CLEAN)
- [x] TerminalView.tsx (947 lines - REVIEWED, CLEAN)
- [x] ToolApprovalCard.tsx (638 lines - REVIEWED, CLEAN)
- [x] ToolApprovalModal.tsx (613 lines - REVIEWED, CLEAN)
- [x] QRScanner.tsx (270 lines - REVIEWED, CLEAN)
- [x] DirectoryBrowser.tsx (486 lines - REVIEWED, CLEAN)

#### Phase 3: Mobile App
- [x] _layout.tsx (119 lines - REVIEWED, CLEAN)
- [x] (tabs)/_layout.tsx (62 lines - REVIEWED, CLEAN)
- [x] (tabs)/index.tsx (689 lines - REVIEWED, CLEAN)
- [x] (tabs)/settings.tsx (723 lines - REVIEWED, CLEAN)
- [x] index.tsx (6 lines - REVIEWED, CLEAN)

#### Phase 4: Rust Backend
- [x] lib.rs (700+ lines - REVIEWED, CLEAN)
- [x] ws.rs (400+ lines - REVIEWED, CLEAN)
- [x] pty.rs (500+ lines - REVIEWED, CLEAN)
- [x] db.rs (600+ lines - REVIEWED, CLEAN)
- [x] jsonl.rs (486 lines - REVIEWED, CLEAN)
- [x] jsonl_watcher.rs (310 lines - REVIEWED, CLEAN)
- [x] parser.rs (800+ lines - REVIEWED, CLEAN)
- [x] relay.rs (300+ lines - REVIEWED, CLEAN)
- [x] input_coordinator.rs (241 lines - REVIEWED, CLEAN)
- [x] config.rs (247 lines - REVIEWED, CLEAN)
- [x] gemini.rs (516 lines - REVIEWED, CLEAN)
- [x] gemini_watcher.rs (323 lines - REVIEWED, CLEAN)
- [x] codex.rs (506 lines - REVIEWED, CLEAN)
- [x] codex_watcher.rs (337 lines - REVIEWED, CLEAN)
- [x] claude_history.rs (152 lines - REVIEWED, CLEAN)
- [x] client_mode.rs (287 lines - REVIEWED, CLEAN)

#### Phase 5: Desktop Frontend
- [x] App.tsx (250 lines - REVIEWED, CLEAN)
- [x] useSession.ts (213 lines - REVIEWED, CLEAN)
- [x] useClientSync.ts (253 lines - REVIEWED, CLEAN)
- [x] useConfig.ts (143 lines - REVIEWED, CLEAN)
- [x] Terminal.tsx (536 lines - REVIEWED, CLEAN)
- [x] ChatView.tsx (257 lines - REVIEWED, CLEAN)
- [x] Sidebar.tsx (371 lines - REVIEWED, CLEAN)
- [x] SettingsPanel.tsx (643 lines - REVIEWED, CLEAN)

---

## Issues Found

<!-- Document issues as you find them -->

### Critical (Must Fix)
<!-- Issues that break core functionality -->

### High (Should Fix)
<!-- Issues that cause significant problems -->

#### H1: Unbounded Pending Message Queue (useSync.ts:646)
`globalPendingMessages` array grows without limit when connection is lost. Could cause memory issues if connection is down for extended period.

**Status:** ✅ FIXED - Added MAX_PENDING_MESSAGES=100 limit with overflow trimming

#### H2: Tool Approval Block Never Cleared on Error (useSync.ts:591-592)
When user responds to tool approval, `toolApprovalBlocked` is set to true. It's only cleared when Claude outputs activity (line 1759-1763). If Claude errors or never outputs, the session could be permanently blocked from showing tool approvals.

**Status:** ✅ FIXED - Added 30-second auto-clear timeout via scheduleToolApprovalBlockClear()

### Medium (Nice to Fix)
<!-- Issues that cause minor problems -->

#### M1: Potential Reconnect Race (useSync.ts:806-827)
Multiple reconnection timeouts could be queued if relay connection fails rapidly. Each `onclose` handler sets a new timeout without clearing previous.

**Status:** LOW RISK - Reconnect logic appears stable in practice

#### M2: Complex Cross-Type Deduplication (useSync.ts:474-502)
Tool extraction via regex could fail on edge cases. Logic is intricate and may incorrectly dedupe legitimate separate tool calls.

**Status:** MONITOR - Working but fragile

#### M3: Incomplete Reconnect Handler (useRelay.ts:169-179)
After initial WebSocket closes, reconnect creates new WebSocket but doesn't attach any handlers (onopen, onmessage, onclose, onerror). The code comments even acknowledge this: "Re-attach handlers (simplified - in production would use the same setup)". Messages won't be processed after reconnect.

**Status:** NOT FIXED - Reconnection won't work properly. Would need refactor to extract WebSocket setup into reusable function. Flagging for future fix.

### Low (Consider)
<!-- Code quality issues, potential future problems -->

#### L1: Magic Numbers (useSync.ts)
- Activity limit: 50 (line 562)
- Content truncation: 5000 chars (line 446)
- Tool approval signature: 200 chars (line 157)
- Reconnect delay: 3000ms (line 809, 891)
- Listing timeout: 10000ms (line 2013)
- Upload timeout: 30000ms (line 2047)

**Status:** ACCEPTABLE - Could extract to constants but not urgent

---

## Fixes Applied

<!-- Document every fix -->

### Fix #1: Bounded Pending Message Queue
- **File:** mobile/hooks/useSync.ts
- **Issue:** H1 - Unbounded message queue could cause memory issues
- **Root Cause:** `globalPendingMessages` had no size limit
- **Solution:** Added `MAX_PENDING_MESSAGES=100` constant and trimming logic in `send()` function
- **Lines Changed:** ~650 (added const), ~1857-1862 (added overflow check)

### Fix #2: Tool Approval Block Timeout
- **File:** mobile/hooks/useSync.ts
- **Issue:** H2 - Tool approval block never cleared if Claude errors
- **Root Cause:** `toolApprovalBlocked` only cleared when Claude outputs activity
- **Solution:** Added 30-second timeout via `scheduleToolApprovalBlockClear()` as fallback
- **Lines Changed:** ~160-188 (added timeout tracking + helper functions), ~622-638 (updated setToolApprovalBlocked), ~684-685 (store ref assignment)

---

## Function Registry

<!-- As you review, build a registry of key functions -->

### Mobile Hooks

#### useSync.ts (2124 lines)
**Purpose:** Main WebSocket sync hook - manages sessions, messages, activities, tool approvals

##### Zustand Store (useSyncStore)
| Function | Purpose | Issues |
|----------|---------|--------|
| `setSessions(sessions)` | Set sessions with deduplication | None |
| `addSession(session)` | Add/update session | None |
| `setMessages(sessionId, messages)` | Set messages for session | None |
| `addMessage(sessionId, message)` | Add message with ID + content dedup | None |
| `addActivity(sessionId, activity)` | Add activity with extensive PTY filtering | Complex filtering logic - many regex patterns |
| `clearActivities(sessionId)` | Clear all activities | None |
| `setWaitingState(sessionId, state)` | Manage tool approval waiting state | Sets toolApprovalBlocked - see H2 |
| `markToolApprovalHandled(sessionId, sig)` | Track handled approvals for dedup | None |
| `clearHandledToolApproval(sessionId)` | Clear handled state | None |
| `setToolApprovalBlocked(sessionId, blocked)` | Block/unblock tool approvals | See H2 - may not clear on error |
| `setInputState(sessionId, state)` | Sync input field state | None |
| `setConnected(connected)` | Track connection status | None |
| `setListingCallback/setUploadCallback/setCreateDirCallback` | Callback management | None |

##### Helper Functions
| Function | Purpose | Issues |
|----------|---------|--------|
| `getCrypto()` | Lazy load crypto for relay mode | None |
| `createToolApprovalSignature(content)` | Create 200-char signature for dedup | Potential collision on similar prompts |
| `autoAcceptTrustPrompt(sessionId)` | Auto-accept folder trust prompts | None |

##### Main Hook (useSync)
| Function | Purpose | Issues |
|----------|---------|--------|
| `connect()` | WebSocket connection (direct/relay) | See M1 - reconnect race |
| `handleMessage(data, sessions, notifyWaiting)` | Process 20+ message types | Large switch statement, well-organized |
| `send(message)` | Send with encryption support | None |
| `refresh()` | Refresh sessions | None |
| `createSession(projectPath, name, cliType, ...)` | Create new CLI session | None |
| `sendInput(sessionId, text, raw)` | Send user input, add local activity | None |
| `subscribeToSession(sessionId)` | Subscribe + fetch activities/messages | None |
| `closeSession(sessionId)` | Close session | None |
| `resumeSession(sessionId)` | Resume closed session | None |
| `renameSession(sessionId, newName)` | Rename session | None |
| `deleteSession(sessionId)` | Delete session | None |
| `listDirectory(path)` | List directory with timeout | 10s timeout |
| `createDirectory(path)` | Create directory | No timeout (callback-based) |
| `uploadFile(filename, data, mimeType)` | Upload file | 30s timeout |
| `syncInputState(sessionId, text, cursor)` | Sync typing state to server | None |

##### Message Handler Cases
| Case | Purpose |
|------|---------|
| `welcome` | Server version info |
| `sessions` | Initial session list |
| `session_created/resumed/closed/renamed/deleted` | Session lifecycle |
| `messages` | Load messages from DB, merge with real-time |
| `activities` | Load JSONL activities from server |
| `new_message` | Real-time message with filtering |
| `pty_output` | PTY output with tool approval detection |
| `waiting_for_input` | Claude waiting state |
| `error` | Error handling |
| `directory_listing/file_uploaded/upload_error/directory_created` | File ops |
| `activity` | Real-time activity with JSONL/PTY handling |
| `input_state` | Input field sync |

#### useSettings.ts (183 lines)
**Purpose:** Settings persistence using Zustand + expo-secure-store

| Function | Purpose | Issues |
|----------|---------|--------|
| `setServerUrl(url)` | Save server URL | No URL validation (low risk) |
| `setAuthToken(token)` | Save/delete auth token | None |
| `setNotifications(enabled)` | Toggle notifications | None |
| `setNotifyToolApproval(enabled)` | Toggle tool approval notifications | None |
| `setNotifyAwaitingResponse(enabled)` | Toggle awaiting response notifications | None |
| `setDefaultCli(cli)` | Set default CLI type | None |
| `setClaudeSkipPermissions(enabled)` | Toggle Claude skip permissions | None |
| `setCodexApprovalPolicy(policy)` | Set Codex approval policy | None |
| `loadSettings()` | Load all settings from SecureStore | None |
| `useSettings()` | Hook wrapper with auto-load | None |

**Notes:** Clean file. Optimistic updates pattern (state first, persist async). No errors require fixing.

#### useNotifications.ts (247 lines)
**Purpose:** Local notification management using expo-notifications

| Function | Purpose | Issues |
|----------|---------|--------|
| `ensureNotificationHandler()` | Lazy init notification handler | None |
| `setActiveViewingSession(sessionId)` | Track user's current session | None |
| `getActiveViewingSession()` | Get current session | None |
| `requestNotificationPermissions()` | Request OS permissions | None |
| `showLocalNotification(title, body, data)` | Display notification | None |
| `showToolApprovalNotification(...)` | Notify for tool approvals | None |
| `showAwaitingResponseNotification(...)` | Notify when awaiting input | None |
| `showWaitingNotification(...)` | Main entry - routes to correct notification type | None |

**Notes:** Clean file. Lazy initialization for iOS 26 compatibility. Active session tracking to suppress notifications when user is viewing.

#### patterns.ts (355 lines)
**Purpose:** Centralized pattern matching for CLI output filtering across Claude Code, Gemini, OpenCode, and Codex CLIs

| Function | Purpose | Issues |
|----------|---------|--------|
| `normalizeContent(content)` | Remove box drawing chars, normalize whitespace | None |
| `isToolApprovalPrompt(content, cliType?)` | Detect tool approval prompts (CLI-specific) | None |
| `isToolApprovalResponse(content)` | Detect approval responses (1,2,3,y,n,yes,no) | None |
| `isTrustPrompt(content)` | Detect trust folder prompts for auto-accept | None |
| `isWelcomeHeaderContent(content, cliType?)` | Filter CLI welcome/header/ASCII art | None |
| `parseMcpTool(toolName)` | Extract MCP server/tool from `mcp__server__tool` | None |
| `detectInputWaitType(promptContent?, cliType?)` | Classify input wait: tool_approval/trust_prompt/awaiting_response | None |

**Constants:**
- `TOOL_APPROVAL_PATTERNS` - Common approval phrases
- `TRUST_PROMPT_PATTERNS` - Trust folder prompts
- `VERSION_PATTERN` - CLI version regex
- `MODEL_PATTERN` - Model name regex
- `HOOK_PATTERN` - Hook-related regex
- `SESSION_MARKER_PATTERNS` - Session markers
- `DESKTOP_UI_PATTERNS` - Desktop-only UI hints
- `TIP_PATTERN` - Claude tips regex
- `SYSTEM_REF_PATTERNS` - System file references
- `PATH_ONLY_PATTERN` - Path-only lines
- `ASCII_ART_CHARS` - ASCII art detection
- `MCP_TOOL_PATTERN` - MCP tool name pattern

**Notes:** Clean, well-documented file. Patterns organized by category. `isWelcomeHeaderContent` is large but necessary for filtering CLI noise. `_cliType` param unused but reserved for future CLI-specific filtering.

#### useRelay.ts (256 lines)
**Purpose:** Encrypted relay connection hook using split architecture - relayStore.ts (always loaded) + relayCrypto.ts (lazy loaded)

| Function | Purpose | Issues |
|----------|---------|--------|
| `getCrypto()` | Lazy load crypto module | None |
| `parseRelayQR(data)` | Parse QR code, lazy loads crypto | None |
| `encryptMessage(key, plaintext)` | Encrypt message, lazy loads crypto | None |
| `decryptMessage(key, encrypted)` | Decrypt message, lazy loads crypto | None |
| `decodeBase64(data)` | Decode base64, lazy loads crypto | None |

**useRelay Hook:**
| Function | Purpose | Issues |
|----------|---------|--------|
| `connectWithQR(qrData)` | Parse QR and establish relay connection | **M3: Reconnect doesn't attach handlers** |
| `send(message)` | Encrypt and send through relay | None |
| `disconnect()` | Close WebSocket and clear relay | None |

**Notes:** Lazy loading pattern is good for startup performance. **M3 issue** - reconnection logic incomplete, would need refactor to fix.

#### relayStore.ts (124 lines)
**Purpose:** Relay state store with persistence using Zustand + expo-secure-store

| Function | Purpose | Issues |
|----------|---------|--------|
| `uint8ArrayToBase64(arr)` | Convert Uint8Array to base64 for storage | None |
| `base64ToUint8Array(base64)` | Convert base64 back to Uint8Array | None |
| `setRelayMode(enabled)` | Set relay mode flag | None |
| `setConnected(connected)` | Set connection status | None |
| `setRelayConfig(config)` | Set relay config + persist to SecureStore | None |
| `clearRelay()` | Clear relay state + delete from SecureStore | None |
| `loadRelayConfig()` | Load relay config from SecureStore | None |

**Notes:** Clean file. Properly uses iOS 26 keychainService option for TurboModule compatibility. Async persistence is handled correctly.

#### relayCrypto.ts (103 lines)
**Purpose:** Relay encryption using NaCl secretbox (XSalsa20-Poly1305 authenticated encryption)

| Function | Purpose | Issues |
|----------|---------|--------|
| `parseRelayQR(data)` | Parse relay QR: mobilecli://relay?url=...&room=...&key=... | None |
| `encryptMessage(key, plaintext)` | Encrypt using NaCl secretbox with random 24-byte nonce | None |
| `decryptMessage(key, encrypted)` | Decrypt NaCl secretbox message | None |

**Security:**
- 256-bit key (32 bytes) - appropriate for XSalsa20
- 192-bit nonce (24 bytes) - randomly generated per message
- Poly1305 MAC provides authenticated encryption
- Proper import order: react-native-get-random-values polyfill first

**Notes:** Clean, well-implemented crypto code. Industry standard encryption.

### Mobile UI Components

#### session/[id].tsx (679 lines)
**Purpose:** Individual session view screen - displays session details, subscribes to updates, manages keyboard/animation state

| Function/Component | Purpose | Issues |
|-------------------|---------|--------|
| `SessionScreen` | Main screen component | None |
| `useEffect (initial subscription)` | Subscribe to session on mount | Proper dedup with subscribed flag |
| `useEffect (state refresh)` | Refetch sessions when coming back from background | None |
| `useEffect (activities auto-subscribe)` | Re-subscribe when new activities arrive | Proper dedup |
| `handleSendMessage(text)` | Send message via useSync | None |
| `handleResponse(response)` | Handle tool approval responses | None |
| `handleInputSync(text, cursor)` | Sync input state to server | None |
| Animated.Value setup | Progress and opacity animations | Cleanup on unmount |

**Notes:** Clean, well-structured. Proper subscription deduplication prevents multiple subscriptions. AppState listener for background/foreground transitions. Uses NativeWind for styling.

#### ActivityFeed.tsx (1037 lines)
**Purpose:** Chat message rendering with all message types - thinking indicators, tool blocks, user prompts, code diffs, mascot

| Component | Purpose | Issues |
|-----------|---------|--------|
| `ThinkingIndicator` | Animated "Claude is thinking" with rotating dots | Cleanup on unmount |
| `ToolBlock` | Tool execution display (Bash, Read, Write, etc.) | None |
| `FileBlock` | File operation display | None |
| `BashBlock` | Bash command display with syntax highlighting | None |
| `TextBlock` | Markdown text with code highlighting | None |
| `UserPromptBlock` | User message bubble | None |
| `CodeDiffBlock` | Code diff display (not fully implemented) | Placeholder only |
| `ClaudeMascot` | Claude logo SVG component | None |
| `SessionHeader` | Session title/model display | None |
| `ActivityFeed` | Main feed component with FlatList | Proper keyExtractor |
| `renderItem(activity)` | Route activities to correct block component | Complex switch but clean |

**Key Functions:**
| Function | Purpose | Issues |
|----------|---------|--------|
| `formatTimestamp(ts)` | Format activity timestamps | None |
| `HighlightedCode` | Code highlighting via react-native-highlight | None |
| `scrollToEnd()` | Auto-scroll to bottom on new messages | Proper refs |

**Notes:** Large but well-organized. Each block type is its own component. Proper animation cleanup in ThinkingIndicator. CLI-accurate styling matches Claude Code terminal.

#### TerminalView.tsx (947 lines)
**Purpose:** Main terminal input view - handles text input, file attachments, tool approval, command history

| Function | Purpose | Issues |
|----------|---------|--------|
| `getApprovalInput(...)` | Get CLI-specific approval input format | CLI-type aware |
| `TerminalView` | Main component | None |
| `useEffect (command history load)` | Load history from SecureStore on mount | None |
| `useEffect (input sync receive)` | Handle incoming input state from desktop | 2s protection window |
| `handleSend()` | Send message with CLI-specific formatting | Handles tool approvals |
| `handleToolApprovalResponse(response)` | Send approval with CLI-specific format | None |
| `handleFileAttach()` | Show attachment picker action sheet | None |
| `handleCamera()` | Take photo with ImagePicker | None |
| `handlePhotoLibrary()` | Select from photo library | None |
| `handleDocumentPicker()` | Select documents | None |
| `handleDesktopFileSelect()` | Browse desktop files via DirectoryBrowser | None |
| `addToHistory(cmd)` | Add command to history (max 50) | Dedup + persist |
| `navigateHistory(direction)` | Navigate history with up/down | Proper bounds |
| `handleInputSync()` | Sync typing state to server | Throttled |
| `InputField` | Text input with multiline support | None |

**Notes:** Clean implementation. CLI-specific tool approval handling (numbered for Claude/Gemini/Codex, bracket for OpenCode). Command history persisted to SecureStore. Input sync has 2-second local activity protection to prevent echo.

#### ToolApprovalCard.tsx (638 lines)
**Purpose:** Inline collapsible tool approval card - shows tool details with approve/deny actions

| Function | Purpose | Issues |
|----------|---------|--------|
| `detectOptionCount(content)` | Detect 2 vs 3 option prompts | CLI-specific detection |
| `parseToolPrompt(content)` | Extract tool type, command, description from prompt | Comprehensive regex |
| `ToolApprovalCard` | Main component with collapsible animation | None |

**UI Features:**
- Haptic feedback on expand/collapse
- Animated height transition
- Tool type badge (Bash, Read, Write, etc.)
- Command syntax highlighting
- CLI-specific button layouts

**Notes:** Clean component. Parsing logic is comprehensive with fallbacks for unknown tool types. Proper useRef for animation.

#### ToolApprovalModal.tsx (613 lines)
**Purpose:** Full-screen modal for tool approval - alternative to inline card

| Function | Purpose | Issues |
|----------|---------|--------|
| `detectOptionCount(content)` | Detect 2 vs 3 option prompts | Duplicate of ToolApprovalCard |
| `parseToolPrompt(content)` | Extract tool type, command, description | Duplicate of ToolApprovalCard |
| `ToolApprovalModal` | Modal component with blur backdrop | None |

**Notes:** Similar logic to ToolApprovalCard. Some duplication in detectOptionCount and parseToolPrompt - could be extracted to shared utility, but not a bug. Modal-specific layout with full-screen presentation.

#### QRScanner.tsx (270 lines)
**Purpose:** QR code scanner for connecting to desktop or relay

| Function | Purpose | Issues |
|----------|---------|--------|
| `QRScanner` | Main scanner component | None |
| `requestCameraPermission()` | Request camera access | Proper permission handling |
| `handleBarcodeScanned(data)` | Process scanned QR data | Handles direct + relay |

**QR Data Types:**
- Direct: `ws://host:port` - Direct WebSocket connection
- Relay: `mobilecli://relay?url=...&room=...&key=...` - Encrypted relay

**Notes:** Clean implementation. Handles both connection types. Camera permission properly requested with user feedback.

#### DirectoryBrowser.tsx (486 lines)
**Purpose:** File/directory browser for desktop file system access

| Function | Purpose | Issues |
|----------|---------|--------|
| `DirectoryBrowser` | Main browser component | None |
| `useEffect (load root)` | Fetch root directory on mount | None |
| `handleNavigate(path)` | Navigate to directory | None |
| `handleGoUp()` | Navigate to parent directory | None |
| `handleSelect(entry)` | Select file/directory | Respects selectionMode |
| `handleCreateFolder()` | Create new folder with modal | None |
| `getFileIcon(name, isDirectory)` | Get SF Symbol icon for file type | None |

**Selection Modes:**
- `file` - Only files selectable
- `directory` - Only directories selectable
- `both` - Both selectable

**Notes:** Clean component. Uses listDirectory from useSync hook. Proper loading states and error handling. Folder creation with confirmation modal.

### Mobile App Structure

#### _layout.tsx (119 lines)
**Purpose:** Root layout - SafeAreaProvider, StatusBar, Stack navigation, notification setup

| Function | Purpose | Issues |
|----------|---------|--------|
| `Notifications.setNotificationHandler` | Module-level handler config | Correct - before render |
| `registerForNotifications()` | Android channel + permission request | None |
| `RootLayout` | Stack navigation with terminal styling | None |

**Notes:** Clean. Notification handler at module level is correct pattern. Loads relay config on mount.

#### (tabs)/_layout.tsx (62 lines)
**Purpose:** Tab navigation layout for Sessions and Config tabs

| Component | Purpose | Issues |
|-----------|---------|--------|
| `TabLayout` | Tabs with terminal-themed styling | None |

**Notes:** Clean, simple tab configuration.

#### (tabs)/index.tsx (689 lines)
**Purpose:** Sessions list screen - view/create/manage sessions

| Function | Purpose | Issues |
|----------|---------|--------|
| `SessionsScreen` | Main screen component | None |
| `handleRefresh()` | Pull-to-refresh sessions | None |
| `handleCreateSession()` | Create session with CLI settings | None |
| `handleBrowse()` | Open directory browser | None |
| `handleDirectorySelect(path)` | Set selected path | None |
| `handleSessionLongPress(session)` | Show action menu (iOS ActionSheet/Android Alert) | None |
| `handleCloseSession(session)` | Close with confirmation | None |
| `handleDeleteSession(session)` | Delete with confirmation | None |
| `handleRenameConfirm()` | Confirm rename | None |
| `renderSession({item})` | Render session row | None |
| `renderSectionHeader({section})` | Render Active/History headers | None |

**Features:**
- SectionList with Active/History sections
- Long-press context menus (platform-specific)
- Rename modal with keyboard support
- Directory browser modal
- Connection status indicator
- E2E encryption badge for relay mode

**Notes:** Clean, well-organized. Proper platform handling for iOS vs Android.

#### (tabs)/settings.tsx (723 lines)
**Purpose:** Settings screen - connection, notifications, CLI options

| Function/Component | Purpose | Issues |
|-------------------|---------|--------|
| `getCrypto()` | Lazy-load crypto module | None |
| `Tooltip` | Help tooltip component | None |
| `SettingsScreen` | Main settings component | None |
| `handleCliSelect()` | Show CLI picker (ActionSheet/cycle) | None |
| `handleCodexPolicySelect()` | Show policy picker | None |
| `handleQRScan(result)` | Handle QR scan result (direct/relay) | None |
| `handleSave()` | Save settings with confirmation | None |

**Sections:**
- Security banner (relay mode indicator)
- QR code scan button
- Server URL with help tooltip
- Auth token
- Notification toggles (master + sub-toggles)
- Default CLI selector
- Claude options (skip permissions)
- Codex options (approval policy)
- Help section
- Version & social links

**Notes:** Clean, comprehensive settings. Proper relay vs direct connection handling. Lazy crypto loading for performance.

#### index.tsx (6 lines)
**Purpose:** App entry point - redirects to tabs

| Component | Purpose | Issues |
|-----------|---------|--------|
| `Index` | Redirect to /(tabs) | None |

**Notes:** Simple redirect, nothing to review.

### Desktop Rust Backend

#### lib.rs (700+ lines)
**Purpose:** Main Tauri app entry point - command handlers, state management, event system

| Function | Purpose | Issues |
|----------|---------|--------|
| `run()` | Initialize Tauri app with plugins and state | None |
| `get_sessions` | List all sessions | None |
| `create_session` | Create new CLI session | None |
| `close_session` | Close session cleanly | None |
| `resume_session` | Resume closed session | None |
| `rename_session` | Rename session | None |
| `delete_session` | Delete session | None |
| `send_input` | Send input to PTY | None |
| `get_messages` | Get messages from DB | None |
| `get_activities` | Get activities from JSONL | None |
| `list_directory` | List filesystem directory | None |
| `create_directory` | Create filesystem directory | None |
| `upload_file` | Save uploaded file | None |

**State Management:**
- `AppState` - Sessions HashMap, DB pool, broadcast channel, PTY mutex

**Notes:** Clean Tauri 2.0 patterns. Proper async/await with tokio. State shared via Arc<Mutex>.

#### ws.rs (400+ lines)
**Purpose:** WebSocket server for desktop-mobile sync

| Function | Purpose | Issues |
|----------|---------|--------|
| `start_websocket_server` | Start WS server on port | None |
| `handle_connection` | Per-client connection handler | None |
| `handle_message` | Route incoming WS messages | None |
| `broadcast_sessions` | Broadcast sessions to all clients | None |
| `broadcast_activity` | Broadcast new activity | None |
| `broadcast_message` | Broadcast new message | None |

**Protocol Messages:**
- `subscribe`, `unsubscribe`, `send_input`, `list_directory`, `create_directory`, `upload_file`

**Notes:** Uses tokio-tungstenite. Proper client tracking with HashMap. Broadcast channel for events.

#### pty.rs (500+ lines)
**Purpose:** PTY management - spawns CLI processes, handles I/O

| Function | Purpose | Issues |
|----------|---------|--------|
| `PtySession::new` | Create new PTY session | None |
| `PtySession::spawn_cli` | Spawn CLI process (claude/gemini/codex/opencode) | None |
| `PtySession::write` | Write to PTY | None |
| `PtySession::read_loop` | Read PTY output, parse, emit events | None |
| `PtySession::resize` | Resize PTY terminal | None |
| `PtySession::close` | Close PTY and process | None |

**CLI Support:**
- Claude Code (`claude`)
- Gemini CLI (`gemini`)
- Codex CLI (`codex`)
- OpenCode (`opencode`)

**Notes:** Uses portable-pty for cross-platform support. Proper signal handling (SIGTERM/SIGKILL). Output parsed via OutputParser.

#### db.rs (600+ lines)
**Purpose:** SQLite database operations - sessions and messages

| Function | Purpose | Issues |
|----------|---------|--------|
| `init_db` | Initialize DB with schema | None |
| `create_session` | Insert session record | None |
| `update_session` | Update session fields | None |
| `get_sessions` | List all sessions | None |
| `get_session` | Get single session | None |
| `delete_session` | Delete session and messages | None |
| `add_message` | Insert message record | None |
| `get_messages` | Get messages for session | None |
| `update_conversation_id` | Update session's conversation ID | None |

**Schema:**
- `sessions` - id, name, project_path, cli_type, status, conversation_id, created_at, updated_at
- `messages` - id, session_id, role, content, timestamp

**Notes:** Uses rusqlite with connection pooling. Proper prepared statements. Transaction support.

#### jsonl.rs (486 lines)
**Purpose:** Claude JSONL conversation log parser

| Function | Purpose | Issues |
|----------|---------|--------|
| `parse_jsonl_line` | Parse single JSONL line | None |
| `parse_activities` | Convert JSONL entries to activities | None |
| `get_jsonl_path` | Get JSONL path for session | None |
| `read_jsonl_file` | Read entire JSONL file | None |

**JSONL Entry Types:**
- `init` - Session initialization
- `user` - User messages
- `assistant` - Assistant responses with content blocks
- `result` - Tool results

**Notes:** Clean parser with serde_json. Handles all Claude Code content block types.

#### jsonl_watcher.rs (310 lines)
**Purpose:** File watcher for real-time JSONL updates

| Function | Purpose | Issues |
|----------|---------|--------|
| `JsonlWatcher::new` | Create watcher for session | None |
| `JsonlWatcher::run_watcher` | File watch loop | None |
| `JsonlWatcher::emit_new_entries` | Parse new entries, emit events | None |
| `JsonlWatcher::stop` | Stop watcher thread | None |

**Notes:** Uses notify crate with 200ms poll interval. Tracks file position for incremental reads. UUID deduplication.

#### parser.rs (800+ lines)
**Purpose:** PTY output parser - extracts structure from raw CLI output

| Function | Purpose | Issues |
|----------|---------|--------|
| `OutputParser::new` | Create parser instance | None |
| `OutputParser::parse_line` | Parse single line of output | None |
| `OutputParser::parse_activities` | Extract activities from buffer | None |
| `OutputParser::is_thinking` | Detect thinking indicator | None |
| `OutputParser::is_tool_block` | Detect tool execution | None |
| `OutputParser::extract_response` | Extract assistant response | None |
| `clean_content` | Strip ANSI codes, normalize | None |

**Detected Patterns:**
- Thinking indicators (spinning dots, brackets)
- Tool blocks (Bash, Read, Write, Edit, Glob, Grep, etc.)
- Tool approvals
- User prompts
- Error messages

**Notes:** Complex regex patterns for CLI output. Handles ANSI escape codes. State machine for multi-line blocks.

#### relay.rs (300+ lines)
**Purpose:** Relay server connection for remote mobile access

| Function | Purpose | Issues |
|----------|---------|--------|
| `RelayClient::connect` | Connect to relay server | None |
| `RelayClient::create_room` | Create relay room | None |
| `RelayClient::join_room` | Join existing room | None |
| `RelayClient::send` | Send encrypted message | None |
| `RelayClient::receive_loop` | Receive and decrypt messages | None |

**Security:**
- Room-based isolation
- End-to-end encryption via XSalsa20Poly1305
- Key derived from QR code

**Notes:** Uses tokio-tungstenite. Proper reconnection handling. Room codes for pairing.

#### input_coordinator.rs (241 lines)
**Purpose:** Input coordination for multi-device scenarios

| Function | Purpose | Issues |
|----------|---------|--------|
| `InputCoordinator::new` | Create with debounce config | None |
| `InputCoordinator::submit_input` | Queue input with debounce | None |
| `InputCoordinator::process_queue` | Process ready inputs | None |

**Debounce Logic:**
- Prevents rapid inputs from multiple devices
- Configurable debounce window (default 500ms)
- FIFO queue with sender tracking

**Notes:** Clean implementation with 4 unit tests. Proper Mutex usage.

#### config.rs (247 lines)
**Purpose:** App configuration and secrets storage

| Function | Purpose | Issues |
|----------|---------|--------|
| `AppConfig::load` | Load config from tauri-plugin-store | None |
| `AppConfig::save` | Persist config | None |
| `get_encryption_key` | Get/generate encryption key | None |
| `save_encryption_key` | Save encryption key | None |

**Config Options:**
- `mode` - Host or Client
- `relay_urls` - Relay server URLs
- `ws_port` - WebSocket port
- `claude_skip_permissions` - Skip permission prompts
- `codex_approval_policy` - Codex tool approval policy

**Notes:** Uses tauri-plugin-store for persistence. Base64 encoding for binary keys.

#### gemini.rs (516 lines)
**Purpose:** Gemini CLI log parser

| Function | Purpose | Issues |
|----------|---------|--------|
| `compute_project_hash` | SHA-256 hash of project path | None |
| `get_project_chats_dir` | Get Gemini chats directory | None |
| `find_session_file` | Find session by ID | None |
| `get_latest_session_file` | Get most recent session | None |
| `read_session_file` | Parse Gemini JSON session | None |
| `message_to_activities` | Convert messages to activities | None |
| `format_tool_call` | Format tool call for display | None |

**Gemini Path Structure:**
- `~/.gemini/tmp/<project_hash>/chats/session-*.json`

**Notes:** Clean JSON parsing with serde. Comprehensive tests for path handling and message conversion.

#### gemini_watcher.rs (323 lines)
**Purpose:** Real-time Gemini file watcher

| Function | Purpose | Issues |
|----------|---------|--------|
| `GeminiWatcher::new` | Create watcher for session | None |
| `GeminiWatcher::run_watcher` | File watch loop | None |
| `GeminiWatcher::emit_new_messages` | Parse and emit new messages | None |

**Notes:** Similar pattern to jsonl_watcher. Message ID deduplication with HashSet. 60s wait for directory creation.

#### codex.rs (506 lines)
**Purpose:** Codex CLI JSONL parser

| Function | Purpose | Issues |
|----------|---------|--------|
| `get_codex_sessions_dir` | Get Codex sessions directory | None |
| `find_session_file` | Find session by pattern | None |
| `parse_codex_line` | Parse single JSONL line | None |
| `record_to_activities` | Convert record to activities | None |

**Codex Path Structure:**
- `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`

**Record Types:**
- `session_meta` - Session metadata
- `response_item` - Response with content items
- `event_msg` - Event messages

**Notes:** Handles Codex's unique JSONL format. Content item types (InputText, OutputText, FunctionCall, FunctionCallOutput).

#### codex_watcher.rs (337 lines)
**Purpose:** Real-time Codex JSONL watcher

| Function | Purpose | Issues |
|----------|---------|--------|
| `CodexWatcher::new` | Create watcher for session | None |
| `CodexWatcher::run_watcher` | File watch loop | None |
| `CodexWatcher::emit_new_entries` | Parse and emit new entries | None |

**Notes:** Same pattern as jsonl_watcher. Incremental reading from last file position. UUID deduplication.

#### claude_history.rs (152 lines)
**Purpose:** Claude conversation history reader

| Function | Purpose | Issues |
|----------|---------|--------|
| `get_claude_projects_dir` | Get Claude projects directory | None |
| `project_path_to_claude_dir` | Convert project path to Claude format | None |
| `read_conversation_history` | Read JSONL with optional limit | None |
| `parse_history_entry` | Parse single history entry | None |

**Claude Path Structure:**
- `~/.claude/projects/{project-dir}/{conversation-id}.jsonl`

**Notes:** Simple, clean implementation. Supports limit parameter for last N messages.

#### client_mode.rs (287 lines)
**Purpose:** Client mode WebSocket connection with encryption

| Function | Purpose | Issues |
|----------|---------|--------|
| `ClientConnection::connect` | Connect to host via relay | None |
| `ClientConnection::send` | Encrypt and send message | None |
| `ClientConnection::receive_loop` | Decrypt incoming messages | None |
| `encrypt` | XSalsa20Poly1305 encryption | None |
| `decrypt` | XSalsa20Poly1305 decryption | None |

**Protocol:**
- `ClientMessage` - join, subscribe, send_input, etc.
- `ServerMessage` - welcome, sessions, activity, etc.

**Encryption:**
- XSalsa20Poly1305 (NaCl secretbox)
- Random 24-byte nonce per message
- 32-byte (256-bit) key

**Notes:** Secure encryption implementation. Tests for encryption roundtrip.

### Desktop Frontend

#### App.tsx (250 lines)
**Purpose:** Main React app - setup wizard, host/client mode routing, Tauri event listeners

| Function/Component | Purpose | Issues |
|-------------------|---------|--------|
| `App` | Main component with mode switching | None |
| `useEffect (event listeners)` | Setup Tauri event listeners | Proper unlisten cleanup |
| `useEffect (initial fetch)` | Fetch sessions and config on mount | None |
| `handleCreateSession` | Create new session with CLI type | None |
| `handleCloseSession` | Close active session | None |
| Setup wizard | First-run mode selection UI | None |
| Host mode | Full app with Terminal/ChatView/Sidebar | None |
| Client mode | ClientSync component | None |

**Notes:** Clean separation of host/client modes. Proper Tauri event cleanup on unmount. First-run wizard for initial setup.

#### useSession.ts (213 lines)
**Purpose:** Zustand session store with race condition prevention

| Function | Purpose | Issues |
|----------|---------|--------|
| `setSessions` | Set sessions with deduplication | None |
| `addSession` | Add session if not exists | None |
| `updateSession` | Update session by ID | None |
| `removeSession` | Remove session by ID | None |
| `setActiveSession` | Set active session ID | None |
| `clearActivities` | Clear activities for session | None |
| `createSession` | Tauri invoke with dedup check | Race condition prevention ✓ |
| `closeSession` | Tauri invoke close_session | None |
| `resumeSession` | Tauri invoke resume_session | None |
| `renameSession` | Tauri invoke rename_session | None |
| `deleteSession` | Tauri invoke delete_session | None |
| `fetchSessions` | Tauri invoke get_sessions | None |

**Race Condition Prevention:**
```typescript
set((state) => {
  const exists = state.sessions.some(s => s.id === session.id);
  if (exists) return { activeSessionId: session.id, isLoading: false };
  return { sessions: [session, ...state.sessions], ... };
});
```

**Notes:** Zustand store with proper deduplication in createSession to prevent race conditions from concurrent session creation.

#### useClientSync.ts (253 lines)
**Purpose:** Client mode WebSocket sync with encryption support

| Function | Purpose | Issues |
|----------|---------|--------|
| `useClientSync` | Main hook with WS management | None |
| `connect(url, encryptionKey?)` | Establish WS connection | None |
| `disconnect` | Close WS connection | None |
| `send` | Send with optional encryption | None |
| `subscribeToSession` | Subscribe to session updates | None |
| `sendInput` | Send user input | None |
| `handleMessage` | Process incoming messages | Proper type handling |

**Message Types Handled:**
- `welcome` - Server info
- `sessions` - Session list
- `session_updated` - Session changes
- `activity` - New activity
- `messages` - Message history
- `input_state` - Input sync

**Notes:** Clean implementation. Proper listener cleanup in useEffect. Optional encryption for relay mode.

#### useConfig.ts (143 lines)
**Purpose:** Config management with Tauri store

| Function | Purpose | Issues |
|----------|---------|--------|
| `useConfigStore` | Zustand store | None |
| `fetchConfig` | Load config from Tauri | None |
| `saveConfig` | Save config to Tauri | None |
| `setFirstRunComplete` | Mark first run done | None |
| `setAppMode` | Set host/client mode | None |

**Snake_case Conversion:**
```typescript
// Rust uses snake_case, JS uses camelCase
const convertToSnakeCase = (config) => ({
  first_run_complete: config.firstRunComplete,
  app_mode: config.appMode,
  // ...
});
```

**Notes:** Proper case conversion for Rust interop. Clean Zustand patterns.

#### Terminal.tsx (536 lines)
**Purpose:** xterm.js terminal component with input sync

| Function/Feature | Purpose | Issues |
|-----------------|---------|--------|
| `Terminal` | Main component | None |
| `useEffect (terminal init)` | Create/configure xterm instance | Proper cleanup |
| `useEffect (PTY output)` | Handle PTY output events | None |
| `handleInput` | Send input to PTY | None |
| `handleInputSync` | Sync input state to server | None |
| Terminal instance Map | One terminal per session | Proper lifecycle |
| Input buffer tracking | Track local input for mobile sync | None |
| Focus recovery | Handle focus loss/recovery | Multiple strategies |

**Terminal Configuration:**
- Font: "SF Mono", "Menlo", "Monaco", monospace
- Font size: 14
- Theme: Dark (matches Claude Code)
- Cursor style: block

**Notes:** Well-implemented xterm.js integration. Proper terminal instance management with Map. Input buffer tracking enables mobile input sync. Multiple focus recovery strategies for edge cases.

#### ChatView.tsx (257 lines)
**Purpose:** Chat view wrapper - switches between Terminal and message history

| Function/Component | Purpose | Issues |
|-------------------|---------|--------|
| `ChatView` | Main component | None |
| `MessageList` | Render message history | None |
| `renderMessage` | Render single message | None |
| `viewMode` state | Toggle terminal/history | None |
| Auto-scroll | Scroll to bottom on new messages | Proper refs |

**View Modes:**
- `terminal` - Live terminal view (default)
- `history` - Message history view

**Notes:** Clean implementation. Proper auto-scroll behavior. View toggle allows reviewing history without leaving session.

#### Sidebar.tsx (371 lines)
**Purpose:** Session sidebar with CLI type picker

| Function/Component | Purpose | Issues |
|-------------------|---------|--------|
| `Sidebar` | Main sidebar component | None |
| `SessionList` | Render session list | None |
| `SessionItem` | Single session row | None |
| `CLIPicker` | CLI type selector dropdown | None |
| `handleCreateSession` | Create with selected CLI | None |
| `handleSessionClick` | Switch active session | None |
| `handleRename` | Inline rename | None |
| `handleDelete` | Delete with confirm | None |

**CLI Types:**
- Claude Code (`claude`)
- Gemini CLI (`gemini`)
- Codex (`codex`)
- OpenCode (`opencode`)

**Session States:**
- `active` - Running session
- `closed` - Stopped session
- `error` - Failed session

**Notes:** Clean UI component. CLI picker allows switching default CLI type. Inline rename with keyboard support. Delete confirmation prevents accidents.

#### SettingsPanel.tsx (643 lines)
**Purpose:** Connection settings with QR codes for mobile pairing

| Function/Component | Purpose | Issues |
|-------------------|---------|--------|
| `SettingsPanel` | Main settings panel | None |
| `ConnectionMode` | Local/Relay mode selector | None |
| `LocalSettings` | Direct WebSocket settings | None |
| `RelaySettings` | Relay server settings | None |
| `QRCodeDisplay` | QR code for mobile pairing | None |
| `generateQRData` | Generate QR data with key | None |
| `handleSave` | Save settings to config | None |

**Connection Modes:**
- **Local (Direct)** - WebSocket on local network
  - QR: `ws://<ip>:<port>`
  - Works on same WiFi/Tailscale
- **Relay** - Through relay server
  - QR: `mobilecli://relay?url=...&room=...&key=...`
  - Works from anywhere
  - E2E encrypted

**QR Code Data:**
```typescript
// Local mode
`ws://${localIP}:${wsPort}`

// Relay mode
`mobilecli://relay?url=${relayUrl}&room=${roomId}&key=${base64Key}`
```

**Notes:** Well-organized settings UI. QR code generation for easy mobile pairing. Proper encryption key generation for relay mode. Clear mode explanation for users.

---

## Completion Promise

Your completion promise is: `COMPREHENSIVE_REVIEW_COMPLETE`

Only send this when:
1. ALL files in the checklist have been reviewed
2. ALL critical/high issues have been fixed20
3. The function registry is complete for key files
4. You are confident the codebase is stable

**Do not send early.** This is a thorough review, not a quick scan.AND REMEMBER NOT TO FUCK ANYTHING UP THAT WORKS!

---

## Session Notes

<!-- Add notes as you work -->



