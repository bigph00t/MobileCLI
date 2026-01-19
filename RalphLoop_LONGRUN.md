# Ralph Loop Long-Run Worklog (2026-01-18)

Use this file to stay on task across long iterations. Append new entries; do not delete prior content.

## Scope (current run)
- Plan mode output tags + command markup leakage
- Tool approvals not showing reliably on mobile
- Incorrect "Claude ready" notifications during tool/plan/clarify
- Base64/image junk in history views
- Session start residual noise in activity/history
- Mobile send newline vs send action mismatch

## Checklist (update status each iteration)
- [ ] Add explicit waitType to waiting-for-input events and consume everywhere
- [ ] Sanitize plan mode markup in JSONL + history output (preserve human text like "Enabled plan mode")
- [ ] Push recent activities on subscribe so tool calls appear on mobile immediately
- [ ] Filter base64/binary in history pipelines
- [ ] Fix mobile send vs newline conflict (return key + arrow button)
- [ ] Verify tool approval modal shows without desktop intervention
- [ ] Verify notifications match actual waitType (tool/clarify/ready)
- [ ] Verify no residual session-start noise appears in activity/history

## Iteration Log (append)

### 2026-01-18 00:00
- Iteration: 0
- Focus: Initialize long-run checklist
- Status: in_progress
- Notes:
  - Added long-run tracking file with explicit checklist and focus areas.

### 2026-01-18 01:00
- Iteration: 1
- Focus: Waiting state + plan markup + history noise
- Status: in_progress
- Notes:
  - Added waitType propagation from desktop waiting-for-input to mobile and notifications.
  - Added plan/command markup sanitization in JSONL and history pipelines.
  - Added base64/binary sanitization for tool results in JSONL and history.
  - Added JSONL summary passthrough in watcher and mobile activity add.
  - Added mobile input send guards for return key.
