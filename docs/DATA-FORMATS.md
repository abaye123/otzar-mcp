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
| `data\books_data.bin` | 10 GB | starts with gzip magic `1F 8B` | unknown | todo |
| `data\books_data_info.bin` | 12 MB | binary | assumed index into books_data.bin | todo |
| `data\books.cords` | 114 GB | binary, u16 quadruples? | assumed OCR word coordinates | todo |
| `data\books_cords\*.cords`, `cords_data\*.cords` | | binary | per-version / per-book coordinates | todo |
| `data\allTsiyuns.bin` | 540 MB | binary | assumed references (ציונים) | todo |
| `data\mefData\<id>` | | | indexes (מפתחות) per book | todo |
| `data\ek.enc` | 266 KB | encrypted | assumed encryption keys | not touched |
| `books\<id>.book` | 161,119 files | header `BKF` + encrypted body | page images, likely DjVu (`splitDjvuFrom`, `djvuHandlersCount` in config) | encrypted |
| `searcher\fs.db` | 9.3 GB | SQLite | full dictionary, see below | used |
| `searcher\tiny.db` | 170 MB | SQLite + FTS5 trigram | frequent-words dictionary | used |
| `searcher\words.bin` | 100 GB | binary | posting lists | partly understood |
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
| `categorybooks` | 9,242,322 | `bookId`, `categoryId`, `pageId` (global page id), `fromPos`, `toPos` (word positions). Indexed on categoryId and pageId, **not** bookId |
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
number. The mapping page id -> (book, page number) is not in `data.db`; candidates are
`data\books_data.bin` and the `.cords` files. Needed for phase 2/3.

## Full-text dictionary (`searcher\fs.db`, `tiny.db`)

`words(w TEXT, b INT, l INT, p BIGINT)`, index `idx_w` on `w`.

- `w`: token as printed, with prefixes (`והשבת`), including OCR noise and joined words
  (`במוצאישבתות`). 145,649,307 rows in fs.db.
- `b`: number of books containing the token. `l`: total occurrences. Verified: `שבת` b=133,500, l=12,666,843.
- `p`: byte offset of the token's posting list in `words.bin`. Same values in fs.db and tiny.db.
- `tiny.db` holds the 2.2 M tokens with b >= 51 and an FTS5 trigram table `words_fts` for
  substring lookup (`match '"שבתות"'`).
- Lookup by `w` is under 1 ms.

## Posting lists (`searcher\words.bin`) - partly understood

File starts `D6 E0 07 00 ...`. At offset `p` for a rare token (b=1, l=1, p=23107559):

```
01 00 00 00   u32 books (=1)
01 00 00 00   u32 occurrences (=1)
51 C3 00 00   u32 book id (=50001)
01 00 00 00   u32 hits in this book
00 00 00 00   ?
A1 F1 02 00   u32 ? (192929, maybe word position)
0A 00 00 00   u32 ? (10)
20 28 D7 95 ...   bytes that look like UTF-8 Hebrew (context or next record?)
```

For a frequent token (`''`, b=7378) the record continues as triplets of u32 that look like
`(bookId?, ?, count?)`. To be decoded in phase 0 by comparing with `.rst` output.

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

## Official app

- Electron; code is V8 bytecode (`index_c.jsc`, bytenode) inside `resources\app.asar`: not readable as JS.
- `OtzarHahochma.bat`: `start /D %OTZARAPPCD% %OTZARAPPCD%\otzar.exe BOOK=%1`. A page argument is unknown.
- Local ports configured: app 3000, mef 3030, search server 6060 (not listening when checked).
