//! MCP tool surface. Each tool runs its blocking SQLite work on a blocking thread.

use crate::catalog::{BookQuery, BookSummary, Catalog};
use crate::config::{self, Config};
use crate::search::{self, Mode};
use crate::words::FullText;
use std::collections::BTreeMap;
use anyhow::Context;
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const DISK_MISSING: &str = "The Otzar HaHochma disk is not connected (no drive with searcher\\fs.db, books and data). \
     Catalog tools still work; full-text tools need the disk.";

pub struct State {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    catalog: Mutex<Option<Catalog>>,
    words: Mutex<Option<FullText>>,
}

impl State {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        Self { config, config_path, catalog: Mutex::new(None), words: Mutex::new(None) }
    }

    fn with_catalog<R>(&self, f: impl FnOnce(&Catalog) -> anyhow::Result<R>) -> anyhow::Result<R> {
        let mut guard = self.catalog.lock().map_err(|_| anyhow::anyhow!("catalog lock poisoned"))?;
        if guard.is_none() {
            let disk = self.config.find_disk();
            let path = self
                .config
                .catalog_db(disk.as_deref())
                .context("catalog data.db not found (set catalog_db in otzar-mcp.toml)")?;
            *guard = Some(Catalog::open(&path, &self.config.subscription)?);
        }
        f(guard.as_ref().expect("catalog opened above"))
    }

    /// Lock order: catalog before words, never the reverse.
    fn with_words<R>(&self, f: impl FnOnce(&FullText) -> anyhow::Result<R>) -> anyhow::Result<R> {
        let mut guard = self.words.lock().map_err(|_| anyhow::anyhow!("word index lock poisoned"))?;
        // Re-detect when the disk was unplugged or re-lettered since the last call.
        if guard.as_ref().is_some_and(|w| !config::is_otzar_disk(&w.root)) {
            *guard = None;
        }
        if guard.is_none() {
            let disk = self.config.find_disk().context(DISK_MISSING)?;
            *guard = Some(FullText::open(&disk)?);
        }
        f(guard.as_ref().expect("word index opened above"))
    }

    fn limit(&self, requested: Option<usize>, default: usize) -> usize {
        requested.unwrap_or(default).clamp(1, self.config.limits.max_results)
    }
}

#[derive(Clone)]
pub struct OtzarServer {
    state: Arc<State>,
}

impl OtzarServer {
    pub fn new(state: State) -> Self {
        Self { state: Arc::new(state) }
    }

    async fn run<T, F>(&self, f: F) -> Result<CallToolResult, ErrorData>
    where
        T: Serialize + Send + 'static,
        F: FnOnce(&State) -> anyhow::Result<T> + Send + 'static,
    {
        // `T` is serialized into the text content; Claude reads the JSON directly.
        let state = self.state.clone();
        let outcome = tokio::task::spawn_blocking(move || f(&state)).await;
        Ok(match outcome {
            Ok(Ok(value)) => {
                let json = serde_json::to_string_pretty(&value)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                CallToolResult::success(vec![ContentBlock::text(json)])
            }
            Ok(Err(err)) => CallToolResult::error(vec![ContentBlock::text(format!("{err:#}"))]),
            Err(join) => CallToolResult::error(vec![ContentBlock::text(format!("internal error: {join}"))]),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindBooksParams {
    /// Words from the book title, in Hebrew. Every word must appear (in the title or an alternative title).
    pub title: Option<String>,
    /// Words from an author's name, e.g. "קוק" or "שפירא, אברהם".
    pub author: Option<String>,
    /// Exact author id from find_authors.
    pub author_id: Option<i64>,
    /// Earliest print year: Gregorian (1850) or Hebrew Anno Mundi (5610).
    pub year_from: Option<i32>,
    /// Latest print year: Gregorian or Anno Mundi.
    pub year_to: Option<i32>,
    /// Print place, e.g. "וילנא", "ירושלים".
    pub place: Option<String>,
    /// Era (seder hadorot): 1 Mikra and Chazal, 2 Rishonim, 3 Acharonim 5300-5600, 4 Acharonim 5600-5700, 5 contemporary, 6 other.
    pub period: Option<i64>,
    /// Category id from browse_categories (its sub-categories included).
    pub category_id: Option<i64>,
    /// Max results (default 20).
    pub limit: Option<usize>,
    /// Results to skip, for paging.
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BookIdParams {
    /// Book id from the catalog (find_books / browse_categories results).
    pub book_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindAuthorsParams {
    /// Words from the author's name.
    pub name: String,
    /// Max results (default 20).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BrowseCategoriesParams {
    /// Category id to open. Omit for the top level.
    pub category_id: Option<i64>,
    /// Max children and books to list (default 50).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WordStatsParams {
    /// A single word exactly as printed, prefixes included (e.g. "שבת", "והשבת").
    pub word: String,
    /// Also report spelling variants the official search treats as equivalent (default true).
    pub include_alternatives: Option<bool>,
    /// Also list widespread indexed words that contain this text, at least 3 letters (default false).
    pub include_containing: Option<bool>,
    /// Max containing words (default 20).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTextParams {
    /// Words to find, in Hebrew, exactly as printed (prefixes are part of the word: "והאידנא", "בשבת").
    pub query: String,
    /// "near" (default): all words within `distance` words of each other, in any order.
    /// "phrase": the words adjacent and in this order.
    pub mode: Option<String>,
    /// Max distance in words for mode "near" (default 30, like the app).
    pub distance: Option<u32>,
    /// Only these book ids.
    pub book_ids: Option<Vec<i64>>,
    /// Only books whose title contains these words.
    pub title: Option<String>,
    /// Only books by an author whose name contains these words.
    pub author: Option<String>,
    /// Only books by this author id (find_authors).
    pub author_id: Option<i64>,
    /// Only books printed from this year: Gregorian (1850) or Anno Mundi (5610).
    pub year_from: Option<i32>,
    /// Only books printed up to this year.
    pub year_to: Option<i32>,
    /// Only books printed in this place.
    pub place: Option<String>,
    /// Only books of this era (1-6, see find_books).
    pub period: Option<i64>,
    /// Only hits inside this category (browse_categories id, sub-categories included). Categories
    /// map to word ranges inside books, e.g. a commentary's section on one chapter.
    pub category_id: Option<i64>,
    /// Also match the words with one or two prefix letters (ו ה ב כ ל מ ש ד), like the app's
    /// "אותיות שימוש". Default false.
    pub prefixes: Option<bool>,
    /// Leave out books outside the user's subscription (default false: they are returned and marked).
    pub only_subscription: Option<bool>,
    /// Books per response (default 20).
    pub limit: Option<usize>,
    /// Books to skip, for paging.
    pub offset: Option<usize>,
    /// Pages listed per book (default 10), in page order.
    pub pages_per_book: Option<usize>,
}

#[derive(Serialize)]
struct HitPage {
    /// Page order in the book (1-based), for open_in_otzar and read_page.
    page: i64,
    /// Printed page label.
    #[serde(skip_serializing_if = "String::is_empty")]
    label: String,
    /// Scan name.
    scan: String,
    page_id: i64,
    hits: u32,
}

#[derive(Serialize)]
struct SearchHit {
    #[serde(flatten)]
    book: BookSummary,
    hits: u32,
    /// Number of pages with hits; `pages` lists the first ones.
    hit_pages: usize,
    pages: Vec<HitPage>,
}

#[derive(Serialize)]
struct SearchResult {
    query: String,
    prefixes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    category_id: Option<i64>,
    mode: String,
    total_books: usize,
    total_hits: u64,
    offset: usize,
    results: Vec<SearchHit>,
}

#[derive(Serialize)]
struct WordStatsResult {
    word: String,
    found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    books: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    occurrences: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    alternatives: Vec<AlternativeStat>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    containing: Vec<crate::words::WordStat>,
}

#[derive(Serialize)]
struct AlternativeStat {
    word: String,
    /// "variant" (spelling or synonym) or "abbreviation" (abbreviation <-> expansion).
    kind: &'static str,
    /// None for multi-word phrases, which the single-word dictionary cannot count.
    books: Option<i64>,
    occurrences: Option<i64>,
}

#[derive(Serialize)]
struct Status {
    version: &'static str,
    config_file: Option<String>,
    disk: Option<String>,
    disk_version: Option<String>,
    db_version: Option<String>,
    catalog: Option<String>,
    catalog_books: Option<i64>,
    catalog_error: Option<String>,
    /// Full-text index segments in merge order (main, then sub-indexes).
    index_segments: Option<Vec<String>>,
    index_error: Option<String>,
    otzar_exe: String,
    otzar_exe_found: bool,
    subscription_packages: Option<Vec<String>>,
    /// Codes accepted in `[subscription] packages`.
    known_package_codes: Vec<String>,
}

#[tool_router]
impl OtzarServer {
    #[tool(
        description = "Search the Otzar HaHochma catalog (about 163,000 books) by title words, author, print year range, print place or era. Returns book ids, authors, year, place and subscription access.",
        annotations(read_only_hint = true)
    )]
    async fn find_books(&self, Parameters(p): Parameters<FindBooksParams>) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            let query = BookQuery {
                title: p.title,
                author: p.author,
                author_id: p.author_id,
                year_from: p.year_from,
                year_to: p.year_to,
                place: p.place,
                period: p.period,
                category_id: p.category_id,
                limit: st.limit(p.limit, 20),
                offset: p.offset.unwrap_or(0),
            };
            st.with_catalog(|c| c.find_books(&query))
        })
        .await
    }

    #[tool(
        description = "Full catalog record of one book: authors, year, place, alternative titles, works printed inside it with their start pages, other volumes of the set, and subscription access.",
        annotations(read_only_hint = true)
    )]
    async fn get_book(&self, Parameters(p): Parameters<BookIdParams>) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            st.with_catalog(|c| c.get_book(p.book_id))?
                .with_context(|| format!("no book with id {}", p.book_id))
        })
        .await
    }

    #[tool(
        description = "Find authors by name. Returns author ids (use with find_books author_id), era and number of books.",
        annotations(read_only_hint = true)
    )]
    async fn find_authors(&self, Parameters(p): Parameters<FindAuthorsParams>) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            let limit = st.limit(p.limit, 20);
            st.with_catalog(|c| c.find_authors(&p.name, limit))
        })
        .await
    }

    #[tool(
        description = "Browse the Otzar category tree (ספרי יסוד, מפרשים, נושאים, ...). Without category_id lists the top level; with it lists sub-categories and the books linked to that category.",
        annotations(read_only_hint = true)
    )]
    async fn browse_categories(
        &self,
        Parameters(p): Parameters<BrowseCategoriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            let limit = st.limit(p.limit, 50);
            st.with_catalog(|c| c.browse_categories(p.category_id, limit))
        })
        .await
    }

    #[tool(
        description = "How widespread a word is in the full-text index: number of books and total occurrences, its spelling variants, and optionally longer indexed words containing it. Needs the disk. Words are indexed as printed, with prefixes.",
        annotations(read_only_hint = true)
    )]
    async fn word_stats(&self, Parameters(p): Parameters<WordStatsParams>) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            let word = p.word.trim().to_string();
            if word.is_empty() || word.contains(char::is_whitespace) {
                anyhow::bail!("give a single word");
            }
            let limit = st.limit(p.limit, 20);
            st.with_catalog(|c| {
            let alternatives = if p.include_alternatives.unwrap_or(true) { c.alternatives(&word)? } else { vec![] };
            let keep = |b: u32| c.has_book(b as i64);
            st.with_words(|w| {
                let hit = w.stat(&word, &keep)?;
                let mut alt_stats = Vec::new();
                for (alt, kind) in alternatives {
                    let stat = if alt.contains(char::is_whitespace) { None } else { w.stat(&alt, &keep)? };
                    alt_stats.push(AlternativeStat {
                        kind,
                        books: stat.as_ref().map(|s| s.books),
                        occurrences: stat.as_ref().map(|s| s.occurrences),
                        word: alt,
                    });
                }
                let containing = if p.include_containing.unwrap_or(false) {
                    w.containing(&word, limit)?
                } else {
                    vec![]
                };
                Ok(WordStatsResult {
                    found: hit.is_some(),
                    books: hit.as_ref().map(|h| h.books),
                    occurrences: hit.as_ref().map(|h| h.occurrences),
                    word,
                    alternatives: alt_stats,
                    containing,
                })
            })
            })
        })
        .await
    }

    #[tool(
        description = "Full-text search in the scanned books, with the same rules as the official Otzar search. Returns books ranked by number of hits (like the app's 'relevance'), each with the pages where the hits are (printed page label and scan name). One word, or several words: mode 'near' (default, words within 30 words of each other, like the app with הצמדת מילים off) or 'phrase' (adjacent and in order, like הצמדת מילים on). Words are matched exactly as printed; set prefixes=true to include forms with prefix letters. Optional catalog filters and category_id restrict the books and hits. Page text is not available. Needs the disk.",
        annotations(read_only_hint = true)
    )]
    async fn search_text(&self, Parameters(p): Parameters<SearchTextParams>) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            let words: Vec<String> = p.query.split_whitespace().map(str::to_string).collect();
            anyhow::ensure!(!words.is_empty(), "query is empty");
            anyhow::ensure!(words.len() <= 8, "at most 8 words");
            let mode = match p.mode.as_deref().unwrap_or("near") {
                "phrase" => Mode::Phrase,
                "near" => Mode::Near(p.distance.unwrap_or(search::DEFAULT_DISTANCE).clamp(1, 1000)),
                other => anyhow::bail!("unknown mode '{other}': use 'near' or 'phrase'"),
            };
            let limit = st.limit(p.limit, 20);
            let offset = p.offset.unwrap_or(0);
            let pages_per_book = p.pages_per_book.unwrap_or(10).clamp(1, 50);
            let prefixes = p.prefixes.unwrap_or(false);
            let forms: Vec<Vec<String>> = words
                .iter()
                .map(|w| if prefixes { search::with_prefixes(w) } else { vec![w.clone()] })
                .collect();
            let query = BookQuery {
                title: p.title,
                author: p.author,
                author_id: p.author_id,
                year_from: p.year_from,
                year_to: p.year_to,
                place: p.place,
                period: p.period,
                // A category restricts hits to its word ranges, handled below.
                category_id: None,
                limit: 0,
                offset: 0,
            };
            st.with_catalog(|c| {
                let ranges = p.category_id.map(|id| c.category_ranges(id)).transpose()?;
                let mut restrict = c.book_ids(&query)?;
                if let Some(ids) = &p.book_ids {
                    let given: std::collections::HashSet<i64> = ids.iter().copied().collect();
                    restrict = Some(match restrict {
                        Some(r) => r.intersection(&given).copied().collect(),
                        None => given,
                    });
                }
                let only_subscription = p.only_subscription.unwrap_or(false);
                let keep = |b: u32| {
                    let id = b as i64;
                    c.has_book(id)
                        && restrict.as_ref().is_none_or(|r| r.contains(&id))
                        && (!only_subscription || c.access(id).status != "not_in_subscription")
                };
                st.with_words(|w| {
                    let outcome = search::run(w, &forms, mode, &keep, ranges.as_ref())?;
                    let mut results = Vec::new();
                    for &(book, hits) in outcome.ranked.iter().skip(offset).take(limit) {
                        let Some(summary) = c.summary_by_id(book as i64)? else { continue };
                        let anchors = outcome.anchors(w, book)?;
                        let mut by_page: BTreeMap<i64, HitPage> = BTreeMap::new();
                        if let Some(pages) = w.pages.book(book as i64)? {
                            for pos in anchors {
                                if let Some(page) = pages.page_for(pos) {
                                    by_page
                                        .entry(page.position)
                                        .or_insert_with(|| HitPage {
                                            page: page.position,
                                            label: page.label.clone(),
                                            scan: page.scan.clone(),
                                            page_id: page.id,
                                            hits: 0,
                                        })
                                        .hits += 1;
                                }
                            }
                        }
                        let hit_pages = by_page.len();
                        results.push(SearchHit {
                            book: summary,
                            hits,
                            hit_pages,
                            pages: by_page.into_values().take(pages_per_book).collect(),
                        });
                    }
                    Ok(SearchResult {
                        prefixes,
                        category_id: p.category_id,
                        query: words.join(" "),
                        mode: match mode {
                            Mode::Phrase => "phrase".into(),
                            Mode::Near(d) => format!("near {d}"),
                        },
                        total_books: outcome.ranked.len(),
                        total_hits: outcome.total_hits(),
                        offset,
                        results,
                    })
                })
            })
        })
        .await
    }

    #[tool(
        description = "Open a book in the official Otzar HaHochma application on the user's screen. Use when the user wants to see the scanned page.",
        annotations(read_only_hint = false, destructive_hint = false, open_world_hint = false)
    )]
    async fn open_in_otzar(&self, Parameters(p): Parameters<BookIdParams>) -> Result<CallToolResult, ErrorData> {
        self.run(move |st| {
            let exe = st.config.otzar_exe();
            anyhow::ensure!(exe.is_file(), "otzar.exe not found at {} (set otzar_exe in otzar-mcp.toml)", exe.display());
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg(format!("BOOK={}", p.book_id))
                .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
                // Never let the child inherit our stdio: it carries the MCP protocol.
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            {
                use std::os::windows::process::CommandExt;
                const DETACHED_PROCESS: u32 = 0x0000_0008;
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
                cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            }
            cmd.spawn().with_context(|| format!("starting {}", exe.display()))?;
            Ok(serde_json::json!({ "opened": true, "book_id": p.book_id }))
        })
        .await
    }

    #[tool(
        description = "Diagnostics: where the disk, catalog and app were found, data versions, and the subscription configuration. Call this first when another tool fails.",
        annotations(read_only_hint = true)
    )]
    async fn server_status(&self) -> Result<CallToolResult, ErrorData> {
        self.run(|st| {
            let disk = st.config.find_disk();
            let catalog = st.with_catalog(|c| Ok((c.path.clone(), c.book_count()?, c.package_codes()?)));
            let (catalog_path, catalog_books, codes, catalog_error) = match catalog {
                Ok((path, count, codes)) => (Some(path), Some(count), codes, None),
                Err(e) => (st.config.catalog_db(disk.as_deref()), None, vec![], Some(format!("{e:#}"))),
            };
            let (index_segments, index_error) = match st.with_words(|w| Ok(w.segment_names())) {
                Ok(names) => (Some(names), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            };
            let exe = st.config.otzar_exe();
            Ok(Status {
                index_segments,
                index_error,
                version: env!("CARGO_PKG_VERSION"),
                config_file: st.config_path.as_ref().map(|p| p.display().to_string()),
                disk_version: disk.as_deref().and_then(|d| config::read_marker(d, r"data\disk-ver.txt")),
                db_version: disk.as_deref().and_then(|d| config::read_marker(d, r"data\db_ver.txt")),
                disk: disk.map(|d| d.display().to_string()),
                catalog: catalog_path.map(|p| p.display().to_string()),
                catalog_books,
                catalog_error,
                otzar_exe_found: exe.is_file(),
                otzar_exe: exe.display().to_string(),
                subscription_packages: st.config.subscription.packages.clone(),
                known_package_codes: codes.into_iter().map(|(code, name)| format!("{code} ({name})")).collect(),
            })
        })
        .await
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResearchArgs {
    /// The research question (Hebrew or English).
    pub question: String,
}

const RESEARCH_GUIDE: &str = "\
Research this question in the user's Otzar HaHochma library with the otzar tools.

Method:
1. Plan the Hebrew search terms: the key words as printed, likely spellings and abbreviations
   (word_stats shows how common a word is and its linked variants and abbreviations).
2. search_text for each term. Use mode 'phrase' for fixed expressions and 'near' for ideas
   expressed by several words. Set prefixes=true when the word may carry ו ה ב כ ל מ ש ד.
   Narrow with author, years, period, place, book_ids or category_id when results are too broad.
3. Use find_books, get_book and browse_categories to understand the sources: who wrote them,
   when, and where a work is printed inside another book.
4. Page text is not available through the tools. Do not invent quotations. Cite book name,
   book id, volume and the printed page label (and scan name) for every source, and offer
   open_in_otzar so the user can read the page.
5. Mark sources outside the subscription (access.status 'not_in_subscription') with the
   package name.
6. Answer in the user's language, with the sources grouped by era (period), earliest first.

Question: ";

#[rmcp::prompt_router]
impl OtzarServer {
    #[rmcp::prompt(
        name = "research",
        description = "Guide Claude through researching a question in the Otzar HaHochma library and citing exact sources."
    )]
    async fn research(&self, Parameters(args): Parameters<ResearchArgs>) -> Vec<rmcp::model::PromptMessage> {
        vec![rmcp::model::PromptMessage::new_text(
            rmcp::model::Role::User,
            format!("{RESEARCH_GUIDE}{}", args.question),
        )]
    }
}

#[tool_handler(
    name = "otzar-mcp",
    instructions = "Local, read-only access to the user's Otzar HaHochma (אוצר החכמה) library: about 163,000 scanned Jewish books. Query in Hebrew. Use find_books / find_authors / browse_categories for the catalog, get_book for one record, word_stats to see how common a word is in the full text, and open_in_otzar to show a book in the official app. Always cite book name and id. Each book carries access.status: 'included', 'not_in_subscription' (name the missing package to the user) or 'unknown'. If a tool fails, call server_status."
)]
#[rmcp::prompt_handler]
impl ServerHandler for OtzarServer {}
