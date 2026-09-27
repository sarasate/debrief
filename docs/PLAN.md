# Debrief — Build plan for Claude Code

Build one milestone per Claude Code run. Each one ends with `cargo test` and `pnpm tsc --noEmit` passing, the app starting with `pnpm tauri dev`, commits made (Angular convention with a scope), and a short summary. Then **stop**.

---

## M1 — Scaffold + HUD shell with live git data
**Prompt:**
> Read CLAUDE.md, docs/SPEC.md §2 and design/project/Debrief.dc.html in full. Scaffold a Tauri 2 + React 19 + Vite + Tailwind app named `debrief` here, copying the tooling setup from ../git-ui (tauri.conf, capabilities, tailwind.config, hud.css, fonts). Port HudFrame, CommandBar, Toast and HelpOverlay. Build the static layout from the design: status strip, classified tape, 4 panels and the command bar. Wire `repo_open` (dialog + last-used path) and `repo_status` + `diff_file` (reuse ../git-ui/src-tauri/src/git), so Changeset shows the real dirty files in FILE TREE mode and Diff analysis shows the real diff with stable hunk ids. No transcript yet.

Done when: opening a dirty repo shows its files and diffs in the DEADBOLT look, and the watcher refreshes on save.

## M2 — Transcript ledger + intent groups
**Prompt:**
> Implement SPEC §3.2–3.3. First look at a real transcript under ~/.claude/projects for this machine, adjust the parser to the real shape, and save a redacted fixture in src-tauri/tests/fixtures. Implement sessions_list, ledger_load and the grouping into turns plus UNATTRIBUTED and GENERATED. Make BY INTENT the default mode, with the Agent briefing panel filled from the turn's final assistant message. Add the `:session` picker to the command palette. Also watch the transcript file.

Done when: after a real Claude Code session, files are grouped by the prompts that produced them, and my own edits show under UNATTRIBUTED.

## M3 — Noise + flags + filters
**Prompt:**
> Implement SPEC §3.4 (noise globs + .debrief.toml override) and §3.5 (flag rules 1–6, with unit tests for each). Add the ALL/OPEN/FLAGGED filters with counts, the noise mask toggle, the path filter and the amber flag banner. Secret matches must never show the value.

## M4 — Review state + keyboard flow
**Prompt:**
> Implement SPEC §4 (state in .git/debrief, viewed invalidated by blob oid) and every binding in SPEC §5 except stage/discard/transmit, all driven from one BINDINGS table as in ../git-ui/src/lib/keymap.ts. Add the Ops console counters and progress bar, and `space` = clear & next OPEN file. Command bar messages as in the design.

Done when: I can review a whole changeset without touching the mouse, and quitting and reopening keeps my progress.

## M5 — Verdicts, stage, discard
**Prompt:**
> Implement keep/revert verdicts (y/x) with the dimmed reverted state, `a` stage cleared (skip hunks marked revert: stage the file minus those hunks) and `d` discard reverted, with a confirm modal and single-hunk reverse-apply (SPEC §5). Write tests on a temp repo: revert one hunk of a two-hunk file and assert the other hunk survives.

## M6 — Field notes + transmit
**Prompt:**
> Implement notes (n, file- or hunk-scoped), the Field notes panel and transmit (f) with the three modes in SPEC §6, clipboard by default. Resume mode is opt-in in settings and warns when the transcript was written in the last 60 s.

## M7 — Polish
Boot sequence (from git-ui) with DEBRIEF lines, shiki syntax highlighting inside diff lines (keep the add/del backgrounds), accent + scanlines settings, empty states ("NO SIGNAL · WORKTREE CLEAN"), a large-diff guard (collapse hunks over 300 lines), app icon.

---

### Later (v2)
- Claude-clustered intents (SPEC §3.3 v2), cached per diff hash.
- A PostToolUse hook installer that writes `.git/debrief/ledger.jsonl` live, as a more robust alternative to transcript parsing.
- Split diff view, and "open in editor" at the hunk line.
