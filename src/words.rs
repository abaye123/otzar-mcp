//! Word dictionary of the full-text index on the disk.
//!
//! * `searcher\fs.db`   - every indexed token: `words(w, b = books, l = occurrences, p = offset in words.bin)`
//! * `searcher\tiny.db` - tokens found in at least ~51 books, with an FTS5 trigram index for substring lookup
//!
//! The posting lists themselves live in `searcher\words.bin` (phase 2, see docs/DATA-FORMATS.md).

use crate::catalog::open_read_only;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct WordStat {
    pub word: String,
    pub books: i64,
    pub occurrences: i64,
    /// Byte offset of the posting list in `words.bin` (used by phase 2).
    #[serde(skip)]
    #[allow(dead_code, reason = "read by the phase 2 posting-list reader")]
    pub offset: i64,
}

pub struct WordIndex {
    fs: Connection,
    tiny: Connection,
    pub root: PathBuf,
}

impl WordIndex {
    pub fn open(disk_root: &Path) -> anyhow::Result<Self> {
        let searcher = disk_root.join("searcher");
        Ok(Self {
            fs: open_read_only(&searcher.join("fs.db"))?,
            tiny: open_read_only(&searcher.join("tiny.db"))?,
            root: disk_root.to_path_buf(),
        })
    }

    /// Exact lookup. The index stores tokens as printed, prefixes included (ו, ה, ב, ...).
    pub fn lookup(&self, word: &str) -> anyhow::Result<Option<WordStat>> {
        Ok(self
            .fs
            .prepare_cached("select b, l, p from words where w = ?1 order by b desc limit 1")?
            .query_row([word], |r| {
                Ok(WordStat { word: word.to_string(), books: r.get(0)?, occurrences: r.get(1)?, offset: r.get(2)? })
            })
            .optional()?)
    }

    /// Tokens that contain `fragment` (at least 3 characters), most widespread first.
    /// Only covers tokens present in `tiny.db`, i.e. those found in many books.
    pub fn containing(&self, fragment: &str, limit: usize) -> anyhow::Result<Vec<WordStat>> {
        if fragment.chars().count() < 3 {
            anyhow::bail!("substring search needs at least 3 characters");
        }
        let quoted = format!("\"{}\"", fragment.replace('"', "\"\""));
        let mut stmt = self.tiny.prepare(
            "select w.w, w.b, w.l, w.p from words_fts f join words w on w.rowid = f.rowid
             where words_fts match ?1 order by w.b desc limit ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![quoted, limit as i64], |r| {
            Ok(WordStat { word: r.get(0)?, books: r.get(1)?, occurrences: r.get(2)?, offset: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
