# otzar-mcp

A local [MCP](https://modelcontextprotocol.io) server that gives Claude read-only access to an
Otzar HaHochma (אוצר החכמה) library installed on this computer: catalog search, full-text
dictionary statistics, and opening books in the official app. Windows only.

For agents and contributors: start with [AGENTS.md](AGENTS.md).

## Requirements

- Windows 10/11 with the Otzar HaHochma app (`C:\OtzarApp`) and its library disk.
- Rust stable (MSVC toolchain) to build.
- Windows SDK `signtool.exe` to sign releases.

## Build, sign, install

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

This builds `target\release\otzar-mcp.exe`, signs it (`scripts\sign.ps1`), copies it to
`C:\tools\otzar-mcp\` and registers it in Claude Code at user scope as `otzar`.

Manual steps:

```powershell
cargo build --release
powershell -File scripts\sign.ps1 target\release\otzar-mcp.exe
claude mcp add otzar --scope user -- C:\tools\otzar-mcp\otzar-mcp.exe
```

Claude desktop app: add to `%APPDATA%\Claude\claude_desktop_config.json`:

```json
{ "mcpServers": { "otzar": { "command": "C:\\tools\\otzar-mcp\\otzar-mcp.exe" } } }
```

## Configuration

Optional `otzar-mcp.toml` next to the exe (or path in `OTZAR_MCP_CONFIG`). Without it, the
server detects the disk and uses the default app paths. See [otzar-mcp.example.toml](otzar-mcp.example.toml).
Set `OTZAR_MCP_LOG=debug` for verbose logs (stderr).

## Tools

| Tool | What it does | Needs the disk |
| --- | --- | --- |
| `server_status` | where everything was found, versions, subscription config | no |
| `find_books` | catalog search by title, author, years, place, era | no |
| `get_book` | full record of one book | no |
| `find_authors` | author lookup | no |
| `browse_categories` | category tree and linked books | no |
| `word_stats` | how common a word is in the full text, variants, containing words | yes |
| `open_in_otzar` | open a book in the official app | yes |

Planned: `search_text` (full-text hits by book and page) and `read_page` (page text).
See [docs/ROADMAP.md](docs/ROADMAP.md).

## Testing

```powershell
cargo test
cargo build --release
python -I tools\smoke_test.py
```

## License and use

Personal use under the Otzar HaHochma license. See [docs/LEGAL.md](docs/LEGAL.md).
