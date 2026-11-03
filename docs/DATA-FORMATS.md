# Otzar HaHochma data formats

Everything here was observed read-only on 2026-11-03 (disk version 24, db version 7.10000).
Mark new findings with how they were verified. "Assumed" means not yet verified.

## Disk layout (`<disk>` = `D:\` at the time)

| Path | Size | Format | Content | State |
| --- | --- | --- | --- | --- |
| `config.json` | 1 KB | JSON | app config: `booksDir`, `dataDir`, `searchServerAppDir`, `SearchServerPort` 6060, `useSearchServer` false, `searcherReleaseWhenNotInUse` 1, `db` = `%CURRENT_APPDATA%data.db` | read |
| `info\key.ol`, `key.ol.ltc` | 886 B, 50 B | assumed encrypted | license: probably the subscribed packages | not touched |
| `data\data.db` | 714 MB | SQLite | catalog (older copy; the app uses `C:\OtzarApp\OtzarLocal\data.db`, 721 MB) | used |
| `data\disk-ver.txt`, `db_ver.txt` | | text | `24`, `7.10000` | used |
| `data\books_data.bin` | 10 GB | gzip members | per-book JSON: pages, word ranges, TOC | decoded, see below |
| `data\books_data_info.bin` | 12 MB | `{u64,u32}` per book id | index into books_data.bin | decoded |
| `data\books_data\*.bin` | | bundles / single gzip | newer books | decoded |
| `data\books.cords` | 114 GB | u16 × 4 per word: x, y, w, h | word boxes on the scans, **no text** | checked |
| `data\books_lst.cords` | 12 MB | 16 bytes per book id | index into books.cords | checked |
| `data\books_cords\*.cords`, `cords_data\*.cords` | | same kind | word boxes for newer books | checked |
| `data\allTsiyuns.bin`, `allTsiyuns.words.bin` | 540 MB, 4 MB | binary + fixed-width UTF-8 strings | references (ציונים) | not used |
| `data\mefData\<id>` | | zlib whose bytes were stored as UTF-8 text (decode UTF-8 -> latin-1 bytes -> zlib) | JSON page sections with rectangles, **no text** | checked |
| `data\ek.enc` | 266 KB | encrypted | assumed encryption keys | not touched |
| `books\<id>.book` | 161,119 files | header `BKF` + encrypted body | page images, likely DjVu (`splitDjvuFrom`, `djvuHandlersCount` in config) | encrypted |
| `searcher\fs.db` | 9.3 GB | SQLite | full dictionary, see below | used |
| `searcher\tiny.db` | 170 MB | SQLite + FTS5 trigram | frequent-words dictionary | used |
| `searcher\words.bin` | 100 GB | binary | posting lists | decoded, see below |
| `searcher\searcherOtzar.exe` | 175 KB | native exe (WS2_32, "searcher for Otzar HaHochma app", writes `./results/`) | official search engine | todo (phase 0) |
| `searcher\results\*.rst` | KB | JSON (`mn_*.rst`) + binary | official search results | partly understood |
| `searcher\app\x64\addBooks.exe` | | .NET (PdfPig) | adds user PDFs to the index | not needed |

## Catalog (`data.db`)

| Table | Rows | Columns and meaning |
| --- | --- | --- |
| `system_books` | 163,415 | `id`, `name`, `volume` (may hold `<series> - ג`) |
| `bookaddnames` | 19,029 | `bookId`, `name`: alternative titles. Index on bookId |
| `system_authors` | 44,156 | `id`, `name` ("Surname, Given ben Father"), `period` (0-7, mostly matches `sederdorots` ids) |
| `system_bookauthors` | 190,971 | `authorId`, `bookId`, `authorTypeId` (1 ראשי, 2 משני via `authortypes`), `position` |
| `bookyears` | 178,136 | `bookId`, `fromYear`, `toYear`: ids into `years` |
| `years` | 656 | `id`, `name` Hebrew label (`תש"ל`, `תר"צ בערך`, `אחר תש"ז`, `ר"מ לפני`), `value` (0; -1 once). Parse with `hebrew::parse_year` |
| `bookplaces` / `places` | 196,204 / 1,639 | print places |
| `bookdata` | 163,415 | `bookId`, `isText`, `isSidra`, `status`, `version` (catalog version added), `dor` (= `sederdorots.id`, 0 unknown), `newBookId` (replacement, 1,665 set) |
| `sederdorots` | 6 | 1 מקרא וחז"ל, 2 ראשונים וקדמונים, 3 אחרונים ש'-תר, 4 אחרונים תר-תש, 5 אחרוני זמנינו, 6 אחרים |
| `volumes` | 130,324 | `volumeId` groups volumes of one set, `position` order, unique on bookId |
| `bookssubbooks` | 1,865 | works printed inside a book: `name`, `author` (trailing `)` in the data), `parentBookId`, `pageStart` |
| `categories` | 170,462 | tree: `id`, `title`, `level`, `position`, `parentId` (0 = root), `categoryTypeId` |
| `categorytypes` | 5 | 1 ספרי יסוד, 2 מפרשים, 3 נושאים, 4 מאגרים, 5 ספרי טקסט |
| `categorybooks` | 9,242,322 | `bookId`, `categoryId`, `pageId` (global page id), `fromPos`, `toPos` (word positions, same space as postings; `fromPos` = the page's `firstWord`). A row with `pageId = 0` and `0, 0` links the **whole book** (typical for upper categories). Indexed on categoryId and pageId, **not** bookId |
| `sifreiyesod` | 69,254 | links base texts to categories and pages: `categoryId`, `bookId`, `pageId`, `pageName` (e.g. `P0140`) |
| `systemmafs` | 2,648 | index entries: `bookId`, `pageId` (e.g. `S0066`), `cords` (`x,y,w,h`), `mafkey`, `mafvalue` |
| `packages` | 25 | `id`, **`title` = code** (`KOOK`), **`name` = Hebrew**, `active` |
| `permissions` | 21 | same layout as packages |
| `bookspackages` | 46,181 | `bookId`, `permissionId` -> `packages.id`. No bookId index (loaded into memory) |
| `bookspermissions` | 25,760 | `bookId`, `permissionId` -> `permissions.id` |
| `fsalternativewords` | 85,517 | `id`, `word` (may start with U+FEFF), `type` 1 = variant/synonym, 2 = abbreviation/expansion |
| `fsalternativewordsrefs` | 162,287 | `wordId`, `refId`: undirected link between two words. Use direct links only |
| `alternativewords(+refs)` | 13,858 | title-search variants |
| `users`, `user*`, `history` | | the local user's data; ignore |
| `pages`, `pagecoords`, `pagedata`, `tsiyuns`, `mm*` | 0 | empty in this install: page data lives on the disk |

Page ids: `categorybooks.pageId` is a global page id (e.g. 1475667), not a printed page
number. The mapping page id -> (book, page) is in `data\books_data.bin` (section below).
`categorybooks.fromPos`/`toPos` are word positions in the same space as the posting lists.

## Full-text dictionary (`searcher\fs.db`, `tiny.db`)

`words(w TEXT, b INT, l INT, p BIGINT)`, index `idx_w` on `w`.

- `w`: token as printed, with prefixes (`והשבת`), including OCR noise and joined words
  (`במוצאישבתות`). 145,649,307 rows in fs.db.
- `b`: number of books containing the token. `l`: total occurrences. Verified: `שבת` b=133,500, l=12,666,843.
- `p`: byte offset of the token's posting list in `words.bin`. Same values in fs.db and tiny.db.
- `tiny.db` holds the 2.2 M tokens with b >= 51 and an FTS5 trigram table `words_fts` for
  substring lookup (`match '"שבתות"'`).
- Lookup by `w` is under 1 ms.

## Posting lists (`searcher\words.bin`) - decoded

Verified 2026-11-03 on `והאידנא`, `שבתות` and `''`: for each, the computed end of the record is
exactly the next record's start, and the next record's word has a dictionary row whose `p`
equals that computed offset. All integers little-endian.

```
at offset p (from fs.db):
  u32 books            == fs.db b
  u32 occurrences      == fs.db l
  books × { u32 book_id, u32 end, u32 start }
        book ids ascending; [start, end) indexes the occurrence array;
        start of entry i == end of entry i-1, last end == occurrences
  occurrences × u32 word_position
        position of the word inside the book (0-based word index), ascending within a book
then the next record's key:
  u32 byte_length, UTF-8 word   (p of that word points right after it)
```

Size of a record = 8 + 12 × books + 4 × occurrences. Example: `שבת` is about 1.6 MB
(133,500 books, 12.7 M occurrences): read only the directory first, then the slices you need.

Book ids in postings can be deleted books that are no longer in the catalog (e.g. 538, 550,
639). Filter hits against `system_books`. (`data\delBooks.bin` is an opaque hex string, not an id list.)

### Sub-indexes (`searcher\subindexs\<name>\`)

The main index stops at the books of an older version (max book id 679,646 for `והאידנא`).
Newer and re-scanned books are in sub-indexes, each a full set `fs.db` + `tiny.db` + `words.bin`
in the same formats:

| Folder | fs.db | words.bin |
| --- | --- | --- |
| `22_133517728054915332` | 24 MB | 63 MB |
| `22_1726642359`, `22_1726644239` | 2 MB | 4.5 MB |
| `23_00001` | 847 MB | 6.5 GB |
| `24` | 1.2 GB | 9.6 GB |

Merge rule, verified against the official engine: process the main index, then the
sub-indexes in name order; for a book that appears in several, **the later index replaces the
earlier one** (do not add counts); finally drop books that are not in the catalog. For
`והאידנא` this gives 22,028 books and 53,185 occurrences, identical to the engine's `.rst`
output in every one of the 22,028 per-book counts. Summing instead of replacing gives 53,896 (wrong).

### Official engine vs. what the app displays

The engine returns all matching books. The app then hides some: for `והאידנא` it shows
42,692 hits in 18,535 books out of the engine's 53,185 in 22,028. The app's hiding rule is not
fully known. The closest tested rule (hide books that are replaced via `newBookId`, in `NO_BT`,
or in a restricted package/permission) leaves 18,654 books. This project does not need the
rule: it searches everything and marks access instead of hiding.

The app's per-book number in brackets (e.g. `[82]`) is the occurrence count in that book, and
"relevance" order is by that count.

### Multi-word queries (verified on `שבת קודש`, 2026-11-03)

With the app's "הצמדת מילים" (adjacent words) option **off**, the engine matches the words
**within 30 word positions of each other, in either order**, and counts **one-to-one pairs**:
walk both sorted position lists, pair two occurrences when `|a - b| <= 30` and advance both,
otherwise advance the smaller. Result vs. the engine's `.rst`: 65,823 vs 65,835 books (0 extra
books on our side), 424,089 vs 423,172 hits, identical per-book count in 65,056 books (98.8%).
The remaining difference is some detail of the engine's pairing; not resolved.

With "הצמדת מילים" **on**, the engine matches the adjacent phrase (word 2 at position + 1).
Verified: engine 45,802 books / 252,478 hits; ours 45,794 / 252,469, identical per-book count
in all 45,794 of our books, nothing extra on our side. The engine finds 8 more books (9 hits),
probably the same small unexplained gap as in the proximity mode.

Linked variants (`fsalternativewords`, e.g. `שב"ק`, `שבת קדש`) are **not** added by default:
adding them makes the match worse.

The app displayed 354,158 hits in 53,825 books for this query, again after its display filter.

## Pages and word positions (`data\books_data_info.bin`, `data\books_data.bin`) - decoded

`books_data_info.bin`: fixed array indexed by book id, 12 bytes per entry:
`{ u64 offset, u32 size }` into `books_data.bin` (1,000,000 entries; size 0 = no record there).

`books_data.bin`: concatenated gzip members. Each decompresses to a JSON array with 4 parts:

```json
[
  [{"moredata": {"isText": 0}}],
  [ {"id": 1475659, "name": "S0001", "position": 1, "bookId": 10758, "empty": 0,
     "pagedata": [{"id": 1475659, "width": 1816, "height": 2368, "margin": 0,
                   "firstWord": 0, "numWords": 41, "letter": "1", "pageId": 1475659}]}, ...],
  [ {"title": "הלכות ערכין וחרמין ג", "type": 1, "id": 3280771, "position": 18, "pageId": 30, "page": 30}, ...],
  [ [], [{"books": [{"bookId": 21709, "mmId": 2217395, "score": 0, "pageId": 0, "positionPage": 0}, ...]}], ... ]
]
```

- Part 1: pages. `id` = global page id (same space as `categorybooks.pageId`), `name` = scan
  name (`P0001L`, `S0002`, `Y0024`), `position` = page order (1-based),
  `pagedata[0].letter` = printed page label (`קה`, `35 36`), `firstWord`/`numWords` = range of
  word positions on the page.
- Part 2: table of contents (ציונים): `title` and the page `position`.
- Part 3: per page, links to other books (מראי מקומות), not yet used.

Word position -> page: take pages with `numWords > 0`, **sort by `firstWord`** (the list is in
page order, which is not always word order), binary-search the position. Verified on 3,001
occurrences of `ברורה`: 98.6% fall inside a page range; the rest sit in gaps between ranges
(assign them to the preceding page).

Newer books are not in `books_data.bin`. They are in `data\books_data\`:

- `booksdata_<unix time>.bin`: bundle. `u32 count`, then `count × { u32 book_id, u64 offset, u32 size }`,
  then the gzip members (offsets are from the file start; verified contiguous, first offset = 4 + 16 × count).
- `<book_id>.bin`: a single gzip member for one book.

Lookup order: `<book_id>.bin`, then bundles newest first, then `books_data_info.bin`.

## Official search results (`searcher\results\*.rst`)

`mn_<uuid>.rst` is JSON:

```json
{"status":0,"errorCode":0,"filename":"mn_...","numBooks":3,"numLocations":90,
 "books":{"144571":{"files":[{"filename":"3p1rag...","end":4,"start":0}],"results":1,"rate":20,...}},
 "files":["3p1rag..."]}
```

`books` keys are book ids; `files[].filename` names a binary `.rst` holding the locations,
`start`/`end` index into it, `rate` is the ranking score. The binary `.rst` starts with a
u32 count followed by fixed-size records (to be decoded).

## Where is the page text? (checked 2026-11-03)

Not in any readable file. The index stores word positions without the text, `.cords` hold
word boxes, `mefData` holds section rectangles, `books_data` holds page metadata. The OCR text
can only be inside the encrypted `books\*.book` files (likely the DjVu hidden-text layer).
Two ways to get page text, both need the publisher (docs/LEGAL.md, docs/ROADMAP.md phase 3):

1. The publisher provides the method or a library to read `.book` files.
2. Rebuild text from the inverted index: scanning every dictionary entry of every segment
   (about 150 M tokens) and writing (book, position) -> token would recreate the full text of
   the library locally. Technically possible without touching the encryption, but it is a
   full copy of the database, which the project rules forbid without explicit approval.

## Official app

- Electron; code is V8 bytecode (`index_c.jsc`, bytenode) inside `resources\app.asar`: not readable as JS.
- `OtzarHahochma.bat`: `start /D %OTZARAPPCD% %OTZARAPPCD%\otzar.exe BOOK=%1`, where `%1` is an
  `OTZARBOOK:` link (registered URL protocol, `HKCR\OTZARBOOK`). On this machine the protocol's
  command is stored unexpanded (`%OTZARAPPCD%/...`), so Windows cannot launch it; start
  `otzar.exe` directly instead.
- Book link format, as produced by the app's "copy link" (verified 2026-11-03):
  `OtzarBook://book/<book id>/p/<page>/t/<unix ms>/fs/0/start/0/end/0/c`.
  `<page>` is the page order in the book (1-based, the `position` in books_data), not the
  printed label. `otzar.exe "BOOK=<link>"` opens it, also while the app runs; each call opens
  an additional app window. `BOOK=<id>` alone does not work. A book outside the subscription
  opens with "אין לך רישיון לצפיה בספר זה".
- Local ports configured: app 3000, mef 3030, search server 6060 (not listening when checked).
