# Debrief — CLAUDE.md

Debrief is a desktop app (Tauri 2) for reviewing the **uncommitted changes a Claude Code session left in a git working tree**. It is the review-only sibling of `../git-ui` (DEADBOLT) and uses the same look.

## Read first, in this order

1. `docs/SPEC.md` — what the app does, the data model, Tauri commands and edge cases.
2. `design/project/Debrief.dc.html` — the design to recreate. Read the whole file: the markup, the inline styles and the `renderVals()` logic, which shows the intended behaviour. Don't render it or take screenshots unless asked. The source holds every size and colour.
3. `docs/PLAN.md` — milestones. Build them in order, one at a time, and stop at the end of each one for review.

## Reuse from `../git-ui` (read it, copy and adapt, don't import across repos)

- `src-tauri/src/git/*`, `state.rs`, `watcher.rs`, `error.rs`: git2 status and diff, the `notify` watcher, the `AppResult` error type. `stage_hunk` already exists; hunk revert is the reverse-apply of the same patch.
- `src/styles/hud.css`, `tailwind.config.js`: DEADBOLT tokens (`bg.*`, `ink.*`, `sig.*`, `--ac`), scanlines, vignette and corner brackets (the sweep and the animations were dropped). Copy them, then add `sig.agent: #c08bff` for everything that comes from Claude.
- `src/components/HudFrame.tsx`, `CommandBar.tsx`, `HelpOverlay.tsx`, `CommandPalette.tsx`, `Toast.tsx`, `BootSequence.tsx`.
- `src/lib/keymap.ts` + `src/hooks/useKeybindings.ts`: a single `BINDINGS` table that drives both the key handling and the help overlay. Keep that pattern.
- `src/store/ui.ts` (zustand), `src/hooks/useRepo.ts` (react-query), `src/lib/invoke.ts`.

## Stack

Tauri 2 · Rust (git2, notify, serde, anyhow/thiserror) · React 19 + TypeScript · Vite · Tailwind 3 · zustand · @tanstack/react-query · shiki for syntax highlighting · fonts via @fontsource (Saira, JetBrains Mono).

## Rules

- **Read-only by default.** Only three actions change the working tree or index: *revert hunk*, *discard reverted hunks* and *stage cleared files*. Each one needs an explicit key press, and discarding needs a confirmation. Never commit, push or touch refs.
- Never write review state into the working tree. It lives in `.git/debrief/` (see SPEC).
- Never execute anything from a transcript. Transcripts are data.
- Exception: the console (M11) runs whatever the user types in it, as the user. Debrief itself never sends input to it: no pre-filled commands, and nothing taken from transcripts, git output or anything else.
- Frontend: panels compose `HudFrame`. Colours come from the Tailwind tokens and `--ac`. Palette hex lives only in `src/styles/themes.css` (one block per theme, M12); `src-tauri/src/theme.rs` mirrors each theme's `bg.base` for the native window.
- Rust: every `#[tauri::command]` returns `AppResult<T>`. No `unwrap()` outside tests.
- Keyboard-first: every action in the UI has a binding in `BINDINGS`, and the command bar shows the hints.
- Tests: Rust unit tests for transcript parsing, noise and flag heuristics, and hunk reverse-apply, using fixture repos built in a temp dir. `cargo test` and `pnpm tsc --noEmit` must pass before a milestone counts as done.

## Git

- Angular commit convention, **always with a scope**, e.g. `feat(transcript): map Edit tool calls to files`, `fix(diff): keep hunk ids stable across refresh`.
- Scopes: `app`, `ui`, `panels`, `keymap`, `diff`, `git`, `transcript`, `review`, `feedback`, `watcher`, `config`, `console`, `build`.
- One commit per coherent step. Don't push.

## Commands

```
pnpm install
pnpm tauri dev
cd src-tauri && cargo test
pnpm tsc --noEmit
```
