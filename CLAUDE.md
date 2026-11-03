@AGENTS.md

## Claude Code specifics

- Test changes with `python -I tools\smoke_test.py` against `target\release`. Do not add a
  project `.mcp.json` that points at `target\release\otzar-mcp.exe`: editors (VS Code Copilot)
  start it automatically and the running process locks the file, so `cargo build` fails with
  "Access is denied". If that happens, find the holder with
  `Get-CimInstance Win32_Process -Filter "Name='otzar-mcp.exe'"`.
- The installed copy used in everyday sessions lives in `C:\tools\otzar-mcp\` and is registered
  at user scope as `otzar`. Update it with `scripts\install.ps1`.
- docs/SPEC.md is the single source of truth for requirements and decisions. Record new
  decisions there (and status in docs/ROADMAP.md), not only in chat.
