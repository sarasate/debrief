# Debrief

Review what a Claude Code session changed in your working tree before you commit, in the DEADBOLT HUD style of `../git-ui`.

Debrief reads the repo's uncommitted changes and the session's transcript under `~/.claude/projects`, groups the files by the prompt that produced them, flags what needs a closer look, and lets you clear files, keep or revert hunks, stage what you cleared and send one batch of notes back to Claude. Review state lives in `.git/debrief/`, never in the working tree.

```
pnpm install
pnpm tauri dev          # port 1430
cd src-tauri && cargo test
pnpm tsc --noEmit
```

Press `?` in the app for every key. Start with `space` (clear & next), `y` / `x` (keep / revert hunk), `n` (note), `f` (transmit), `t` (intent ↔ tree) and `:` (palette: `:session`, `:transmit-mode`, `:accent`, `:scanlines`, `:claude-path`).

- `docs/SPEC.md`: product and technical spec, updated as built
- `docs/PLAN.md`: milestones M1–M7
- `design/`: the design prototype, the DEADBOLT reference and the app icon source (`scripts/make-icon.py`)
- `.debrief.toml` at a repo root overrides the noise globs (SPEC §3.4)
