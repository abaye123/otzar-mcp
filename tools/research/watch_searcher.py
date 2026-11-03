"""Phase 0: record how the official app drives its search engine.

Run this, then search for something in the Otzar HaHochma app. It prints every new
searcher/indexer process (command line, working dir, parent) and every new file in
<disk>\\searcher\\results. Read-only. Stop with Ctrl+C.

    python -I tools/research/watch_searcher.py [D:\\]
"""

import json
import subprocess
import sys
import time
from pathlib import Path

DISK = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("D:\\")
RESULTS = DISK / "searcher" / "results"

PS = (
    "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'searcher|indexer' -and $_.Name -ne 'SearchIndexer.exe' } "
    "| Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine | ConvertTo-Json -Compress"
)


def processes():
    out = subprocess.run(["powershell", "-NoProfile", "-Command", PS], capture_output=True, text=True).stdout.strip()
    if not out:
        return {}
    data = json.loads(out)
    if isinstance(data, dict):
        data = [data]
    return {p["ProcessId"]: p for p in data}


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    seen_procs = set(processes())
    seen_files = {p.name for p in RESULTS.glob("*")} if RESULTS.exists() else set()
    print(f"watching {RESULTS} and searcher processes; search in the app now")
    while True:
        for pid, proc in processes().items():
            if pid not in seen_procs:
                seen_procs.add(pid)
                print(f"\n[{time.strftime('%H:%M:%S')}] process {json.dumps(proc, ensure_ascii=False, indent=1)}")
        if RESULTS.exists():
            for path in sorted(RESULTS.glob("*")):
                if path.name not in seen_files:
                    seen_files.add(path.name)
                    data = path.read_bytes()
                    print(f"\n[{time.strftime('%H:%M:%S')}] new {path.name} ({len(data)} bytes)")
                    if data[:1] == b"{":
                        print(data.decode("utf-8", "replace")[:2000])
                    else:
                        print(data[:128].hex(" "))
        time.sleep(0.3)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
