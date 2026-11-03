"""Compare our reconstruction of a search with the official engine's result file.

Search in the app first (the engine writes D:\\searcher\\results\\mn_<uuid>.rst), then:

    python -I tools/research/compare_engine.py "שבת קודש" [mn_file] [distance=30]

One word: merged postings (main index + sub-indexes, later index replaces earlier per book,
books not in the catalog dropped). Several words: one-to-one pairs within `distance`
positions, the engine's default when "הצמדת מילים" is off; distance 0 = adjacent phrase
("הצמדת מילים" on). Prints totals and how many books
have the same per-book count as the engine. Read-only. See docs/DATA-FORMATS.md.
"""

import glob
import json
import os
import sqlite3
import sys
from array import array

DISK = "D:/"
CATALOG = "C:/OtzarApp/OtzarLocal/data.db"


def ro(path):
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def index_roots():
    sub = os.path.join(DISK, "searcher", "subindexs")
    return [os.path.join(DISK, "searcher")] + [os.path.join(sub, d) for d in sorted(os.listdir(sub))]


def positions(word, books):
    out = {}
    for root in index_roots():
        row = ro(os.path.join(root, "fs.db")).execute(
            "select b, l, p from words where w = ? order by b desc limit 1", (word,)
        ).fetchone()
        if not row:
            continue
        b, l, p = row
        with open(os.path.join(root, "words.bin"), "rb") as f:
            f.seek(p + 8)
            directory = array("I")
            directory.frombytes(f.read(12 * b))
            occ = array("I")
            occ.frombytes(f.read(4 * l))
        for i in range(b):
            out[directory[3 * i]] = occ[directory[3 * i + 2] : directory[3 * i + 1]]
    return {k: v for k, v in out.items() if k in books}


def pairs(a, b, distance):
    if distance == 0:
        # "הצמדת מילים": the second word right after the first.
        following = set(b)
        return sum(1 for x in a if x + 1 in following)
    i = j = count = 0
    while i < len(a) and j < len(b):
        if abs(a[i] - b[j]) <= distance:
            count += 1
            i += 1
            j += 1
        elif a[i] < b[j]:
            i += 1
        else:
            j += 1
    return count


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    words = sys.argv[1].split()
    mn = sys.argv[2] if len(sys.argv) > 2 else max(glob.glob(os.path.join(DISK, "searcher", "results", "mn_*.rst")), key=os.path.getmtime)
    distance = int(sys.argv[3]) if len(sys.argv) > 3 else 30
    engine = {int(k): v["results"] for k, v in json.load(open(mn, encoding="utf-8"))["books"].items()}
    books = {r[0] for r in ro(CATALOG).execute("select id from system_books")}

    lists = [positions(w, books) for w in words]
    if len(lists) == 1:
        ours = {k: len(v) for k, v in lists[0].items()}
    elif len(lists) == 2:
        ours = {}
        for k in lists[0].keys() & lists[1].keys():
            c = pairs(lists[0][k], lists[1][k], distance)
            if c:
                ours[k] = c
    else:
        sys.exit("only one or two words are supported so far")

    same = sum(1 for k in engine if ours.get(k) == engine[k])
    print(f"result file: {os.path.basename(mn)}")
    print(f"engine: {len(engine)} books, {sum(engine.values())} hits")
    print(f"ours:   {len(ours)} books, {sum(ours.values())} hits")
    print(f"engine-only books {len(set(engine) - set(ours))}, ours-only {len(set(ours) - set(engine))}, same count {same}")


if __name__ == "__main__":
    main()
