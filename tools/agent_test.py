"""End-to-end agent test: a fresh headless Claude Code session answers a research question
with the installed `otzar` MCP server, and this script summarizes what it did.

    python -I tools/agent_test.py "מה המקורות למנהג לאכול מאכלי חלב בשבועות? עם ספר ועמוד."

Prints every tool call with its arguments, result size and errors, then the final answer,
turns, duration and cost. Needs `claude` on PATH and the server registered as `otzar`
(scripts/install.ps1). Costs tokens: run it after changes to tools or instructions.

What to look for: failed tool calls, needless retries, quotations not labeled as unverified,
missing access marking, and whether open_in_otzar is offered.
"""

import json
import subprocess
import sys


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    question = sys.argv[1] if len(sys.argv) > 1 else "מה המקורות למנהג לאכול מאכלי חלב בחג השבועות? אני רוצה מקורות מהראשונים ומהאחרונים, עם ספר ועמוד. חפש באוצר החכמה שלי."
    proc = subprocess.run(
        ["claude", "-p", question, "--allowedTools", "mcp__otzar__*", "--max-turns", "40",
         "--output-format", "stream-json", "--verbose"],
        capture_output=True, text=True, encoding="utf-8",
    )
    calls, n, final, errors = {}, 0, None, 0
    for line in proc.stdout.splitlines():
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        if ev.get("type") == "assistant":
            for b in ev["message"].get("content", []):
                if b.get("type") == "tool_use":
                    n += 1
                    calls[b["id"]] = n
                    print(f"#{n} {b['name'].replace('mcp__otzar__', '')} {json.dumps(b['input'], ensure_ascii=False)}")
        elif ev.get("type") == "user":
            for b in ev["message"].get("content", []):
                if isinstance(b, dict) and b.get("type") == "tool_result":
                    c = b.get("content")
                    text = c if isinstance(c, str) else "".join(x.get("text", "") for x in (c or []))
                    if b.get("is_error"):
                        errors += 1
                    flag = " ERROR " + text[:200] if b.get("is_error") else ""
                    print(f"   -> #{calls.get(b['tool_use_id'])} {len(text)} chars{flag}")
        elif ev.get("type") == "result":
            final = ev
    if not final:
        print("no result event; stderr:", proc.stderr[-500:])
        return 1
    print(f"\n=== {n} tool calls, {errors} errors, {final.get('num_turns')} turns, "
          f"{final.get('duration_ms', 0) / 1000:.0f} s, ${final.get('total_cost_usd', 0):.2f}\n")
    print(final.get("result", ""))
    return 1 if errors or final.get("is_error") else 0


if __name__ == "__main__":
    sys.exit(main())
