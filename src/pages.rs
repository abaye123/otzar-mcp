//! Page lists per book: word position -> page.
//!
//! Sources, in lookup order (docs/DATA-FORMATS.md, "Pages and word positions"):
//! 1. `data\books_data\<book_id>.bin`         one gzip member
//! 2. `data\books_data\booksdata_<ts>.bin`    bundles, newest first
//! 3. `data\books_data_info.bin` + `data\books_data.bin`

use anyhow::Context;
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::windows::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Reads exactly `buf.len()` bytes at `offset` without moving a shared cursor.
pub fn read_at(file: &File, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
    let mut done = 0;
    while done < buf.len() {
        let n = file.seek_read(&mut buf[done..], offset + done as u64)?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "short read"));
        }
        done += n;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Page {
    /// Global page id (same space as `categorybooks.pageId`).
    pub id: i64,
    /// Page order in the book, 1-based.
    pub position: i64,
    /// Printed page label, e.g. "קה".
    pub label: String,
    /// Scan name, e.g. "P0012L".
    pub scan: String,
    first_word: u32,
}

/// Pages of one book with a word range, sorted by first word.
pub struct BookPages {
    pages: Vec<Page>,
}

impl BookPages {
    /// The page holding word `position`. Positions in a gap between pages belong to the
    /// preceding page.
    pub fn page_for(&self, position: u32) -> Option<&Page> {
        let idx = self.pages.partition_point(|p| p.first_word <= position);
        if idx == 0 {
            self.pages.first()
        } else {
            self.pages.get(idx - 1)
        }
    }

    #[cfg(test)]
    fn from_ranges(ranges: &[(u32, u32)]) -> Self {
        let mut pages: Vec<Page> = ranges
            .iter()
            .enumerate()
            .map(|(i, (first, _num))| Page {
                id: i as i64,
                position: i as i64 + 1,
                label: String::new(),
                scan: String::new(),
                first_word: *first,
            })
            .collect();
        pages.sort_by_key(|p| p.first_word);
        Self { pages }
    }
}

#[derive(Deserialize)]
struct PageJson {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    position: i64,
    #[serde(default)]
    pagedata: Vec<PageDataJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageDataJson {
    #[serde(default)]
    first_word: i64,
    #[serde(default)]
    num_words: i64,
    #[serde(default)]
    letter: Option<serde_json::Value>,
}

fn parse_book(gz: &[u8]) -> anyhow::Result<BookPages> {
    let mut json = Vec::new();
    GzDecoder::new(gz).read_to_end(&mut json).context("decompressing book data")?;
    let parts: Vec<serde_json::Value> = serde_json::from_slice(&json).context("parsing book data")?;
    let list = parts.get(1).cloned().unwrap_or(serde_json::Value::Array(vec![]));
    let raw: Vec<PageJson> = serde_json::from_value(list).context("parsing page list")?;
    let mut pages: Vec<Page> = raw
        .into_iter()
        .filter_map(|p| {
            let data = p.pagedata.first()?;
            if data.num_words <= 0 || data.first_word < 0 {
                return None;
            }
            let label = match &data.letter {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            Some(Page {
                id: p.id,
                position: p.position,
                label,
                scan: p.name,
                first_word: data.first_word as u32,
            })
        })
        .collect();
    // Page order is not always word order: sort for binary search.
    pages.sort_by_key(|p| p.first_word);
    Ok(BookPages { pages })
}

enum Source {
    Single(PathBuf),
    Bundle { file: Arc<File>, offset: u64, size: u32 },
}

pub struct PageStore {
    info: File,
    data: File,
    extra: HashMap<i64, Source>,
    cache: Mutex<HashMap<i64, Arc<BookPages>>>,
}

const CACHE_LIMIT: usize = 512;

impl PageStore {
    pub fn open(disk_root: &Path) -> anyhow::Result<Self> {
        let data_dir = disk_root.join("data");
        let info = File::open(data_dir.join("books_data_info.bin")).context("opening books_data_info.bin")?;
        let data = File::open(data_dir.join("books_data.bin")).context("opening books_data.bin")?;

        let mut extra = HashMap::new();
        let updates = data_dir.join("books_data");
        if let Ok(entries) = std::fs::read_dir(&updates) {
            let mut bundles = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(stamp) = name.strip_prefix("booksdata_").and_then(|s| s.strip_suffix(".bin")) {
                    bundles.push((stamp.parse::<u64>().unwrap_or(0), entry.path()));
                } else if let Some(id) = name.strip_suffix(".bin").and_then(|s| s.parse::<i64>().ok()) {
                    extra.insert(id, Source::Single(entry.path()));
                }
            }
            bundles.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
            for (_, path) in bundles {
                let file = Arc::new(File::open(&path).with_context(|| format!("opening {}", path.display()))?);
                let mut head = [0u8; 4];
                read_at(&file, 0, &mut head)?;
                let count = u32::from_le_bytes(head) as usize;
                let mut table = vec![0u8; count * 16];
                read_at(&file, 4, &mut table)?;
                for rec in table.chunks_exact(16) {
                    let id = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as i64;
                    let offset = u64::from_le_bytes(rec[4..12].try_into().unwrap());
                    let size = u32::from_le_bytes(rec[12..16].try_into().unwrap());
                    extra.entry(id).or_insert(Source::Bundle { file: file.clone(), offset, size });
                }
            }
        }
        Ok(Self { info, data, extra, cache: Mutex::new(HashMap::new()) })
    }

    fn load(&self, book_id: i64) -> anyhow::Result<Option<BookPages>> {
        let bytes = match self.extra.get(&book_id) {
            Some(Source::Single(path)) => std::fs::read(path)?,
            Some(Source::Bundle { file, offset, size }) => {
                let mut buf = vec![0u8; *size as usize];
                read_at(file, *offset, &mut buf)?;
                buf
            }
            None => {
                let mut entry = [0u8; 12];
                if read_at(&self.info, book_id as u64 * 12, &mut entry).is_err() {
                    return Ok(None);
                }
                let offset = u64::from_le_bytes(entry[0..8].try_into().unwrap());
                let size = u32::from_le_bytes(entry[8..12].try_into().unwrap());
                if size == 0 {
                    return Ok(None);
                }
                let mut buf = vec![0u8; size as usize];
                read_at(&self.data, offset, &mut buf)?;
                buf
            }
        };
        Ok(Some(parse_book(&bytes).with_context(|| format!("book {book_id}"))?))
    }

    /// Pages of a book, cached. `None` when the disk has no page data for it.
    pub fn book(&self, book_id: i64) -> anyhow::Result<Option<Arc<BookPages>>> {
        if let Some(hit) = self.cache.lock().ok().and_then(|c| c.get(&book_id).cloned()) {
            return Ok(Some(hit));
        }
        let Some(pages) = self.load(book_id)? else {
            return Ok(None);
        };
        let pages = Arc::new(pages);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(book_id, pages.clone());
        }
        Ok(Some(pages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_positions_to_pages() {
        // Second page listed first on purpose: lists are not always in word order.
        let book = BookPages::from_ranges(&[(100, 50), (0, 90), (150, 10)]);
        assert_eq!(book.page_for(0).unwrap().first_word, 0);
        assert_eq!(book.page_for(89).unwrap().first_word, 0);
        assert_eq!(book.page_for(95).unwrap().first_word, 0, "gap belongs to the preceding page");
        assert_eq!(book.page_for(120).unwrap().first_word, 100);
        assert_eq!(book.page_for(500).unwrap().first_word, 150);
    }
}
