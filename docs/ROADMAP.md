# Roadmap

Each phase ends with something usable. Update the checkboxes and dates as work lands.

## Phase 0 - research the search engine (Python, read-only) - done 2026-11-03

- [x] Decode the posting-list layout in `words.bin` (docs/DATA-FORMATS.md).
- [x] Find the word position -> page mapping: `books_data_info.bin` + `books_data.bin` +
      update bundles in `data\books_data\`.
- Decision: read `words.bin` directly (backend A). No official engine process is needed.
- [x] Sub-indexes in `searcher\subindexs` and the merge rule (later index replaces earlier per
      book). Single word equals the engine exactly: `והאידנא` 22,028 books / 53,185 hits.
- [x] Multi-word, "הצמדת מילים" off: within 30 words, one-to-one pairs; 98.8% of books identical.
- [x] Multi-word, "הצמדת מילים" on: adjacent phrase; identical in all our 45,794 books.
- Tools: `tools/research/compare_engine.py` repeats the comparison for any search made in the app.

Known gaps (minor, not blocking):

- [ ] The engine finds a few more books than we do for multi-word queries (12 near, 8 phrase,
      out of ~50,000 for `שבת קודש`). Decoding the binary `.rst` location records
      (`tools/research/read_rst.py`) would show which positions it matched.
- [ ] The app hides some engine results (display filter). Not needed: we mark access instead of hiding.

## Phase 1 - server skeleton and catalog (Rust) - done 2026-11-03

- [x] rmcp stdio server, logging to stderr, config file, disk detection by content.
- [x] `server_status`, `find_books`, `get_book`, `find_authors`, `browse_categories`.
- [x] `word_stats` with variants/abbreviations and substring lookup.
- [x] `open_in_otzar` (book only).
- [x] Subscription marking (`access.status`), configurable packages.
- [x] `research` MCP prompt.
- [x] Smoke test over stdio (`tools/smoke_test.py`) and single-call helper (`tools/call.py`).
- [x] `open_in_otzar` with a page, through the app's `OtzarBook://book/<id>/p/<page>/...` link
      (verified with the user watching the app, 2026-11-03). Each call opens another app window.
- [ ] Claude desktop app registration: the app is not installed on this machine (no
      `%APPDATA%\Claude`). README has the config snippet for when it is.

## Phase 2 - full-text search - done 2026-11-03

- [x] Index reader (`words.rs`): all segments, later replaces earlier, positioned reads.
- [x] `search_text`: one word, `phrase`, `near` (default 30); filters: book ids, catalog filters,
      only_subscription; hit pages per book (`pages.rs`); ranked by hits like the app.
- [x] `word_stats` merges the sub-indexes and drops deleted books.
- [x] `category_id` for `find_books` (books) and `search_text` (hits limited to the category's
      word ranges; `pageId = 0` rows mean the whole book).
- [x] `prefixes` option: the word with one or two prefix letters (ו ה ב כ ל מ ש ד), like
      "אותיות שימוש". Not compared with the app yet: run a search with that option on and
      compare with `tools/research/compare_engine.py` after extending it to prefixes.
- [ ] Optional query expansion with `fsalternativewords` (the engine does not do it by default).

## Agent test (2026-11-03)

A fresh headless Claude answered "sources for eating dairy on Shavuot" with the installed
server (`tools/agent_test.py`).

- Run 1: 19 calls, 3 failed (`bookyears.toYear` can be NULL; fixed). Claude quoted from
  memory and said so only at the end.
- Run 2, after the fix and clearer server instructions (page text unavailable, label wording
  from memory, search tips): 11 calls, 0 errors, about 25 sources with page and access, wording
  clearly labeled as unverified, offers open_in_otzar.
- Remaining limit: without page text Claude cannot confirm the context of a hit.

## Phase 3 - page text - blocked on the publisher

- [x] Located the OCR text (2026-11-03): it is not in any readable file; only the encrypted
      `books\*.book` files can hold it (DATA-FORMATS, "Where is the page text?").
- [ ] Decision by the user / publisher, one of:
      1. the publisher provides a method or library to read `.book` page text (preferred);
      2. explicit approval to rebuild the text locally from the inverted index (a full copy of
         the text; forbidden by the project rules without that approval).
- [ ] Then: `read_page` (max 5 pages, rate limited, refuses books outside the subscription)
      and snippets in `search_text` results.
- Exit criterion: text of 20 random pages matches the app's copy-from-page.

## Later

- Subscription from `info\key.ol` (needs the publisher).
- Optional second source: Otzaria (open texts) in the same server.
