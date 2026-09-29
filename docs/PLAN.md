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

### Later (v2)
- Claude-clustered intents (SPEC §3.3 v2), cached per diff hash.
- A PostToolUse hook installer that writes `.git/debrief/ledger.jsonl` live, as a more robust alternative to transcript parsing.
- Split diff view, and "open in editor" at the hunk line.
