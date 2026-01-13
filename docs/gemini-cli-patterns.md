# Gemini CLI Patterns Documentation

## Overview

MobileCLI supports Gemini CLI as a second CLI type alongside Claude Code. This document details the patterns used to parse and display Gemini CLI output.

## CLI Type Enum

```rust
// desktop/src-tauri/src/db.rs
pub enum CliType {
    ClaudeCode,
    GeminiCli,
}
```

String representations:
- Database/Settings: `"gemini"`
- Display name: `"Gemini CLI"`

## Session Management

### Starting a New Session
```rust
// desktop/src-tauri/src/pty.rs:208-212
let mut cmd = CommandBuilder::new("gemini");
cmd.cwd(&project_path);
```

### Resuming a Session
```rust
// desktop/src-tauri/src/pty.rs:640-644
let mut cmd = CommandBuilder::new("gemini");
cmd.arg("--resume");
cmd.arg(&conversation_id); // Index like "1" or "latest"
cmd.cwd(&project_path);
```

**Note:** Gemini uses `--resume` with session index, whereas Claude uses `--continue` with UUID.

## Parser Patterns

### Thinking Indicators

```rust
// desktop/src-tauri/src/parser.rs:117-122
CliType::GeminiCli => vec![
    "Thinking", "thinking...", "Processing",
    "Analyzing", "Generating", "Working",
    "esc to cancel",
],
```

### Response Markers

```rust
// desktop/src-tauri/src/parser.rs:130
CliType::GeminiCli => ('▶', '│'),  // Start marker, continuation marker
```

| Marker | Character | Purpose |
|--------|-----------|---------|
| Start | `▶` (U+25B6) | Beginning of response line |
| Continuation | `│` (U+2502) | Subsequent response lines |

**Note:** These markers need real-world verification against actual Gemini CLI output.

### Waiting-for-Input Detection

The same patterns used for Claude Code apply:
- `\n> ` / `\n❯ ` - Standard prompts
- `Allow?` / `Continue?` - Permission prompts
- `[Y/n]` / `(y/n)` - Yes/no prompts

## Mobile App Patterns

### Version Detection
```typescript
// mobile/hooks/patterns.ts:129
export const VERSION_PATTERN = /(claude code|gemini cli|aider)\s*v?\d+(\.\d+)*/i;
```

### Model Detection
```typescript
// mobile/hooks/patterns.ts:132
export const MODEL_PATTERN = /\b(opus|sonnet|haiku|claude|gpt|gemini|flash)\s*\d*\.?\d*/i;
```

### Welcome Header Filtering
```typescript
// mobile/hooks/patterns.ts:238-239
if (/^●\s*(claude code|gemini)/i.test(trimmed)) return true;
if (/^⎿\s*(opus|sonnet|claude|gpt|gemini|\/home\/)/i.test(trimmed)) return true;
```

### ActivityFeed Welcome Header

Gemini CLI has a dedicated welcome header in `mobile/components/ActivityFeed.tsx:538-546`:
- Google blue background (#4285F4)
- Sparkles icon
- "Gemini CLI" title
- "Gemini 2.0 Flash" subtitle

## Tool Approval Patterns

The same tool approval patterns are used for both Claude and Gemini:
- `do you want to proceed`
- `1. yes`, `2. yes, and...`, `3. no`
- `esc to cancel`

**Verification Needed:** Gemini CLI's actual tool approval format may differ.

## Settings Storage

```typescript
// mobile/hooks/useSettings.ts
defaultCli: CliType  // 'claude' | 'gemini'
```

Stored in `SecureStore` with key `mobilecli_default_cli`.

## Gaps and TODOs

### Needs Real-World Verification
1. **Response markers** - Actual `▶` and `│` characters need verification
2. **Thinking indicators** - Current list may be incomplete
3. **Tool approval format** - May differ from Claude Code
4. **Error message format** - Not documented
5. **Session resume format** - `--resume` flag behavior

### Future Improvements
1. Add Gemini-specific activity icons
2. Support Gemini's model selection (`--model flash-2.0`)
3. Handle Gemini-specific error states
4. Parse Gemini's token/cost display

## Testing Checklist

- [ ] Start new Gemini session
- [ ] Resume existing session
- [ ] Verify thinking indicators display
- [ ] Test tool approval flow
- [ ] Verify response streaming
- [ ] Check mobile display of Gemini responses
- [ ] Test session sync between desktop and mobile

## Version History

| Date | Change |
|------|--------|
| 2026-01-11 | Initial documentation created during production readiness review |
