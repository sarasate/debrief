# Debrief

Review what a Claude Code session changed in your working tree before you commit, in the DEADBOLT HUD style of `../git-ui`.

Status: **handoff bundle only**. There's no code yet.

- `CLAUDE.md`: instructions for Claude Code
- `docs/SPEC.md`: product and technical spec
- `docs/PLAN.md`: milestones M1–M7, each with a ready-to-paste prompt
- `design/`: interactive design prototype + the original DEADBOLT reference

Start:

```
cd ~/Workspace/Personal/debrief
git init && git add -A && git commit -m "docs(app): add debrief handoff bundle"
claude
```
Then paste the M1 prompt from `docs/PLAN.md`.
