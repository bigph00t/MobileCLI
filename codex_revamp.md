# Codex Revamp Log

## 2026-01-22
- Added cross-platform PTY spawn helpers (home resolution, tilde expansion, project dir creation, shell selection, PATH enrichment).
- Normalized project path before spawning CLI and ensured missing directories are created when starting/resuming sessions.
- Switched CLI spawn to OS-aware builders (Unix uses login shell + stty -echo, Windows runs binaries directly).
- Hardened CLI availability checks to avoid bash/zsh on Windows and search PATH/known install dirs.
- Mobile: kept scroll pinning, paste prompt behavior, and tool modal parsing fixes active; updated WebView message type for scroll events.
- Note: creating `/home/bigphoot/Desktop/codextester` from this sandbox failed (permission denied); needs manual run on host.
- Bumped iOS buildNumber + Android versionCode to 53 in `mobile/app.json`.
- EAS build attempt failed from sandbox with DNS error `getaddrinfo EAI_AGAIN api.expo.dev`; also hit `~/.cache/eas-cli` permission issue (worked around via `XDG_CACHE_HOME` but DNS still failing).
- Hardened conversation ID tracking: parser now only extracts IDs from lines mentioning session/conversation ID.
- Only store generated conversation IDs for Claude sessions; other CLIs now update IDs from actual watcher file paths (Codex/Gemini/OpenCode) and emit conversation-id events.
- Prevented Claude sessions from overriding conversation IDs via parser output to avoid resume failures.
- Attempted desktop build via `desktop/build.sh`; Linux bundle failed at linuxdeploy (binary not runnable in this environment). Frontend + Rust build completed; AppImage/DEB bundling hit linuxdeploy failure.
- Pushed desktop repo changes to `bigph00t/MobileCLI` and website repo to `bigph00t/MobileCLI-website` for release-based downloads.
