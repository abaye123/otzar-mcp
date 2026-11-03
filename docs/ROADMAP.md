# Roadmap

Each phase ends with something usable. Update the checkboxes and dates as work lands.

## Phase 0 - research the search engine (Python, read-only)

- [x] Decode the posting-list layout in `words.bin` (2026-11-03, docs/DATA-FORMATS.md).
- [x] Find the word position -> page mapping: `books_data_info.bin` + `books_data.bin` +
      update bundles in `data\books_data\` (2026-11-03).
- Decision: backend A (read `words.bin` directly) is the primary backend. It needs no
  external process; the official engine (backend B) is only a fallback if A falls short.
- [x] Found the sub-indexes in `searcher\subindexs` and the merge rule (later index replaces
      earlier per book). Single-word results now equal the engine's `.rst` exactly:
      `והאידנא` 22,028 books / 53,185 hits, every per-book count identical (2026-11-03).
- [x] Multi-word queries: default = within 30 words, one-to-one pairs; 98.8% per-book identical
      to the engine for `שבת קודש` (2026-11-03, DATA-FORMATS "Multi-word queries").
- [x] "הצמדת מילים" = adjacent phrase: identical per-book counts in all our 45,794 books (2026-11-03).
- [ ] Explain the small gap where the engine finds a few more books (12 in proximity mode, 8 in
      phrase mode for `שבת קודש`).
- [ ] Decode the binary `.rst` location records (`tools/research/read_rst.py`) to compare
      positions, not only counts.
- [ ] Find whether `open_in_otzar` can take a page (try `PAGE=` / `page=` arguments).
- [ ] Optional: the app's display filter (see DATA-FORMATS, "Official engine vs. what the app displays").
- Exit criterion: for 10 test queries, our book and page list equals the app's.
  Single-word: met. Multi-word: open.

## Phase 1 - server skeleton and catalog (Rust) - done 2026-11-03

- [x] rmcp stdio server, logging to stderr, config file, disk detection by content.
- [x] `server_status`, `find_books`, `get_book`, `find_authors`, `browse_categories`.
- [x] `word_stats` with variants/abbreviations and substring lookup.
- [x] `open_in_otzar` (book only).
- [x] Subscription marking (`access.status`), configurable packages.
- [x] Smoke test over stdio (`tools/smoke_test.py`), 11/11 passing.
- [ ] Register in Claude desktop app (`claude_desktop_config.json`), not only Claude Code.
- [ ] `research` MCP prompt.

## Phase 2 - full-text search

- [x] `word_stats` merges the sub-indexes and drops deleted books (2026-11-03).
- [x] Index reader (`words.rs`): all segments, later replaces earlier, positioned reads
      (per-book slices for small sets, one sequential read for large ones). No official engine needed.
- [x] `search_text`: one word, `phrase`, `near` (default 30); filters: book ids, catalog filters,
      only_subscription; hit pages per book (`pages.rs`). Ranked by hits like the app (2026-11-03).
- [x] Smoke test numbers equal the research numbers: והאידנא 22,028/53,185 (engine-identical),
      שבת קודש phrase 45,794/252,469, near 65,823/424,089. Timings 80-500 ms warm.
- [ ] Category filter (`categorybooks` has no bookId index; needs an in-memory map or a join on categoryId).
- [ ] Optional query expansion with `fsalternativewords` (the engine does not do it by default).
- [ ] Prefix handling ("אותיות שימוש" in the app): match the word with common prefixes.
- Exit criterion: results match the app; under 2 s for a common word with a filter. Met for the tested queries.

## Phase 3 - page text

- [ ] Locate OCR text: `books.cords`, `books_data.bin`, or inside `.book`.
- [ ] If the text exists only inside encrypted `.book` files: stop and ask the publisher for a
      key or library (docs/LEGAL.md). Do not break the encryption.
- [ ] `read_page` (max 5 pages, rate limited, refuses books outside the subscription).
- [ ] Snippets in `search_text` results.
- Exit criterion: text of 20 random pages matches the app's copy-from-page.

## Later

- Subscription from `info\key.ol` (needs the publisher).
- Optional second source: Otzaria (open texts) in the same server.
