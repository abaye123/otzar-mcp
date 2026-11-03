//! The full-text index on the disk: `searcher\` plus the sub-indexes in `searcher\subindexs\`.
//!
//! Each segment has `fs.db` (dictionary: `words(w, b, l, p)`) and `words.bin` (posting lists).
//! Segments are applied in order (main, then sub-indexes by name); for a book present in
//! several segments the later one replaces the earlier. This reproduces the official engine
//! exactly for single words (docs/DATA-FORMATS.md).

use crate::catalog::open_read_only;
use crate::pages::{PageStore, read_at};
use anyhow::Context;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Positions of one word, per book: book id -> ascending word positions.
pub type Positions = HashMap<u32, Vec<u32>>;

struct Segment {
    name: String,
    fs: Connection,
    bin: File,
}

#[derive(Clone, Copy)]
struct Entry {
    books: u32,
    occurrences: u32,
    offset: u64,
}

/// One book in a posting list: `[start, end)` into the occurrence array.
#[derive(Clone, Copy)]
struct DirEntry {
    book: u32,
    start: u32,
    end: u32,
}

#[derive(Debug, Serialize)]
pub struct WordStat {
    pub word: String,
    pub books: i64,
    pub occurrences: i64,
}

pub struct FullText {
    segments: Vec<Segment>,
    tiny: Connection,
    pub pages: PageStore,
    pub root: PathBuf,
}

fn le_u32s(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Above this many wanted books, read the whole occurrence array in one go instead of
/// one read per book (sequential reads are much faster on the USB disk).
const SLICE_READ_LIMIT: usize = 256;

impl FullText {
    pub fn open(disk_root: &Path) -> anyhow::Result<Self> {
        let searcher = disk_root.join("searcher");
        let mut dirs = vec![("main".to_string(), searcher.clone())];
        let sub = searcher.join("subindexs");
        if let Ok(entries) = std::fs::read_dir(&sub) {
            let mut names: Vec<(String, PathBuf)> = entries
                .flatten()
                .filter(|e| e.path().join("fs.db").is_file() && e.path().join("words.bin").is_file())
                .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                .collect();
            names.sort();
            dirs.extend(names);
        }
        let mut segments = Vec::new();
        for (name, dir) in dirs {
            segments.push(Segment {
                fs: open_read_only(&dir.join("fs.db"))?,
                bin: File::open(dir.join("words.bin")).with_context(|| format!("opening {}", dir.display()))?,
                name,
            });
        }
        Ok(Self {
            segments,
            tiny: open_read_only(&searcher.join("tiny.db"))?,
            pages: PageStore::open(disk_root)?,
            root: disk_root.to_path_buf(),
        })
    }

    pub fn segment_names(&self) -> Vec<String> {
        self.segments.iter().map(|s| s.name.clone()).collect()
    }

    fn entry(seg: &Segment, word: &str) -> anyhow::Result<Option<Entry>> {
        Ok(seg
            .fs
            .prepare_cached("select b, l, p from words where w = ?1 order by b desc limit 1")?
            .query_row([word], |r| {
                Ok(Entry {
                    books: r.get::<_, i64>(0)? as u32,
                    occurrences: r.get::<_, i64>(1)? as u32,
                    offset: r.get::<_, i64>(2)? as u64,
                })
            })
            .optional()?)
    }

    fn directory(seg: &Segment, e: Entry) -> anyhow::Result<Vec<DirEntry>> {
        let mut header = [0u8; 8];
        read_at(&seg.bin, e.offset, &mut header)?;
        let books = u32::from_le_bytes(header[0..4].try_into().unwrap());
        anyhow::ensure!(
            books == e.books,
            "words.bin in segment {} does not match fs.db (offset {})",
            seg.name,
            e.offset
        );
        let mut raw = vec![0u8; e.books as usize * 12];
        read_at(&seg.bin, e.offset + 8, &mut raw)?;
        Ok(le_u32s(&raw)
            .chunks_exact(3)
            .map(|t| DirEntry { book: t[0], end: t[1], start: t[2] })
            .collect())
    }

    /// Per-book occurrence counts of `word`, merged across segments, books kept by `keep`.
    /// Reads only the posting directories, never the positions.
    pub fn counts(&self, word: &str, keep: &dyn Fn(u32) -> bool) -> anyhow::Result<HashMap<u32, u32>> {
        let mut out = HashMap::new();
        for seg in &self.segments {
            if let Some(e) = Self::entry(seg, word)? {
                for d in Self::directory(seg, e)? {
                    out.insert(d.book, d.end - d.start);
                }
            }
        }
        out.retain(|book, _| keep(*book));
        Ok(out)
    }

    /// Positions of `word` per book, merged across segments, for books kept by `keep`.
    pub fn positions(&self, word: &str, keep: &dyn Fn(u32) -> bool) -> anyhow::Result<Positions> {
        let mut out: Positions = HashMap::new();
        for seg in &self.segments {
            let Some(e) = Self::entry(seg, word)? else { continue };
            let dir = Self::directory(seg, e)?;
            let wanted: Vec<DirEntry> = dir.into_iter().filter(|d| keep(d.book)).collect();
            if wanted.is_empty() {
                continue;
            }
            let occ_base = e.offset + 8 + e.books as u64 * 12;
            if wanted.len() <= SLICE_READ_LIMIT {
                for d in wanted {
                    let mut raw = vec![0u8; (d.end - d.start) as usize * 4];
                    read_at(&seg.bin, occ_base + d.start as u64 * 4, &mut raw)?;
                    out.insert(d.book, le_u32s(&raw));
                }
            } else {
                let mut raw = vec![0u8; e.occurrences as usize * 4];
                read_at(&seg.bin, occ_base, &mut raw)?;
                let all = le_u32s(&raw);
                for d in wanted {
                    out.insert(d.book, all[d.start as usize..d.end as usize].to_vec());
                }
            }
        }
        Ok(out)
    }

    /// Merged statistics for a word across all segments.
    pub fn stat(&self, word: &str, keep: &dyn Fn(u32) -> bool) -> anyhow::Result<Option<WordStat>> {
        let counts = self.counts(word, keep)?;
        if counts.is_empty() {
            return Ok(None);
        }
        Ok(Some(WordStat {
            word: word.to_string(),
            books: counts.len() as i64,
            occurrences: counts.values().map(|c| *c as i64).sum(),
        }))
    }

    /// Tokens that contain `fragment` (at least 3 characters), most widespread first.
    /// Uses the main index's `tiny.db`, which holds only tokens found in many books; the
    /// numbers are the main index's, without sub-indexes.
    pub fn containing(&self, fragment: &str, limit: usize) -> anyhow::Result<Vec<WordStat>> {
        if fragment.chars().count() < 3 {
            anyhow::bail!("substring search needs at least 3 characters");
        }
        let quoted = format!("\"{}\"", fragment.replace('"', "\"\""));
        let mut stmt = self.tiny.prepare(
            "select w.w, w.b, w.l from words_fts f join words w on w.rowid = f.rowid
             where words_fts match ?1 order by w.b desc limit ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![quoted, limit as i64], |r| {
            Ok(WordStat { word: r.get(0)?, books: r.get(1)?, occurrences: r.get(2)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
