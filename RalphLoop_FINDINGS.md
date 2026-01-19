# Ralph Loop Findings

Append-only log for discoveries, open questions, and hypotheses.

## Template

### YYYY-MM-DD HH:MM
- Iteration: <number>
- Finding:
  - <bullet>
- Evidence:
  - <paths/logs>
- Open questions:
  - <bullet>
- Follow-ups:
  - <bullet>

---

### 2026-01-18 02:45
- Iteration: 2
- Finding:
  - Desktop waiting state only recognized tool_approval/awaiting_response; added plan_approval and clarifying_question.
  - Desktop App was emitting waiting-for-input without waitType during request-waiting-state sync.
  - Mobile processing indicator could persist on stale user_prompt when no waitingState is present.
- Evidence:
  - `desktop/src/App.tsx` waiting-for-input listener and request-waiting-state emit
  - `desktop/src/components/Sidebar.tsx` waitingState UI mapping
  - `mobile/app/session/[id].tsx` processing logic
- Open questions:
  - Manual verification needed for plan mode enter/exit on both desktop and mobile.
- Follow-ups:
  - Run QA checklist and validate tool approval modals without desktop involvement.
