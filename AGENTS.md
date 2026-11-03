# otzar-mcp: guide for coding agents

Local MCP server (Rust, stdio) that lets Claude search the user's **Otzar HaHochma**
(אוצר החכמה) library: a licensed external disk with ~161,000 scanned Jewish books, plus
the official Electron app installed under `C:\OtzarApp`.

Read this file first, then the doc that matches your task:

| Task | Read |
| --- | --- |
| What the server must do, tool contracts, decisions | [docs/SPEC.md](docs/SPEC.md) |
| Phase status and next steps | [docs/ROADMAP.md](docs/ROADMAP.md) |
| Anything about the disk files, tables, binary layouts | [docs/DATA-FORMATS.md](docs/DATA-FORMATS.md) |
| What the license allows and the rules that follow from it | [docs/LEGAL.md](docs/LEGAL.md) |
| Build, sign, install, register with Claude | [README.md](README.md) |

## Hard rules

1. **Never write to the Otzar disk or to `C:\OtzarApp`.** Open SQLite read-only
   (`SQLITE_OPEN_READ_ONLY`), open files with read sharing, never create files there.
   The official app may be running at the same time.
2. **stdout belongs to the MCP protocol.** Log only to stderr (`tracing`, already wired).
   Any child process must get `Stdio::null()` for stdin/stdout/stderr.
3. **No bulk export.** Tools return bounded results (`limits.max_results`, default 100).
   Do not add tools that dump whole books or the whole index. See docs/LEGAL.md.
4. **No network listener.** stdio transport only.
5. **Decryption keys and decrypted text stay in memory.** Never cache decrypted content on disk.
6. **Respect subscription access.** Results carry `access.status`; text of books outside the
   subscription must not be returned by `read_page` (phase 3).
7. Language: everything in the repo is English (code, comments, docs, commits).
   Data values are Hebrew because the library is Hebrew. The user talks to agents in Hebrew.
8. Commits describe the change only: no AI attribution lines, no "Generated with" footers.
9. Every release `otzar-mcp.exe` is signed (see README, `scripts/sign.ps1`).
10. **Windows only.** Otzar HaHochma has no macOS/Linux version. `main.rs` has a
    `compile_error!` for other targets; use Windows APIs and paths freely, no cfg branches
    for other platforms.

## Layout

```
src/main.rs       entry point: logging to stderr, config, stdio serve
src/server.rs     MCP tools (rmcp #[tool_router]); each tool = params struct + async fn
src/catalog.rs    catalog queries over data.db (books, authors, categories, access, alternatives)
src/words.rs      full-text index: main + sub-index segments (fs.db + words.bin), merge rule, postings
src/search.rs     matching rules (word, phrase, near) equal to the official engine; ranking
src/pages.rs      per-book page lists (books_data*.bin): word position -> page
src/config.rs     otzar-mcp.toml, disk detection by content, default paths
src/hebrew.rs     Hebrew numerals and year parsing (unit tested)
tools/smoke_test.py         end-to-end MCP test over stdio against the real data
tools/call.py               call one tool: python -I tools\call.py search_text '{"query": "שבת"}'
tools/agent_test.py         a fresh headless Claude answers a research question with the
                            installed server; prints its tool calls, errors and answer (costs tokens)
tools/research/             phase-0 scripts for reverse-engineering formats (read-only)
scripts/                    sign.ps1, install.ps1
otzar-mcp.example.toml      documented config template
```

## Commands

```powershell
cargo build --release                      # target\release\otzar-mcp.exe
cargo test                                 # unit tests (no disk needed)
python -I tools\smoke_test.py              # end-to-end, needs the catalog; full-text calls need the disk
powershell -File scripts\install.ps1       # build, sign, copy to C:\tools\otzar-mcp, register in Claude Code
```

`cargo clippy --all-targets` should stay clean.

## How to add or change a tool

1. Add the query to `catalog.rs` / `words.rs` (or a new module) as a plain sync method
   returning a `Serialize` type. Keep SQL parameterized; build `LIKE` patterns with `like()`.
2. In `server.rs` add a `#[derive(Deserialize, JsonSchema)]` params struct. Doc comments on
   fields become the JSON schema descriptions Claude sees: write them for Claude.
3. Add an `async fn` in the `#[tool_router] impl` with `#[tool(description = ..., annotations(read_only_hint = true))]`
   and call `self.run(move |st| ...)`, which runs the work on a blocking thread and turns
   errors into tool errors (`isError: true`) with a readable message.
4. Clamp limits with `st.limit(requested, default)`.
5. Add a call to `tools/smoke_test.py`, run it, and update docs/SPEC.md (tool table) and
   docs/ROADMAP.md.

## Data facts you will need (details in docs/DATA-FORMATS.md)

- Catalog: `C:\OtzarApp\OtzarLocal\data.db` (the copy the app uses), fallback `<disk>\data\data.db`.
- `packages` / `permissions`: column `title` is the code (`KOOK`), `name` is the Hebrew name.
- `years.name` is a Hebrew numeral label (`תש"ל`, `תר"צ בערך`); parse with `hebrew::parse_year`.
- `fsalternativewords.type`: 1 = spelling variant/synonym, 2 = abbreviation <-> expansion.
  Only direct links are meaningful (siblings in a type-2 group are unrelated).
- `fs.db.words(w, b, l, p)`: token, books, occurrences, offset into `words.bin`. Indexed on `w`.
  Tokens are stored as printed, with prefixes (`והשבת`), and include OCR noise.
- `.book` files are encrypted (`BKF` header). Do not try to break the encryption without the
  publisher's written approval and method (docs/LEGAL.md).

## Working with the user

The owner reads Hebrew, not English. Chat replies and status updates to them are in Hebrew;
technical terms, file names and code may stay in English. Plans and approval questions in Hebrew.
