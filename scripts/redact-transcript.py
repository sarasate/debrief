#!/usr/bin/env python3
"""Turn a real Claude Code transcript into a redacted test fixture.

Keeps the shape the parser depends on (line types, uuids, timestamps, cwd,
tool names, file paths, tool_result errors) and blanks everything else:
prompts, assistant text, thinking, commands, file contents and tool output.

    scripts/redact-transcript.py SESSION.jsonl OUT.jsonl [--home /Users/dev] [--repo /path/to/repo]

With --repo, file paths outside that repo (memory files, scratch dirs)
become "/redacted/outside-repo", so they can't leak into a fixture.

Prompts become "[prompt N]", assistant text "[text N]" and the AI title
"Redacted session title", so tests can assert which one the parser picked.
Transcripts are data: nothing in them is ever executed.
"""
import argparse
import json
import os
import re

DROP_TYPES = {"file-history-snapshot", "file-history-delta", "queue-operation", "attachment"}
DROP_KEYS = {"serverClassifierRequest", "serverClassifierContext", "classifierMetaLines", "classifierBoundary", "requestId"}
KEEP_INPUT = {"file_path", "notebook_path"}
# Extra terms that must not survive redaction (employer, product names),
# as a regex in $REDACT_FORBID; checked on every output line.
FORBIDDEN = re.compile(os.environ.get("REDACT_FORBID") or r"(?!)", re.I)
EMAIL = re.compile(r"[\w.+-]+@[\w-]+\.[\w.]+")


OUTSIDE = "/redacted/outside-repo"  # absolute, so it never resolves inside a repo
REPO = None  # set from --repo


def keep_path(v):
    """A kept file path, or OUTSIDE when --repo is set and it isn't under it.
    Relative paths are kept: they resolve against the line's cwd."""
    if REPO is None or not isinstance(v, str) or not os.path.isabs(v):
        return v
    real = os.path.normpath(v)
    return v if real == REPO or real.startswith(REPO + os.sep) else OUTSIDE


def redact_input(inp):
    if not isinstance(inp, dict):
        return "[redacted]"
    return {k: (keep_path(v) if k in KEEP_INPUT else "[redacted]") for k, v in inp.items()}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("out")
    ap.add_argument("--home", default="/Users/dev")
    ap.add_argument("--repo", help="blank file paths outside this repo")
    args = ap.parse_args()
    global REPO
    REPO = os.path.normpath(os.path.abspath(args.repo)) if args.repo else None
    real_home = os.path.expanduser("~")
    counters = {"prompt": 0, "text": 0}
    user = os.path.basename(real_home)

    def scrub(s):
        # Home dir first, then the username where it appears in path slugs.
        s = s.replace(real_home, args.home).replace(f"-Users-{user}-", "-Users-dev-")
        return EMAIL.sub("dev@example.invalid", s)

    def tag(kind):
        counters[kind] += 1
        return f"[{kind} {counters[kind]}]"

    out = []
    for line in open(args.src, encoding="utf-8"):
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        t = o.get("type")
        if t in DROP_TYPES:
            continue
        for k in DROP_KEYS:
            o.pop(k, None)
        if t == "ai-title":
            o["aiTitle"] = "Redacted session title"
        if t == "last-prompt":
            o["lastPrompt"] = "[redacted]"
        msg = o.get("message")
        if isinstance(msg, dict):
            msg.pop("usage", None)
            c = msg.get("content")
            if isinstance(c, str):
                msg["content"] = tag("prompt") if t == "user" else "[redacted]"
            elif isinstance(c, list):
                for b in c:
                    bt = b.get("type")
                    if bt == "text":
                        b["text"] = tag("text") if t == "assistant" else tag("prompt")
                    elif bt == "thinking":
                        b["thinking"], b["signature"] = "", ""
                    elif bt == "tool_use":
                        b["input"] = redact_input(b.get("input", {}))
                    elif bt == "tool_result":
                        b["content"] = "[redacted]"
                    elif bt in ("server_tool_use", "tool_search_tool_result"):
                        b.pop("input", None)
                        b.pop("content", None)
        # A second copy of every tool input, keyed by tool_use id.
        if isinstance(o.get("wireToolInputs"), dict):
            o["wireToolInputs"] = {k: redact_input(v) for k, v in o["wireToolInputs"].items()}
        tur = o.get("toolUseResult")
        if isinstance(tur, dict):
            o["toolUseResult"] = {k: (keep_path(tur[k]) if k == "filePath" else tur[k]) for k in ("type", "filePath") if k in tur}
        elif tur is not None:
            o["toolUseResult"] = "[redacted]"
        line_out = scrub(json.dumps(o, ensure_ascii=False))
        if FORBIDDEN.search(line_out) or re.search(rf"\b{re.escape(user)}\b", line_out, re.I):
            raise SystemExit(f"unredacted data survived in: {line_out[:200]}")
        out.append(line_out)

    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    print(f"{len(out)} lines, {counters['prompt']} prompts, {counters['text']} texts -> {args.out}")


if __name__ == "__main__":
    main()
