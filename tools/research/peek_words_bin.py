"""Phase 0: dump the posting list of a word from words.bin to decode its layout.

    python -I tools/research/peek_words_bin.py <word> [bytes=256] [disk=D:\\]

Prints the dictionary row (books, occurrences, offset), then the bytes at the offset as hex,
as little-endian u32 values, and any UTF-8 runs. Read-only.
"""

import sqlite3
import struct
import sys
from pathlib import Path


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    word = sys.argv[1]
    size = int(sys.argv[2]) if len(sys.argv) > 2 else 256
    disk = Path(sys.argv[3]) if len(sys.argv) > 3 else Path("D:\\")
    db = sqlite3.connect(f"file:{(disk / 'searcher' / 'fs.db').as_posix()}?mode=ro", uri=True)
    row = db.execute("select b, l, p from words where w = ? order by b desc limit 1", (word,)).fetchone()
    if not row:
        print("not in the dictionary")
        return
    books, occurrences, offset = row
    print(f"word={word} books={books} occurrences={occurrences} offset={offset}")
    with open(disk / "searcher" / "words.bin", "rb") as f:
        f.seek(offset)
        data = f.read(size)
    for i in range(0, len(data), 16):
        chunk = data[i : i + 16]
        print(f"{offset + i:>14}  {chunk.hex(' '):<48}  {chunk.decode('latin-1').translate({c: '.' for c in range(32)})!s}")
    u32 = struct.unpack_from(f"<{len(data) // 4}I", data)
    print("\nu32:", list(u32[:64]))
    print("\nutf-8 runs:", [s for s in data.decode("utf-8", "replace").split("\ufffd") if len(s.strip()) > 2][:20])


if __name__ == "__main__":
    main()
