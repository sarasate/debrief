# Transcript fixtures

- `session-real.jsonl`: a real Claude Code 2.1.x transcript (the session that
  built M1), redacted with `scripts/redact-transcript.py`. Prompts, assistant
  text, thinking, commands, file contents and tool output are blanked; the
  structure, ids, timestamps and file paths are kept, with the home dir
  rewritten to `/Users/dev`.
- `session-variants.jsonl` + `session-variants-agent.jsonl`: hand-written, for
  shapes the real session doesn't contain (old `summary` title, `!` bash echoes,
  task notifications, interrupts, meta lines, failed edits, MultiEdit,
  NotebookEdit, relative paths, a subagent transcript, a half-written last line).
  Line shapes follow real transcripts.
