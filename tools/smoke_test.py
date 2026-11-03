"""End-to-end smoke test: talks MCP over stdio to the built server, like Claude does.

Usage (from the repo root):
    cargo build --release
    python -I tools/smoke_test.py [path-to-exe]

Read-only: it never calls open_in_otzar (that would open a window).
"""

import json
import subprocess
import sys
import time

EXE = sys.argv[1] if len(sys.argv) > 1 else r"target\release\otzar-mcp.exe"

CALLS = [
    ("server_status", {}),
    ("find_books", {"title": "משנה ברורה", "limit": 3}),
    ("find_books", {"author": "קוק", "year_from": 1920, "year_to": 1935, "limit": 3}),
    ("find_books", {"place": "וילנא", "period": 4, "limit": 2}),
    ("get_book", {"book_id": 10758}),
    # 7395 belongs to the KOOK package: expect access.status "unknown" without a subscription config.
    ("get_book", {"book_id": 7395}),
    ("find_authors", {"name": "חפץ חיים", "limit": 3}),
    ("browse_categories", {}),
    ("browse_categories", {"category_id": 1, "limit": 5}),
    ("word_stats", {"word": "שבת"}),
    ("word_stats", {"word": "ארץ", "include_containing": True, "limit": 5}),
    # Reference numbers from the official engine (docs/DATA-FORMATS.md):
    # והאידנא 22,028 books / 53,185 hits; שבת קודש phrase 45,802 / 252,478; near 65,835 / 423,172.
    ("search_text", {"query": "והאידנא", "limit": 2, "pages_per_book": 3}),
    ("search_text", {"query": "שבת קודש", "mode": "phrase", "limit": 2, "pages_per_book": 3}),
    ("search_text", {"query": "שבת קודש", "limit": 1, "pages_per_book": 2}),
    ("search_text", {"query": "והאידנא", "author": "קוק", "limit": 3, "pages_per_book": 2}),
    ("search_text", {"query": "האידנא", "prefixes": True, "limit": 1, "pages_per_book": 1}),
    # Regression: books with a NULL toYear in bookyears used to fail the whole search.
    ("search_text", {"query": "מאכלי חלב בשבועות", "mode": "phrase", "prefixes": True, "limit": 25, "pages_per_book": 1}),
    # 79 = ספרי יסוד / רמב"ם
    ("search_text", {"query": "שבת", "category_id": 79, "limit": 2, "pages_per_book": 2}),
    ("find_books", {"category_id": 79, "limit": 2}),
]


def main() -> int:
    proc = subprocess.Popen(
        [EXE], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, encoding="utf-8",
    )
    next_id = 0

    def send(method, params=None, notify=False):
        nonlocal next_id
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        if not notify:
            next_id += 1
            msg["id"] = next_id
        proc.stdin.write(json.dumps(msg, ensure_ascii=False) + "\n")
        proc.stdin.flush()
        if notify:
            return None
        while True:
            line = proc.stdout.readline()
            if not line:
                raise RuntimeError("server closed stdout: " + proc.stderr.read())
            reply = json.loads(line)
            if reply.get("id") == next_id:
                return reply

    init = send("initialize", {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "smoke-test", "version": "0"},
    })
    print("server:", init["result"]["serverInfo"])
    send("notifications/initialized", notify=True)
    tools = send("tools/list")["result"]["tools"]
    print("tools:", ", ".join(t["name"] for t in tools))

    failures = 0
    for name, args in CALLS:
        started = time.perf_counter()
        reply = send("tools/call", {"name": name, "arguments": args})
        elapsed = (time.perf_counter() - started) * 1000
        result = reply.get("result", {})
        text = "".join(c.get("text", "") for c in result.get("content", []))
        failed = "error" in reply or result.get("isError")
        failures += bool(failed)
        print(f"\n=== {name} {json.dumps(args, ensure_ascii=False)}  [{elapsed:.0f} ms]{'  FAILED' if failed else ''}")
        print(text[:1200] if text else json.dumps(reply, ensure_ascii=False)[:1200])

    prompts = send("prompts/list")
    names = [p["name"] for p in prompts.get("result", {}).get("prompts", [])]
    print("\nprompts:", names)
    got = send("prompts/get", {"name": "research", "arguments": {"question": "מנהג נר שבת"}})
    prompt_ok = "result" in got and got["result"]["messages"]
    print("prompts/get research:", "ok" if prompt_ok else json.dumps(got, ensure_ascii=False)[:400])
    failures += not prompt_ok

    proc.stdin.close()
    proc.wait(timeout=10)
    print(f"\n{len(CALLS) - failures}/{len(CALLS)} calls succeeded")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main())
