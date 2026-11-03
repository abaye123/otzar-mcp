//! Read-only access to the catalog database (`data.db`, SQLite).
//!
//! Table semantics are documented in `docs/DATA-FORMATS.md`.

use crate::config::Subscription;
use crate::hebrew;
use anyhow::Context;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params, types::Value};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Package codes that look like content filters or base tiers, not purchasable add-ons.
/// Assumption to verify with the publisher (see docs/SPEC.md, open questions).
const NON_LICENSE_CODES: &[&str] = &["", "ALL", "SCHOOL", "FREE", "NO_BT"];

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    /// "package" (`bookspackages`) or "permission" (`bookspermissions`).
    pub kind: &'static str,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Access {
    /// "included", "not_in_subscription" or "unknown".
    pub status: &'static str,
    /// Hebrew names of the restricted groups the book belongs to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Author {
    pub id: i64,
    pub name: String,
    /// "ראשי" (primary) or "משני" (secondary).
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct BookSummary {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub volume: String,
    pub authors: Vec<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_ce: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    pub access: Access,
}

#[derive(Debug, Serialize)]
pub struct SubBook {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    pub start_page: i64,
}

#[derive(Debug, Serialize)]
pub struct VolumeRef {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub volume: String,
}

#[derive(Debug, Serialize)]
pub struct BookDetails {
    #[serde(flatten)]
    pub summary: BookSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub additional_names: Vec<String>,
    /// Works printed inside this book, with the page where each starts.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sub_books: Vec<SubBook>,
    /// Other volumes of the same set.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub volume_set: Vec<VolumeRef>,
    /// Catalog version the book was added in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_in_version: Option<f64>,
    /// Newer catalog id that replaces this book, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<i64>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Serialize)]
pub struct AuthorHit {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    pub books: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryNode {
    pub id: i64,
    pub title: String,
    pub kind: String,
    pub children: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryBook {
    pub book_id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub volume: String,
    /// Global page id inside the Otzar data (not a printed page number).
    pub first_page_id: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryListing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<CategoryNode>,
    pub children: Vec<CategoryNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub books: Vec<CategoryBook>,
}

#[derive(Debug, Default)]
pub struct BookQuery {
    pub title: Option<String>,
    pub author: Option<String>,
    pub author_id: Option<i64>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub place: Option<String>,
    pub period: Option<i64>,
    /// Category id; includes all sub-categories.
    pub category_id: Option<i64>,
    pub limit: usize,
    pub offset: usize,
}

/// A category and all its descendants (`union` guards against cycles).
const CATEGORY_TREE: &str = "with recursive d(id) as (
    select ?1 union select c.id from categories c join d on c.parentId = d.id)";

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub total: i64,
    pub offset: usize,
    pub items: Vec<T>,
}

pub struct Catalog {
    conn: Connection,
    pub path: PathBuf,
    /// years.id -> (label, parsed Anno Mundi year)
    years: HashMap<i64, (String, Option<i32>)>,
    /// sederdorots.id -> title
    periods: HashMap<i64, String>,
    /// bookId -> restricted and non-restricted groups
    groups: HashMap<i64, Vec<Group>>,
    allowed: Option<Vec<String>>,
    /// Every book id in the catalog. The full-text index still lists deleted books.
    books: HashSet<i64>,
}

/// Book restriction built from catalog filters.
enum Filter {
    /// No filter given.
    All,
    /// Filters given, nothing can match (e.g. no year label in the range).
    Nothing,
    /// SQL condition on `system_books b` and its parameters.
    Sql(String, Vec<Value>),
}

pub fn open_read_only(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("opening {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

fn like(word: &str) -> String {
    let escaped = word.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

impl Catalog {
    pub fn open(path: &Path, subscription: &Subscription) -> anyhow::Result<Self> {
        let conn = open_read_only(path)?;

        let mut years = HashMap::new();
        {
            let mut stmt = conn.prepare("select id, name from years")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (id, label) = row?;
                let label = label.trim_start_matches('\u{feff}').to_string();
                let am = hebrew::parse_year(&label);
                years.insert(id, (label, am));
            }
        }

        let mut periods = HashMap::new();
        {
            let mut stmt = conn.prepare("select id, title from sederdorots")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (id, title) = row?;
                periods.insert(id, title);
            }
        }

        let mut groups: HashMap<i64, Vec<Group>> = HashMap::new();
        // In both tables `title` holds the code (e.g. "KOOK") and `name` the Hebrew name.
        for (kind, sql) in [
            (
                "package",
                "select bp.bookId, p.title, p.name from bookspackages bp join packages p on p.id = bp.permissionId",
            ),
            (
                "permission",
                "select bp.bookId, p.title, p.name from bookspermissions bp join permissions p on p.id = bp.permissionId",
            ),
        ] {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ))
            })?;
            for row in rows {
                let (book, code, name) = row?;
                let list = groups.entry(book).or_default();
                if !list.iter().any(|g| g.code == code && g.kind == kind) {
                    list.push(Group { kind, code, name });
                }
            }
        }

        let allowed = subscription
            .packages
            .as_ref()
            .map(|codes| codes.iter().map(|c| c.trim().to_uppercase()).collect());

        let books = {
            let mut stmt = conn.prepare("select id from system_books")?;
            stmt.query_map([], |r| r.get::<_, i64>(0))?
                .collect::<Result<HashSet<_>, _>>()?
        };

        Ok(Self {
            conn,
            path: path.to_path_buf(),
            years,
            periods,
            groups,
            allowed,
            books,
        })
    }

    pub fn has_book(&self, id: i64) -> bool {
        self.books.contains(&id)
    }

    /// Book ids matching the catalog filters of `q`; `None` when `q` has no filter.
    pub fn book_ids(&self, q: &BookQuery) -> anyhow::Result<Option<HashSet<i64>>> {
        match self.filter(q)? {
            Filter::All => Ok(None),
            Filter::Nothing => Ok(Some(HashSet::new())),
            Filter::Sql(cond, args) => {
                let mut stmt = self.conn.prepare(&format!("select b.id from system_books b where {cond}"))?;
                let ids = stmt
                    .query_map(rusqlite::params_from_iter(args.iter()), |r| r.get::<_, i64>(0))?
                    .collect::<Result<HashSet<_>, _>>()?;
                Ok(Some(ids))
            }
        }
    }

    pub fn book_count(&self) -> anyhow::Result<i64> {
        Ok(self
            .conn
            .query_row("select count(*) from system_books", [], |r| r.get(0))?)
    }

    pub fn package_codes(&self) -> anyhow::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "select title, name from packages where title != '' union select title, name from permissions where title != '' order by 1",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn access(&self, book_id: i64) -> Access {
        let restricted: Vec<&Group> = self
            .groups
            .get(&book_id)
            .map(|list| {
                list.iter()
                    .filter(|g| !NON_LICENSE_CODES.contains(&g.code.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        if restricted.is_empty() {
            return Access { status: "included", groups: vec![] };
        }
        // A book often appears both as a package and as a permission with the same name.
        let mut names: Vec<String> = Vec::new();
        for group in &restricted {
            if !names.contains(&group.name) {
                names.push(group.name.clone());
            }
        }
        let status = match &self.allowed {
            None => "unknown",
            Some(allowed) if restricted.iter().any(|g| allowed.contains(&g.code.to_uppercase())) => {
                "included"
            }
            Some(_) => "not_in_subscription",
        };
        Access { status, groups: names }
    }

    fn year_label(&self, from: i64, to: i64) -> (Option<String>, Option<i32>) {
        let first = self.years.get(&from);
        let last = self.years.get(&to);
        let label = match (first, last) {
            (Some(a), Some(b)) if from != to => Some(format!("{} - {}", a.0, b.0)),
            (Some(a), _) => Some(a.0.clone()),
            (None, Some(b)) => Some(b.0.clone()),
            (None, None) => None,
        };
        let am = first.and_then(|y| y.1).or_else(|| last.and_then(|y| y.1));
        (label, am.map(hebrew::am_to_ce))
    }

    /// Word-position ranges that a category (with its sub-categories) covers, per book.
    /// A row with `pageId = 0` (positions 0..0) links the whole book, as do rows without
    /// usable positions.
    pub fn category_ranges(&self, category: i64) -> anyhow::Result<HashMap<u32, Vec<(u32, u32)>>> {
        let mut stmt = self.conn.prepare(&format!(
            "{CATEGORY_TREE} select cb.bookId, cb.pageId, cb.fromPos, cb.toPos from categorybooks cb
             where cb.categoryId in (select id from d)"
        ))?;
        let rows = stmt.query_map([category], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?;
        let mut out: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for row in rows {
            let (book, page, from, to) = row?;
            let range = match (page, from, to) {
                (Some(p), Some(f), Some(t)) if p != 0 && f >= 0 && t >= f => (f as u32, t as u32),
                _ => (0, u32::MAX),
            };
            out.entry(book as u32).or_default().push(range);
        }
        for list in out.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        Ok(out)
    }

    /// Summary of a book by id; `None` when it is not in the catalog.
    pub fn summary_by_id(&self, id: i64) -> anyhow::Result<Option<BookSummary>> {
        let row: Option<(String, String)> = self
            .conn
            .prepare_cached("select name, coalesce(volume, '') from system_books where id = ?1")?
            .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        row.map(|(name, volume)| self.summary(id, name, volume)).transpose()
    }

    fn summary(&self, id: i64, name: String, volume: String) -> anyhow::Result<BookSummary> {
        let mut stmt = self.conn.prepare_cached(
            "select a.id, a.name, coalesce(t.title, '') from system_bookauthors ba
             join system_authors a on a.id = ba.authorId
             left join authortypes t on t.id = ba.authorTypeId
             where ba.bookId = ?1 order by ba.authorTypeId, ba.position",
        )?;
        let authors = stmt
            .query_map([id], |r| {
                Ok(Author { id: r.get(0)?, name: r.get(1)?, role: r.get(2)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let years: Option<(i64, i64)> = self
            .conn
            .prepare_cached("select fromYear, toYear from bookyears where bookId = ?1 limit 1")?
            .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let (year, year_ce) = years.map(|(f, t)| self.year_label(f, t)).unwrap_or((None, None));

        let mut stmt = self.conn.prepare_cached(
            "select p.name from bookplaces bp join places p on p.id = bp.placeId where bp.bookId = ?1",
        )?;
        let places = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let dor: Option<i64> = self
            .conn
            .prepare_cached("select dor from bookdata where bookId = ?1 limit 1")?
            .query_row([id], |r| r.get(0))
            .optional()?;
        let period = dor.and_then(|d| self.periods.get(&d).cloned());

        Ok(BookSummary {
            id,
            name,
            volume,
            authors,
            year,
            year_ce,
            places,
            period,
            access: self.access(id),
        })
    }

    fn filter(&self, q: &BookQuery) -> anyhow::Result<Filter> {
        let mut conds: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();

        if let Some(title) = q.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            let words: Vec<&str> = title.split_whitespace().collect();
            let mut on_name = Vec::new();
            let mut on_alias = Vec::new();
            for word in &words {
                args.push(Value::Text(like(word)));
                let n = args.len();
                on_name.push(format!("b.name like ?{n} escape '\\'"));
                on_alias.push(format!("n.name like ?{n} escape '\\'"));
            }
            conds.push(format!(
                "(({}) or exists (select 1 from bookaddnames n where n.bookId = b.id and {}))",
                on_name.join(" and "),
                on_alias.join(" and ")
            ));
        }

        if let Some(author) = q.author.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            let mut parts = Vec::new();
            for word in author.split_whitespace() {
                args.push(Value::Text(like(word)));
                parts.push(format!("a.name like ?{} escape '\\'", args.len()));
            }
            conds.push(format!(
                "exists (select 1 from system_bookauthors ba join system_authors a on a.id = ba.authorId
                         where ba.bookId = b.id and {})",
                parts.join(" and ")
            ));
        }

        if let Some(author_id) = q.author_id {
            args.push(Value::Integer(author_id));
            conds.push(format!(
                "exists (select 1 from system_bookauthors ba where ba.bookId = b.id and ba.authorId = ?{})",
                args.len()
            ));
        }

        if let Some(place) = q.place.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            args.push(Value::Text(like(place)));
            conds.push(format!(
                "exists (select 1 from bookplaces bp join places p on p.id = bp.placeId
                         where bp.bookId = b.id and p.name like ?{} escape '\\')",
                args.len()
            ));
        }

        if q.year_from.is_some() || q.year_to.is_some() {
            let from = q.year_from.map(hebrew::normalize_year).unwrap_or(i32::MIN);
            let to = q.year_to.map(hebrew::normalize_year).unwrap_or(i32::MAX);
            let ids: Vec<String> = self
                .years
                .iter()
                .filter(|(_, (_, am))| am.is_some_and(|y| y >= from && y <= to))
                .map(|(id, _)| id.to_string())
                .collect();
            if ids.is_empty() {
                return Ok(Filter::Nothing);
            }
            conds.push(format!(
                "exists (select 1 from bookyears y where y.bookId = b.id and y.fromYear in ({}))",
                ids.join(",")
            ));
        }

        if let Some(period) = q.period {
            args.push(Value::Integer(period));
            conds.push(format!(
                "exists (select 1 from bookdata d where d.bookId = b.id and d.dor = ?{})",
                args.len()
            ));
        }

        if let Some(category) = q.category_id {
            args.push(Value::Integer(category));
            let n = args.len();
            // categorybooks has no bookId index: collect the ids once through the categoryId index.
            conds.push(format!(
                "b.id in (with recursive d(id) as (select ?{n} union select c.id from categories c join d on c.parentId = d.id)
                          select cb.bookId from categorybooks cb where cb.categoryId in (select id from d))"
            ));
        }

        if conds.is_empty() {
            return Ok(Filter::All);
        }
        Ok(Filter::Sql(conds.join(" and "), args))
    }

    pub fn find_books(&self, q: &BookQuery) -> anyhow::Result<Page<BookSummary>> {
        let (filter, mut args) = match self.filter(q)? {
            Filter::All => anyhow::bail!(
                "give at least one filter: title, author, author_id, place, year_from/year_to or period"
            ),
            Filter::Nothing => return Ok(Page { total: 0, offset: q.offset, items: vec![] }),
            Filter::Sql(cond, args) => (cond, args),
        };
        let mut order = "b.id".to_string();

        let total: i64 = self.conn.query_row(
            &format!("select count(*) from system_books b where {filter}"),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )?;

        // Ranking parameters come after the filter parameters so the count query above
        // binds exactly the placeholders it uses.
        if let Some(title) = q.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            args.push(Value::Text(title.to_string()));
            let exact = args.len();
            args.push(Value::Text(format!("{}%", title.replace('%', ""))));
            let prefix = args.len();
            order = format!(
                "case when b.name = ?{exact} then 0 when b.name like ?{prefix} then 1 else 2 end, length(b.name), b.id"
            );
        }

        args.push(Value::Integer(q.limit as i64));
        let limit = args.len();
        args.push(Value::Integer(q.offset as i64));
        let offset = args.len();
        let sql = format!(
            "select b.id, b.name, coalesce(b.volume, '') from system_books b where {filter}
             order by {order} limit ?{limit} offset ?{offset}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let items = rows
            .into_iter()
            .map(|(id, name, volume)| self.summary(id, name, volume))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Page { total, offset: q.offset, items })
    }

    pub fn get_book(&self, id: i64) -> anyhow::Result<Option<BookDetails>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "select name, coalesce(volume, '') from system_books where id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((name, volume)) = row else {
            return Ok(None);
        };
        let summary = self.summary(id, name, volume)?;

        let mut stmt = self.conn.prepare("select name from bookaddnames where bookId = ?1")?;
        let additional_names = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            "select name, coalesce(author, ''), coalesce(pageStart, 0) from bookssubbooks
             where parentBookId = ?1 order by pageStart",
        )?;
        let sub_books = stmt
            .query_map([id], |r| {
                let author: String = r.get(1)?;
                Ok(SubBook {
                    name: r.get(0)?,
                    // The source data closes author names with a stray ')'.
                    author: author.trim_end_matches(')').to_string(),
                    start_page: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            "select v2.bookId, b.name, coalesce(b.volume, '') from volumes v1
             join volumes v2 on v2.volumeId = v1.volumeId
             join system_books b on b.id = v2.bookId
             where v1.bookId = ?1 and v2.bookId != ?1 order by v2.position",
        )?;
        let volume_set = stmt
            .query_map([id], |r| Ok(VolumeRef { id: r.get(0)?, name: r.get(1)?, volume: r.get(2)? }))?
            .collect::<Result<Vec<_>, _>>()?;

        let data: Option<(Option<f64>, Option<i64>)> = self
            .conn
            .query_row(
                "select version, newBookId from bookdata where bookId = ?1 limit 1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (added_in_version, replaced_by) = match data {
            Some((version, new_id)) => (
                version.filter(|v| *v > 0.0),
                new_id.filter(|n| *n != 0 && *n != id),
            ),
            None => (None, None),
        };

        Ok(Some(BookDetails {
            summary,
            additional_names,
            sub_books,
            volume_set,
            added_in_version,
            replaced_by,
            groups: self.groups.get(&id).cloned().unwrap_or_default(),
        }))
    }

    pub fn find_authors(&self, name: &str, limit: usize) -> anyhow::Result<Vec<AuthorHit>> {
        let mut conds = Vec::new();
        let mut args: Vec<Value> = Vec::new();
        for word in name.split_whitespace() {
            args.push(Value::Text(like(word)));
            conds.push(format!("a.name like ?{} escape '\\'", args.len()));
        }
        if conds.is_empty() {
            anyhow::bail!("name is empty");
        }
        args.push(Value::Integer(limit as i64));
        let sql = format!(
            "select a.id, a.name, a.period,
                    (select count(*) from system_bookauthors ba where ba.authorId = a.id) as books
             from system_authors a where {} order by books desc, a.name limit ?{}",
            conds.join(" and "),
            args.len()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, name, period, books)| AuthorHit {
                id,
                name,
                period: period.and_then(|p| self.periods.get(&p).cloned()),
                books,
            })
            .collect())
    }

    fn category_node(&self, id: i64) -> anyhow::Result<Option<CategoryNode>> {
        Ok(self
            .conn
            .query_row(
                "select c.id, c.title, coalesce(t.title, ''),
                        (select count(*) from categories k where k.parentId = c.id)
                 from categories c left join categorytypes t on t.id = c.categoryTypeId where c.id = ?1",
                [id],
                |r| Ok(CategoryNode { id: r.get(0)?, title: r.get(1)?, kind: r.get(2)?, children: r.get(3)? }),
            )
            .optional()?)
    }

    pub fn browse_categories(&self, parent: Option<i64>, limit: usize) -> anyhow::Result<CategoryListing> {
        let parent_id = parent.unwrap_or(0);
        let category = match parent {
            Some(id) => Some(self.category_node(id)?.with_context(|| format!("no category {id}"))?),
            None => None,
        };
        let mut stmt = self.conn.prepare(
            "select c.id, c.title, coalesce(t.title, ''),
                    (select count(*) from categories k where k.parentId = c.id)
             from categories c left join categorytypes t on t.id = c.categoryTypeId
             where c.parentId = ?1 order by c.position, c.id limit ?2",
        )?;
        let children = stmt
            .query_map(params![parent_id, limit as i64], |r| {
                Ok(CategoryNode { id: r.get(0)?, title: r.get(1)?, kind: r.get(2)?, children: r.get(3)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let books = if parent.is_some() {
            let mut stmt = self.conn.prepare(
                "select cb.bookId, b.name, coalesce(b.volume, ''), min(cb.pageId)
                 from categorybooks cb join system_books b on b.id = cb.bookId
                 where cb.categoryId = ?1 group by cb.bookId order by b.name limit ?2",
            )?;
            stmt.query_map(params![parent_id, limit as i64], |r| {
                Ok(CategoryBook { book_id: r.get(0)?, name: r.get(1)?, volume: r.get(2)?, first_page_id: r.get(3)? })
            })?
            .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };
        Ok(CategoryListing { category, children, books })
    }

    /// Words the official search links to `word` (`fsalternativewords`), as (word, kind).
    ///
    /// Only direct links are returned. Siblings are not: `שבת` is one expansion of `שב"ק`,
    /// and so is `שבבא קמא`, but those two are unrelated to each other.
    pub fn alternatives(&self, word: &str) -> anyhow::Result<Vec<(String, &'static str)>> {
        let mut stmt = self.conn.prepare(
            "with me(id) as (select id from fsalternativewords where replace(word, char(65279), '') = ?1)
             select distinct replace(w.word, char(65279), ''), w.type from fsalternativewords w
             where (w.id in (select refId from fsalternativewordsrefs where wordId in (select id from me))
                 or w.id in (select wordId from fsalternativewordsrefs where refId in (select id from me)))
               and replace(w.word, char(65279), '') != ?1",
        )?;
        let rows = stmt.query_map([word], |r| {
            let kind = match r.get::<_, i64>(1)? {
                1 => "variant",
                2 => "abbreviation",
                _ => "other",
            };
            Ok((r.get::<_, String>(0)?, kind))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
