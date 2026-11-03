//! Full-text matching with the official engine's rules (docs/DATA-FORMATS.md):
//!
//! * one word: every occurrence is a hit;
//! * `Phrase` ("הצמדת מילים" on): word i at position x + i;
//! * `Near(d)` (default, d = 30): two words pair one-to-one when |a - b| <= d.
//!   For three or more words a hit is an occurrence of the first word with every other word
//!   within d (an extension, not verified against the engine).

use crate::words::{FullText, Positions};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Near(u32),
    Phrase,
}

/// Default distance of the official engine when "הצמדת מילים" is off.
pub const DEFAULT_DISTANCE: u32 = 30;

fn contains(sorted: &[u32], value: u32) -> bool {
    sorted.binary_search(&value).is_ok()
}

fn any_within(sorted: &[u32], center: u32, distance: u32) -> bool {
    let lo = center.saturating_sub(distance);
    let idx = sorted.partition_point(|&p| p < lo);
    sorted.get(idx).is_some_and(|&p| p <= center.saturating_add(distance))
}

/// Match positions (anchors) in one book. `lists` are ascending positions, one per query word.
pub fn match_book(lists: &[&[u32]], mode: Mode) -> Vec<u32> {
    match lists {
        [] => vec![],
        [only] => only.to_vec(),
        [first, rest @ ..] => match mode {
            Mode::Phrase => first
                .iter()
                .copied()
                .filter(|&x| rest.iter().enumerate().all(|(i, l)| contains(l, x + i as u32 + 1)))
                .collect(),
            Mode::Near(d) if rest.len() == 1 => {
                let (a, b) = (*first, rest[0]);
                let (mut i, mut j) = (0, 0);
                let mut anchors = Vec::new();
                while i < a.len() && j < b.len() {
                    if a[i].abs_diff(b[j]) <= d {
                        anchors.push(a[i].min(b[j]));
                        i += 1;
                        j += 1;
                    } else if a[i] < b[j] {
                        i += 1;
                    } else {
                        j += 1;
                    }
                }
                anchors
            }
            Mode::Near(d) => first
                .iter()
                .copied()
                .filter(|&x| rest.iter().all(|l| any_within(l, x, d)))
                .collect(),
        },
    }
}

pub struct Outcome {
    /// (book, hits), most hits first, then by book id.
    pub ranked: Vec<(u32, u32)>,
    /// Match positions per book for multi-word queries. Single-word queries leave this empty
    /// and fetch positions only for the books shown.
    anchors: HashMap<u32, Vec<u32>>,
    words: Vec<String>,
}

impl Outcome {
    pub fn total_hits(&self) -> u64 {
        self.ranked.iter().map(|(_, h)| *h as u64).sum()
    }

    /// Match positions in one book.
    pub fn anchors(&self, ft: &FullText, book: u32) -> anyhow::Result<Vec<u32>> {
        if let Some(a) = self.anchors.get(&book) {
            return Ok(a.clone());
        }
        let mut positions = ft.positions(&self.words[0], &|b| b == book)?;
        Ok(positions.remove(&book).unwrap_or_default())
    }
}

pub fn run(ft: &FullText, words: &[String], mode: Mode, keep: &dyn Fn(u32) -> bool) -> anyhow::Result<Outcome> {
    anyhow::ensure!(!words.is_empty(), "empty query");
    let mut ranked: Vec<(u32, u32)>;
    let mut anchors = HashMap::new();

    if words.len() == 1 {
        ranked = ft.counts(&words[0], keep)?.into_iter().collect();
    } else {
        // Books that contain every word, from the directories alone; only then read positions.
        let mut common: Option<HashSet<u32>> = None;
        for word in words {
            let books: HashSet<u32> = ft.counts(word, keep)?.into_keys().collect();
            common = Some(match common {
                None => books,
                Some(c) => c.intersection(&books).copied().collect(),
            });
        }
        let common = common.unwrap_or_default();
        let lists: Vec<Positions> = words
            .iter()
            .map(|w| ft.positions(w, &|b| common.contains(&b)))
            .collect::<anyhow::Result<_>>()?;
        ranked = Vec::new();
        for &book in &common {
            let per_word: Option<Vec<&[u32]>> = lists.iter().map(|l| l.get(&book).map(Vec::as_slice)).collect();
            let Some(per_word) = per_word else { continue };
            let found = match_book(&per_word, mode);
            if !found.is_empty() {
                ranked.push((book, found.len() as u32));
                anchors.insert(book, found);
            }
        }
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(Outcome { ranked, anchors, words: words.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_needs_adjacent_words_in_order() {
        let a = [10, 20, 30];
        let b = [11, 19, 40];
        assert_eq!(match_book(&[&a, &b], Mode::Phrase), vec![10]);
        let c = [12];
        assert_eq!(match_book(&[&a, &b, &c], Mode::Phrase), vec![10]);
    }

    #[test]
    fn near_pairs_one_to_one() {
        // 0 and 5 pair; 100 pairs with 120; 300 has no partner within 30.
        let a = [0, 100, 300];
        let b = [5, 6, 120];
        assert_eq!(match_book(&[&a, &b], Mode::Near(30)), vec![0, 100]);
        // Either order counts.
        let c = [40];
        let d = [20];
        assert_eq!(match_book(&[&c, &d], Mode::Near(30)), vec![20]);
    }

    #[test]
    fn near_three_words_anchor_on_first() {
        let a = [50, 500];
        let b = [60];
        let c = [30, 900];
        assert_eq!(match_book(&[&a, &b, &c], Mode::Near(30)), vec![50]);
    }

    #[test]
    fn single_word_returns_all_positions() {
        let a = [1, 2, 3];
        assert_eq!(match_book(&[&a], Mode::Phrase), vec![1, 2, 3]);
    }
}
