"""Phase 0: show the official engine's result files side by side.

    python -I tools/research/read_rst.py [disk=D:\\]

For every mn_*.rst (JSON summary) prints the summary and, for each book, the slice of the
referenced binary .rst as u32 values, to decode the location records. Read-only.
"""

import json
import struct
import sys
from pathlib import Path


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    disk = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("D:\\")
    results = disk / "searcher" / "results"
    for summary_path in sorted(results.glob("mn_*.rst"), key=lambda p: p.stat().st_mtime):
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        print(f"\n=== {summary_path.name}: {summary.get('numBooks')} books, {summary.get('numLocations')} locations")
        blobs = {}
        for name in summary.get("files", []):
            path = results / f"{name}.rst"
            blobs[name] = path.read_bytes() if path.exists() else b""
            data = blobs[name]
            print(f"  {name}.rst: {len(data)} bytes, u32 head {list(struct.unpack_from(f'<{min(len(data) // 4, 16)}I', data))}")
        for book, info in summary.get("books", {}).items():
            print(f"  book {book}: results={info.get('results')} rate={info.get('rate')}")
            for f in info.get("files", []):
                print(f"    {f['filename']} [{f['start']}:{f['end']}]")


if __name__ == "__main__":
    main()
