# otzar-mcp specification

Status as of 2026-11-03. This file is the single source of truth for requirements and decisions.

## 1. Goal and scope

A local MCP server, `otzar-mcp`, that lets Claude (Claude Code and the Claude desktop app)
search the whole Otzar HaHochma library on the user's disk, find books and pages, read page
text (OCR) and open books in the official app.

In scope:

- Catalog search: title, author, print year, print place, era, category.
- Full-text search through the existing index on the disk (`<disk>\searcher`), including word
  combinations and proximity.
- Page text for a page or a small page range, with context around hits.
- Opening a book in the official app.
- Navigating the category tree and the works printed inside a book (`bookssubbooks`, `categorybooks`).
- Search over the **whole** library, with every result marked by subscription access.

Out of scope:

- Any write to the disk or to `C:\OtzarApp`.
- Network access or use from another computer (stdio only, this machine only).
- Bulk export of books or building a parallel library.
- macOS and Linux. Otzar HaHochma is Windows only, and so is this server.

## 2. Environment (verified 2026-11-03)

| Item | Location |
| --- | --- |
| Official app (Electron) | `C:\OtzarApp\OtzarLocal\launcher\bin\x64\app\otzar.exe` |
| Catalog the app uses | `C:\OtzarApp\OtzarLocal\data.db` (per `db` in the disk's `config.json`) |
| Disk root | `D:\` at the time of writing; detected by content, see 6.2 |
| Disk / DB version | `data\disk-ver.txt` = 24, `data\db_ver.txt` = 7.10000 |

## 3. Architecture

One process, four modules, each reading one data source. If phase 3 (page text) stalls, the
rest keeps working.

| Module | Tools | Data | Needs disk |
| --- | --- | --- | --- |
| Catalog (`catalog.rs`) | `find_books`, `get_book`, `find_authors`, `browse_categories` | `data.db` | no |
| Full text (`words.rs`, phase 2: postings) | `word_stats`, `search_text` | `fs.db`, `tiny.db`, `words.bin` or `searcherOtzar.exe` | yes |
| Pages (phase 3) | `read_page` | `books\*.book`, `data\books.cords`, ... | yes |
| Launcher | `open_in_otzar` | `otzar.exe BOOK=<id>` | yes (the app needs it) |

Full-text search sits behind a `SearchBackend` abstraction with two possible implementations:
(A) read `words.bin` posting lists directly; (B) drive the official `searcherOtzar.exe`.
Phase 0 decoded the posting lists and the word-position-to-page mapping, so A is the primary
backend and B is only a fallback.

## 4. Tools

All tools are read-only except `open_in_otzar`. Results are JSON text. Every book result
carries `id` (the catalog `system_books.id`) so Claude can cite precisely.

| Tool | Parameters | Returns | Phase | Status |
| --- | --- | --- | --- | --- |
| `server_status` | none | disk, catalog, versions, app path, subscription config, known package codes | 1 | done |
| `find_books` | `title`, `author`, `author_id`, `year_from`, `year_to`, `place`, `period`, `limit`, `offset` | `{total, offset, items: [book summary]}` | 1 | done |
| `get_book` | `book_id` | summary + alternative titles, works inside with start pages, volume set, version, groups | 1 | done |
| `find_authors` | `name`, `limit` | authors with id, era, book count | 1 | done |
| `browse_categories` | `category_id?`, `limit` | category, children, linked books with first page id | 1 | done |
| `word_stats` | `word`, `include_alternatives`, `include_containing`, `limit` | books/occurrences, linked variants and abbreviations, containing words | 1 | done |
| `open_in_otzar` | `book_id` | `{opened}` | 1 | done (page parameter unknown, see ROADMAP) |
| `search_text` | `query`, `mode` (`near` = within `distance` words, default 30 like the app / `phrase` = adjacent), `distance`, `book_ids`, catalog filters (`title`, `author`, `author_id`, `year_from`, `year_to`, `place`, `period`), `only_subscription`, `limit`, `offset`, `pages_per_book` | `{total_books, total_hits, results: [book summary + hits, hit_pages, pages[{page, label, scan, page_id, hits}]]}` ranked by hits (the app's "relevance"); snippets from phase 3 | 2 | done |
| `read_page` | `book_id`, `page`, `count` (max 5) | OCR text of the pages | 3 | todo |

Book summary fields: `id, name, volume, authors[{id,name,role}], year, year_ce, places, period, access{status, groups}`.

Conventions:

- Default 20 results, hard cap `limits.max_results` (100). Paging with `offset` (catalog) or `cursor` (full text).
- Errors return `isError: true` with a sentence Claude can act on (e.g. disk not connected).
- `period` ids are `sederdorots`: 1 Mikra and Chazal, 2 Rishonim, 3 Acharonim 5300-5600,
  4 Acharonim 5600-5700, 5 contemporary, 6 other.
- Years accept Gregorian (1850) or Anno Mundi (5610).
- Planned: an MCP prompt `research` that guides Claude to search, read the source itself and cite book and page.

## 5. Subscription access

Search covers the whole library. Every book result has `access.status`:

- `included`: in the subscription. Books with no restricted group are treated as the base collection.
- `not_in_subscription`: belongs only to restricted groups the subscription lacks. `access.groups`
  holds their Hebrew names; Claude tells the user "(outside the subscription: <name>)".
- `unknown`: restricted, and the subscription is not configured.

Sources: `bookspackages` -> `packages`, `bookspermissions` -> `permissions`. Codes treated as
non-licensing (filters or base tiers) are listed in `catalog.rs` `NON_LICENSE_CODES`:
`"", ALL, SCHOOL, FREE, NO_BT` (assumption, to verify with the publisher).

How the subscription is known, in order of preference:
1. Read the licensed packages from `<disk>\info\key.ol` (needs the publisher's method).
2. `[subscription] packages = [...]` in `otzar-mcp.toml` (current implementation).
3. Otherwise `unknown` with the group name.

Books outside the subscription can be found and cited (book and page), but `read_page` will not
return their text, matching the official app.

## 6. Runtime behavior

### 6.1 Lifecycle

- Claude starts `otzar-mcp.exe` per session and stops it at the end. No Windows service, nothing resident.
- The official app does not need to run. Its search engine is not resident either: the app starts
  `searcherOtzar.exe` per search and releases it after 1 s (`searcherReleaseWhenNotInUse`).
- Backend B (official engine) will start `searcherOtzar.exe` on demand, attach it to a Job Object
  so it dies with the server, and stop it after 60 s idle.
- `open_in_otzar` is the only tool that opens a window.

### 6.2 Disk detection

The disk is found by content, not by letter: the first drive `C:`..`Z:` with
`searcher\fs.db`, `books\` and `data\`. Detection runs per call, so plugging the disk in works
without restarting, and an unplugged disk yields a clear error. `disk_root` in the config
overrides detection.

| Tool | Disk | Official engine | App running |
| --- | --- | --- | --- |
| catalog tools | no (catalog on `C:`) | no | no |
| `word_stats` | yes | no | no |
| `search_text` | yes | backend B only, started automatically | no |
| `read_page` | yes | no | no |
| `open_in_otzar` | yes | no | started by the tool |

## 7. Performance targets

- Catalog search under 200 ms (measured: 1-160 ms; author + year range ~1 s cold).
- `word_stats` under 100 ms warm (measured 20-30 ms; ~1 s on a cold disk).
- Rare-word full-text search under 500 ms; common word with filters under 2 s.
- Intersect posting lists starting from the rarest term; filter by category or book list first.
- The disk is external (USB): random read latency dominates. Prefer mmap and batched reads.

## 8. Security and data protection

- Read-only everywhere; SQLite opened with `SQLITE_OPEN_READ_ONLY`.
- stdio only, no listening port.
- Decryption keys (if ever provided) and decrypted text only in memory.
- Rate limit for page text: max 5 pages per call and about 200 pages per hour (configurable).
- Optional local log of tool calls (arguments only, never returned text) to show the publisher how it is used.

## 9. Configuration

`otzar-mcp.toml` next to the exe, or the path in `OTZAR_MCP_CONFIG`. See `otzar-mcp.example.toml`.
Log level: env `OTZAR_MCP_LOG` (e.g. `debug`), logs on stderr.

## 10. Language choice

Rust for the server: mmap over a 100 GB index, fast parallel page decoding, a single signed exe,
millisecond startup, official `rmcp` SDK. Python only for throwaway research scripts in
`tools/research`.
