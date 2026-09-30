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

## Branch & PR review (M8–M10)

Review what a feature branch or PR changed, not just what's uncommitted. Same screen, same review flow, but over commits Claude already made.

### Design

**Review target.** Today everything reads one diff: `git::diff::worktree_diff` (HEAD → worktree, used by `status`, `diff_file` and `apply`). Replace that with a target:

```rust
enum Target {
    Worktree,                                         // today: HEAD → working tree
    Range { base: String, head: String, worktree: bool }, // merge-base(base, head) → head
}
```

- A range diffs `merge-base(base, head)` against `head`: the three-dot view GitHub uses for PRs, so commits that landed on `main` after the branch point don't show up. With `worktree: true` (only when `head` is the checked-out branch) it goes on to the working tree, so "the branch plus what's still uncommitted" is one review.
- The target lives in app state next to the repo path, and the UI picks it. Every command that reads changes goes through it.
- **Base detection:** a PR's base branch when reviewing a PR, else `origin/HEAD`, else the first of `main` / `master` / `trunk` that exists. It can be overridden in the picker.
- Hunk ids (`path + body`) already don't depend on line numbers, so they work unchanged across rebases, as long as the hunk's content is the same.

**Review state per target.** `.git/debrief/<key>.json`, with `key` = session id (worktree, as today) or `branch-<sha1(head-ref + base)>` for a range. Viewed marks store the **blob oid at `head`** instead of the worktree file, so a new commit that touches a cleared file reopens it, and a force-push or rebase that leaves the file unchanged keeps it cleared.

**Attribution across a branch.** One branch is usually several sessions:
1. Edits, not sessions (changed during M9): every ledger edit records the `gitBranch` of its own line (`HEAD` means none). A branch takes the edits of all repo sessions made after the merge-base commit time; for any file with edits recorded on the branch itself, only those count. Filtering whole sessions by branch would have been wrong: a real session made 21 edits on `main`, then switched to the feature branch for 9 more, and its first branch commit held the `main` edits.
2. The ledger is the union of those sessions' edits. `drop_committed` (right for the worktree, where committed work can't explain what's dirty) is **off** in range mode, since committed work is exactly what's being reviewed.
3. A commit whose message has a `Co-Authored-By: Claude` trailer counts as Claude's even without a ledger entry. That covers `Bash` side effects and sessions whose transcript is gone.
4. Anything else is UNATTRIBUTED, as today.

**Grouping.** BY INTENT becomes session → turn (the group title gets a session prefix when there's more than one). New **BY COMMIT** mode: one group per non-merge commit in `base..head`, oldest first, titled with the commit subject, briefing = commit body plus author plus trailer. `t` cycles intent → commit → tree.

**What stays the same.** Noise, flags (rule 3's "lines before" come from the base blob), notes, transmit and the key flow. The prompt names the branch and range instead of "uncommitted changes".

**What changes for actions.** Committed changes can't be "discarded" from the worktree safely.
- `x` still records a revert verdict, and **transmit lists reverted hunks as requests** ("please revert these in a new commit") instead of "already discarded".
- `a` (stage) and `d` (discard) act only on uncommitted hunks: in a range with `worktree: true` they work on the part past `head`; committed hunks are read-only and say so in the command bar. With `worktree: false` both keys are disabled.
- CLAUDE.md's rule stands: never commit, push or move refs.

**PRs.** `gh pr list` / `gh pr view --json number,title,body,headRefName,headRefOid,baseRefName,url` are read-only metadata calls. A PR becomes `Range { base: baseRefName, head: headRefOid }`, with its number, title and URL in the status strip, and its body as the briefing of a PR group. They only work when `gh` is installed and logged in; otherwise the PR list is hidden, not an error.

**UI.**
- **Picker:** `B` / `:target` opens a palette with Worktree, local branches (ahead count vs base) and open PRs.
- **Status strip:** `BRANCH feat/x ← main · 12 COMMITS · PR #42`.
- **Tape:** `// BRANCH REVIEW // COMMITTED CHANGES // …`.
- **Watcher:** it already watches `refs/`, so a new commit or a fetch refreshes a range; in range mode, worktree edits only matter when `worktree: true`.

**Limits.**
- **Size:** warn above 500 changed files, and cap the diff so a branch that rewrote a vendored dir doesn't hang the UI (the existing 300-line hunk collapse still applies).
- **Merges:** merge commits are skipped in BY COMMIT; their changes still show in the range diff.

### Decisions (settled 2026-09-29)

1. **PR heads that aren't local are not fetched.** Debrief never runs `git fetch`; such a PR shows "not fetched" and the command to run (`git fetch origin pull/42/head`, or the branch name for same-repo PRs). A Debrief-owned fetch into `refs/debrief/pr/*` stays a possible later addition.
2. **`a` / `d` work on the uncommitted part of a checked-out branch.** In a range with `worktree: true`, staging and discarding apply to hunks past `head` only, exactly as in worktree mode; committed hunks can be marked but not staged or discarded.

### M8 — Review target + range diff
**Prompt:**
> Implement the Target from docs/PLAN.md "Branch & PR review": `git::diff` diffs merge-base(base, head) → head (optionally on to the worktree when head is checked out), with rename detection and the same hunk ids. Route status, diff_file, the flags scan and the review model through the current target; `apply` (stage/discard) keeps working on the worktree part only (decision 2). Key review state by target and store viewed oids from the head tree in range mode. Add base detection, `target_list` (worktree + local branches with ahead counts) and `target_set`, the `B` / `:target` picker, and the status strip and tape for branches. In a range, `a` / `d` act on uncommitted hunks only and explain in the command bar when the hunk under the cursor is committed (decision 2). Tests on temp repos: a branch with 3 commits against a base that moved on (only branch changes show); a rebase that keeps a hunk keeps its id and viewed mark; a new commit to a cleared file reopens it.

Done when: I can pick a local feature branch and review everything it changed since it left main, and switch back to the worktree without losing either review's progress.

### M9 — Branch attribution + BY COMMIT
**Prompt:**
> Parse `gitBranch` from transcripts. In range mode, build the ledger from every session of the repo on that branch with activity after the merge-base, don't drop committed edits, and count commits with a `Co-Authored-By: Claude` trailer as Claude's. Group by session → turn in BY INTENT, add a BY COMMIT mode (non-merge commits oldest first, subject as title, body as briefing) and make `t` cycle intent → commit → tree. Transmit in range mode names the branch and lists revert verdicts as requests. Fixture: a redacted multi-session branch plus a hand-written trailer-only commit.

Done when: a branch built over two Claude sessions shows its files under the right prompts, a commit Claude co-authored via Bash is attributed, and my own commits show under UNATTRIBUTED.

### M10 — Pull requests
**Prompt:**
> Add open PRs to the target picker via `gh pr list/view --json …` (read-only; hidden when gh is missing or logged out). A PR is a range from its base to its head commit; show number, title and URL in the status strip and its body as a PR briefing group. Per decision 1, never fetch: a PR whose head commit isn't local shows "not fetched" and the `git fetch` command to run instead of a diff. Tests mock `gh` with a fixture script on PATH.

Done when: I can open a Claude-authored PR from the palette, review it with intent groups and flags, and send notes to the session that wrote it.

---

## Console (M11)

A real shell in the current repo, one shortcut away, for the quick things a review needs: run the tests, `git log -p` a file, try the thing Claude built. It opens in the repo root (the worktree being reviewed, not the app's own directory) and keeps running while you review.

### Design

**Shell.** A real pseudo-terminal, not a one-shot command runner, so interactive programs work: `vim`, `less`, `git add -p`, test watchers and colours.
- **Backend:** `portable-pty` spawns `$SHELL -l` (the login shell, since a macOS GUI app starts with a bare PATH, the same problem `claude` had in M6), falling back to `/bin/zsh`.
- **Environment:** the repo root as working directory, `TERM=xterm-256color`, and `DEBRIEF=1` so a prompt can show it's inside Debrief.
- **Frontend:** xterm.js (`@xterm/xterm` + `@xterm/addon-fit`) renders it.

**Streaming.** PTY output goes to the webview through a Tauri `Channel` as raw bytes, not the event bus: output can be heavy (a test run), and bytes avoid splitting a UTF-8 character across chunks. Keystrokes go back through `console_write`. Commands:

```
console_open(cols, rows, on_output: Channel<bytes>) -> id   // one per repo
console_write(id, data)
console_resize(id, cols, rows)
console_close(id)                                           // kills the shell and its children
```

**Lifetime.**
- **One console per repo**, started on first open. Hiding it doesn't stop the shell, so a running command keeps going, and showing it again brings back the scrollback.
- **Stopped:** opening another repo or quitting the app kills it and its process group. A shell that exits on its own (`exit`) shows "shell exited · ⌘⇧J to restart".
- **Watcher:** it needs no changes. Files the console writes refresh the review through the existing watcher, like any other edit.

**Layout.** A drawer that floats over the panels, anchored just above the command bar, so opening it never reflows the review (changed during M11; it first pushed the panels up). The resume output drawer is hidden while the console is open.
- **Height:** starts at 40% of the window, resized by dragging its top edge, and remembered in settings.
- **Look:** styled as a `HudFrame` (`▣ CONSOLE · <repo> · <shell>`). xterm's ANSI palette comes from CSS variables in `hud.css`, so no raw hex goes into components.

**Keys.**
- **`⌘J`** toggles the console from anywhere, including inside it (shells never receive ⌘ combos, as in VS Code's panel toggle). Hidden: it opens and takes focus. Showing: it hides and focus returns to the review. `:console` in the palette does the same.
- **While it's showing**, the console joins the panel cycle, so `h` / `l` / `Tab` from the review can move focus into it without hiding the review. Clicking it focuses it too.
- **Inside the console, every other key goes to the shell,** including Escape (for vim), `ctrl+c`, `ctrl+d` and the single-letter review keys. The global handler already ignores text fields; it gets an explicit rule so Escape doesn't blur the terminal.
- **`⌘⇧J`** restarts a shell that has exited, or kills a stuck one after asking.
- **`^c` conflict:** M6's `^c` (stop a resume) only applies outside the console.

**Copy and paste (macOS).** The console follows Terminal.app and iTerm; `ctrl` belongs to the shell, `⌘` to the app:

| Keys | In the console |
|---|---|
| `⌘C` | Copy the selection. With nothing selected it does nothing; it never interrupts. |
| `ctrl+c` | Interrupt the running command (SIGINT), as in any terminal. |
| `⌘V` | Paste, as a bracketed paste so shells don't run pasted lines one by one. |
| `⌘A` | Select all scrollback. |
| `⌘K` | Clear the screen and scrollback. |
| `⌥` + key | Sent as Meta (xterm's `macOptionIsMeta`), so readline's `⌥B` / `⌥F` word jumps work. |

- **Selecting:** a drag selects, and selecting doesn't copy on its own.
- **Relies on:** ⌘C / ⌘V reach xterm through the macOS Edit menu Tauri installs by default (the same path text fields use today), and the global key handler already ignores `⌘` combos. M11 checks both by hand, since a headless test can't press ⌘C.

**Safety, and CLAUDE.md.** The console is the one place Debrief runs arbitrary commands, so the rules need an explicit exception rather than an implied one:
- **Only you type.** Debrief never writes into the console on its own: no pre-filled commands, and no "run this" buttons built from transcript `Bash` commands, git output or anything else. "Transcripts are data" still holds.
- **CLAUDE.md gets one line:** *"Exception: the console (M11) runs whatever the user types in it, as the user; Debrief itself never sends input to it."*
- **Quitting with a command still running** asks first.

### Decisions (settled 2026-09-30)

1. **A drawer in the main window.** "Pop out to its own window" is a later addition.
2. **`⌘J` toggles the console**, from the review and from inside it. `⌘M` stays macOS's Minimize, and bare letters and `` ` `` stay with the review and the shell.
3. **`O` opens the repo in your terminal app** (Terminal, iTerm or Ghostty via macOS `open`; which one is a setting, Terminal by default).
4. **`⌘C` copies and `ctrl+c` interrupts**, as in Terminal.app (see "Copy and paste").

### M11 — Console
**Prompt:**
> Implement the console from docs/PLAN.md "Console (M11)": a `console` Rust module on portable-pty (login shell, repo root as working directory, TERM=xterm-256color, output streamed over a Tauri Channel as bytes, resize, kill the process group on close, repo switch and app exit), the four console commands, and a resizable drawer with xterm.js + fit addon themed from CSS variables in hud.css. Bindings in BINDINGS: `⌘J` toggle from anywhere (xterm hands it back via attachCustomKeyEventHandler; also `:console`), `⌘⇧J` restart, the console in the h/l/Tab panel cycle while it shows, `O` open the repo in the terminal app from settings; every other key goes to the shell when it has focus, including Escape. macOS keys as in "Copy and paste": `⌘C` copies (never interrupts), `ctrl+c` interrupts, `⌘V` bracketed paste, `⌘A`, `⌘K` clear, `⌥` as Meta. Add the CLAUDE.md exception line. Apply the decisions recorded in the plan. Tests: spawn a shell in a temp dir, write `pwd` and `echo $TERM`, read both back; resize; close kills a running `sleep`.

Done when: `⌘J` opens a shell in the repo under review and `⌘J` again takes me back, I can run the tests and use `vim` in it, hide it with a command still running and bring it back with the output intact, the review refreshes from what the command changed, and `⌘C` / `⌘V` copy and paste while `ctrl+c` stops a running command.

---

## Themes (M12)

Debrief has one look today: DEADBOLT, dark, with four accents. M12 makes the palette themeable. It adds **Daylight**, a light theme picked automatically when macOS is in light mode, and **Ember**, a second dark theme that proves the system handles more than a light/dark pair. The accent stays a separate setting that works on top of any theme.

### Design

**Where colours live today.** The Tailwind tokens (`bg.*`, `ink.*`, `sig.*`) are hard-coded hex in `tailwind.config.js`. `hud.css` has its own hex and rgba values for the body, the root gradient, the diff lines, the shiki tokens, the `--term-*` ANSI palette, the flag flash, the scanlines and the vignette. Components are already clean: they only use tokens, `--ac` and `theme(colors.…)`. That means the work is in the config and the CSS, not in the panels.

**Tokens become variables.**
- Each token becomes an RGB channel variable, so Tailwind's alpha modifiers keep working: `base: "rgb(var(--bg-base) / <alpha-value>)"`, with `--bg-base: 6 9 12`. The 38 uses like `border-sig-warn/45` and `bg-ink-void/[.82]`, and the `theme(colors.sig.warn/20%)` shadows, stay as they are. Step 1 checks the `theme()` shadows against Tailwind 3.4 before converting the rest.
- A new `src/styles/themes.css` holds one block per theme: `:root[data-theme="deadbolt"] { … }`, then `daylight` and `ember`. It is the only file with palette hex. `hud.css` refers to variables only: the diff lines, shiki, `--term-*`, the flash and a few new effect variables.
- **Token names stay.** In a light theme, `ink.darkest` means "least contrast", not literally dark, and `bg.deep` means "recessed". A comment at the top of `themes.css` explains this, so no component needs renaming.
- **Accent per theme.** A cyan of `#3df0ff` can't be read on a light background. Each theme defines its own four accents, as `--ac-cyan` … `--ac-red`, and `data-accent` picks one (Daylight's cyan is `#077a8c`). The dark themes share the DEADBOLT accents. `ink.void` (text on an accent fill) becomes light in Daylight, because the accent fill there is dark.

**HUD effects as variables.** The effects were tuned for black and look dirty on paper. Each one gets its values from the theme:

| Effect | DEADBOLT / Ember | Daylight |
|---|---|---|
| Scanlines `--fx-scan` | `rgba(0,0,0,.16)` multiply | `rgba(0,0,0,.035)` |
| Vignette `--fx-vignette` | `inset 0 0 220px rgba(0,0,0,.72)` | `inset 0 0 160px rgba(0,0,0,.07)` |
The scanlines setting still turns scanlines off in every theme. The animated sweep, scanline crawl and flicker were dropped after M12: fixing their keyframes showed they weren't worth it.

**Palettes.** These are starting values, adjusted when reviewing the screenshots at the end of the milestone. The design file only has the dark look, so Daylight is derived from it: the same structure with the lightness flipped and the signal colours darkened until they reach about 4.5:1 contrast on `bg.panel`.

| Token | DEADBOLT (today) | Daylight | Ember |
|---|---|---|---|
| `bg.base` / `panel` / `deep` | `#06090c` / `#080d10` / `#05080b` | `#e9eef0` / `#f7fafa` / `#e1e8ea` | `#0c0907` / `#110c09` / `#090605` |
| `bg.grad1` / `grad2` | `#0d161d` / `#080f14` | `#ffffff` / `#eef3f4` | `#1a120d` / `#110b08` |
| `ink.base` / `bright` / `dim` | `#bcccd0` / `#eaf6f8` / `#8aa0a6` | `#2c3a3f` / `#0b1418` / `#50656b` | `#d6c6b4` / `#fbefe2` / `#a8927e` |
| `ink.dimmer` / `darkest` / `deepest` | `#5a6e72` / `#46585e` / `#3a4a50` | `#6f8388` / `#8a9ca1` / `#a3b2b6` | `#75624f` / `#5d4d3f` / `#4a3d32` |
| `ink.void` | `#04080b` | `#f7fafa` | `#0a0604` |
| `sig.warn` / `danger` / `ok` | `#ffb000` / `#ff5a3c` / `#7fd49a` | `#94600a` / `#c8341c` / `#267a40` | `#ffb000` / `#ff6a45` / `#9fd48a` |
| `sig.add` / `delete` | `#7fd49a` / `#d98a7d` | `#2a7d43` / `#b24a3a` | `#9fd48a` / `#e0907a` |
| `sig.agent` | `#c08bff` | `#7b3fd0` | `#d49bff` |
| accent cyan / green / amber / red | `#3df0ff` / `#39ff7d` / `#ffb000` / `#ff5a3c` | `#077a8c` / `#147a3d` / `#94600a` / `#c8341c` | as DEADBOLT |

The remaining tokens (`ink.light`, `mid`, `label`, `faint`, `agentVoid`, `sig.okDark`, `warnInk`, `deleteHi`, `dangerInk`, `dangerDark`) follow the same pattern. The shiki tokens and the 16 ANSI colours get a set per theme; Daylight's are the xterm "light" defaults, shifted towards the HUD hues.

Ember is a warm charcoal and parchment dark, easy on the eyes at night. Its signal colours are close to DEADBOLT's, so flags, adds and deletes still look the same. It has a different base hue, which checks that nothing still assumes the cyan-grey palette.

**Choosing the theme.** Two settings instead of one flat list, because "follow the system" needs to know which dark theme to use:

```rust
enum ThemeMode { System, Light, Dark }   // default System
enum DarkTheme { Deadbolt, Ember }       // default Deadbolt
```

- **System:** Daylight when macOS is light, `dark_theme` when it's dark. It changes live when the system appearance changes, including macOS "Auto" at sunset.
- **Light / Dark:** fixed to Daylight or `dark_theme`.
- Both settings use `#[serde(default)]`, so an existing `settings.json` loads without changes and keeps today's look on a dark Mac.

**Resolving it.** `src/lib/theme.ts` turns `(mode, dark_theme, system appearance)` into a theme name. It writes `data-theme` and `color-scheme` (for native scrollbars and form controls) on `<html>`, next to the `data-accent` and `data-scanlines` that `App.tsx` already sets.
- **System appearance:** `matchMedia("(prefers-color-scheme: dark)")` with a change listener. The window's native appearance follows the resolved theme through `getCurrentWindow().setTheme(…)` (`null` in System mode), so the title bar and traffic lights match the page.
- **No flash at startup:** settings come from an async `invoke`, so the first paint can't wait for them. In Rust `setup`, the window theme and background colour are set from the stored settings before the window shows. A small inline script in `index.html` sets `data-theme` from `matchMedia` and the last resolved theme (a `localStorage` cache, used only for this), so the first frame is already right. The settings file stays the source of truth.
- **Console:** `ConsolePanel` already re-reads `readTheme()` when the accent changes. It now also re-reads when the resolved theme changes, so a running shell gets the new colours without restarting.

**Keys.** These are palette commands in the `LOOK` group of `BINDINGS`, like the accents, so they show up in the help overlay: `:theme system`, `:theme light`, `:theme dark`, `:theme deadbolt`, `:theme ember`. Picking a named theme sets both settings at once: `ember` sets mode Dark with dark theme Ember, and `daylight` sets mode Light. The command bar confirms with `theme · ember` or `theme · system (daylight)`.

**CLAUDE.md.** The colour rule changes to: *"Colours come from the Tailwind tokens and `--ac`. Palette hex lives only in `src/styles/themes.css`."* The old exception for the diff line classes is no longer needed.

### Decisions (settled 2026-09-30)

1. **The accent setting works in every theme.** Daylight has darkened versions of the four accents; there is no fixed accent per theme.
2. **Palette only.** The `:theme …` commands have no key of their own, like the accents.
3. **The boot sequence follows the theme,** so it boots in Daylight when that's the resolved theme.

### M12 — Themes
**Prompt:**
> Implement docs/PLAN.md "Themes (M12)". First convert the Tailwind tokens to `rgb(var(--…) / <alpha-value>)` and check that `theme(colors.x/NN%)` shadows still compile, then move all palette hex from `tailwind.config.js` and `hud.css` into `src/styles/themes.css` with `deadbolt` (exactly today's values, so nothing visibly changes), `daylight` and `ember` blocks, including per-theme accents, diff and shiki colours, `--term-*` and the `--fx-*` effect variables. Commit that before adding the new themes. Add `theme_mode` (system/light/dark) and `dark_theme` (deadbolt/ember) to Settings with serde defaults and `settings_set` support; `src/lib/theme.ts` resolves the theme from them and `prefers-color-scheme` (live), writes `data-theme` and `color-scheme`, and syncs the native window theme. Set the window theme and background in Rust `setup`, and add the inline pre-paint script to `index.html`. Re-theme the console and the boot sequence. Add the `:theme …` palette entries to BINDINGS, with no key (decision 2). Update the CLAUDE.md colour rule. Tests: settings without the new fields load with System/Deadbolt; round-trip of both fields. Check by hand, with screenshots of each theme for review: all panels, flag banner, discard and quit modals, toast, help overlay, palette, boot sequence, console running `ls --color` and `git log --color`, a diff with syntax highlighting.

Done when: with macOS in light mode Debrief opens in Daylight with no dark flash, switching macOS to dark changes it live to DEADBOLT (or Ember if chosen), `:theme ember` sticks across restarts, the console follows every switch, and DEADBOLT looks exactly as before M12.

---

## Branch history (M13)

A read-only log of the active branch, so the reviewer can see what the branch already holds before the changes under review.

### Design
- **Which log.** The branch under review in a branch review, else the checked-out branch (HEAD, "HEAD" when detached). First-parent only (`git log --first-parent`), newest first, 200 commits a page, more as the list scrolls or the cursor nears the end. The watcher refreshes it like everything under `repo`.
- **Where.** `H` swaps the left panel between the changeset and **HISTORY**; `h`/`l` focus cycling treats it as the left panel. Nothing is written, and it adds no action that changes the working tree.
- **Rows.** A pinned **CURRENT REVIEW** row on top (⏎ on it returns to the review), then per commit: short sha (violet for a Claude co-authored commit), subject, author, age, a `CLAUDE` and a `MERGE` tag, files and `+/−` against the first parent, and the branches, remote branches and tags pointing at it (`origin/HEAD` left out).
- **Merge-base.** The base is the review's, else the detected one (§3 review target) unless that is the branch itself (`main` against `main` or `origin/main`). Commits above the merge-base are the branch's own; a `LEFT <BASE> HERE` divider sits above the first one below it. No base: no divider.
- **Commit view.** ⏎ (or a click) shows the commit in the diff panel: sha, author, date, first parent, refs, subject and body, then every changed file (at most 200; the rest are counted) under a sticky path header, with the usual hunks minus keep/revert. `]`/`[` step through the commit's hunks across files, `e` expands a large one, `o` shows each file whole as it is in that commit. Review actions (`space`, `v`, `y`, `x`, `n`, `N`, `!`) say they don't apply. Esc returns to the review with its scroll and hunk cursor as they were.
- **Filter.** `/` in the log filters the loaded commits by subject, author or sha prefix.
- **Commands.** `branch_log(skip, limit)`, `commit_diff(sha)`; `file_lines` takes an optional `sha`. A sha argument must be hex (4+ chars), so no ref or range syntax reaches revparse.

### M13 — Branch history
Tests: unborn repo (empty log), order and paging, merge-base marking and no base on the trunk, a branch target that isn't checked out, first-parent over a merge, Claude trailer and ref labels, a commit's files (added, modified, deleted), sha-only arguments, `file_lines` at a commit. By hand: `H`, the divider on a feature branch, ⏎ / Esc keeping the review's scroll, `o` inside a commit, paging on a long history.

### Later (v2)
- Claude-clustered intents (SPEC §3.3 v2), cached per diff hash.
- A PostToolUse hook installer that writes `.git/debrief/ledger.jsonl` live, as a more robust alternative to transcript parsing.
- Split diff view, and "open in editor" at the hunk line.
