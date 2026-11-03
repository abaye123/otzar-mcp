"""Call one MCP tool of the built server and print the result. For debugging.

    python -I tools/call.py <tool> '<json arguments>' [path-to-exe]
    python -I tools/call.py search_text '{"query": "שבת", "book_ids": [142009]}'
"""

import json
import subprocess
import sys


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    tool = sys.argv[1]
    args = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    exe = sys.argv[3] if len(sys.argv) > 3 else r"target\release\otzar-mcp.exe"
    messages = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "call", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": tool, "arguments": args}},
    ]
    proc = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            text=True, encoding="utf-8")
    for m in messages:
        proc.stdin.write(json.dumps(m, ensure_ascii=False) + "\n")
    proc.stdin.flush()
    while True:
        line = proc.stdout.readline()
        if not line:
            print("server exited without a reply")
            return 1
        reply = json.loads(line)
        if reply.get("id") == 2:
            break
    proc.stdin.close()
    proc.wait(timeout=10)
    result = reply.get("result", {})
    for block in result.get("content", []):
        print(block.get("text", ""))
    if "error" in reply:
        print(json.dumps(reply["error"], ensure_ascii=False, indent=1))
    return 1 if result.get("isError") or "error" in reply else 0


if __name__ == "__main__":
    sys.exit(main())
