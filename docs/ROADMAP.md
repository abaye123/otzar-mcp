# Roadmap

Each phase ends with something usable. Update the checkboxes and dates as work lands.

## Phase 0 - research the search engine (Python, read-only)

- [ ] Capture how the app starts `searcherOtzar.exe`: run `tools/research/watch_searcher.py`
      while the user searches in the app; record command line, working dir, files written to `results\`.
- [ ] Decode the posting-list layout in `words.bin` (`tools/research/peek_words_bin.py`).
- [ ] Decode the binary `.rst` location records (`tools/research/read_rst.py`).
- [ ] Find the page id -> (book, page number) mapping.
- [ ] Find whether `open_in_otzar` can take a page (try `PAGE=` / `page=` arguments).
- Exit criterion: for 10 test queries, our book and page list equals the app's.

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

- [ ] `SearchBackend` trait; backend A (mmap `words.bin` via `memmap2`) and/or backend B (official engine).
- [ ] `search_text`: exact word, all words, within N words; filters: book ids, category, era, years.
- [ ] Use `fsalternativewords` to expand queries like the app does (optional flag).
- [ ] Ranking comparable to the app (`rate` in `.rst`).
- Exit criterion: results match the app; under 2 s for a common word with a filter.

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
