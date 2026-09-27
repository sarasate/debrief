# CODING AGENTS: READ THIS FIRST

This is a design handoff bundle made in Claude's Design canvas ("DEADBOLT Debrief").

- **Primary design:** `project/Debrief.dc.html`. It's an interactive HTML prototype of the whole screen at 1440×900. Read it top to bottom. The `<script type="text/x-dc">` block at the end holds sample data and the intended behaviour (grouping, filters, verdicts, notes, progress). Treat it as a behavioural reference, not production code.
- **Visual reference:** `project/DEADBOLT.dc.html` is the original git-ui design this one extends. The shared tokens are already implemented in `../git-ui/src/styles/hud.css` and `../git-ui/tailwind.config.js`, so reuse those instead of re-deriving values from the HTML.
- `project/support.js` is the prototype runtime. Ignore it for the implementation.

Recreate the layout pixel-faithfully in React + Tailwind. Don't render the files or take screenshots unless the user asks: every dimension and colour is in the source.

Design notes:
- Violet `#c08bff` (`sig.violet` in git-ui) marks everything that comes from Claude: the briefing box, the AGENT label, TRANSMIT and the Field notes badge.
- Amber `#ffb000` marks flags and the WORKTREE DIRTY state.
- The centre panel always has the focused-panel treatment in the mock. In the app, focus moves between panels like it does in git-ui.
- The key hints in the mock are labels only. The real bindings are in `docs/SPEC.md` §5.
- The sample data (a realm-scoped auth refactor) is fictional. Don't ship it.
