export type PanelId = "changeset" | "diff" | "notes";

export type HelpGroup = "NAVIGATION" | "REVIEW" | "NOTES" | "DIFF" | "FILTER" | "SESSION" | "LOOK" | "GLOBAL";

export interface Binding {
  /** KeyboardEvent.key, or "ctrl+x"; "" for palette-only commands. */
  key: string;
  panel: PanelId | "global";
  action: string; // logical action id
  desc: string;
  group: HelpGroup;
  /** Shown in the command bar, in table order. */
  hint?: { keys: string; label: string };
  /** Listed in the command palette as `:cmd`. */
  cmd?: string;
}

// Single source of truth — consumed by useKeybindings, HelpOverlay and CommandBar
export const BINDINGS: Binding[] = [
  // Files. j/k follow the list when it holds focus; J/K work from anywhere.
  { key: "j", panel: "changeset", action: "file.next", desc: "Next file", group: "NAVIGATION", hint: { keys: "j/k", label: "file" } },
  { key: "k", panel: "changeset", action: "file.prev", desc: "Previous file", group: "NAVIGATION" },
  { key: "ArrowDown", panel: "changeset", action: "file.next", desc: "Next file", group: "NAVIGATION" },
  { key: "ArrowUp", panel: "changeset", action: "file.prev", desc: "Previous file", group: "NAVIGATION" },
  { key: "g", panel: "changeset", action: "file.first", desc: "First file", group: "NAVIGATION" },
  { key: "G", panel: "changeset", action: "file.last", desc: "Last file", group: "NAVIGATION" },
  { key: "J", panel: "global", action: "file.next", desc: "Next file", group: "NAVIGATION" },
  { key: "K", panel: "global", action: "file.prev", desc: "Previous file", group: "NAVIGATION" },
  { key: "z", panel: "global", action: "tree.fold", desc: "Fold / unfold folder (tree)", group: "NAVIGATION" },
  { key: "Z", panel: "global", action: "tree.unfoldAll", desc: "Unfold all folders (tree)", group: "NAVIGATION", cmd: "unfold" },
  { key: "Tab", panel: "global", action: "focus.next", desc: "Next panel", group: "NAVIGATION" },
  { key: "l", panel: "global", action: "focus.next", desc: "Next panel", group: "NAVIGATION" },
  { key: "ArrowRight", panel: "global", action: "focus.next", desc: "Next panel", group: "NAVIGATION" },
  { key: "h", panel: "global", action: "focus.prev", desc: "Previous panel", group: "NAVIGATION" },
  { key: "ArrowLeft", panel: "global", action: "focus.prev", desc: "Previous panel", group: "NAVIGATION" },

  // Review (SPEC §5)
  { key: " ", panel: "global", action: "file.clear", desc: "Clear & next open file", group: "REVIEW", hint: { keys: "spc", label: "clear" } },
  { key: "v", panel: "global", action: "file.viewed", desc: "Toggle viewed", group: "REVIEW" },
  { key: "y", panel: "global", action: "hunk.keep", desc: "Keep hunk (again: undecided)", group: "REVIEW", hint: { keys: "y", label: "keep" } },
  { key: "x", panel: "global", action: "hunk.revert", desc: "Mark hunk for revert (again: undecided)", group: "REVIEW", hint: { keys: "x", label: "revert" } },
  { key: "a", panel: "global", action: "ops.stage", desc: "Stage cleared files (minus reverted hunks)", group: "REVIEW", cmd: "stage" },
  { key: "d", panel: "global", action: "ops.discard", desc: "Discard reverted hunks (asks first)", group: "REVIEW", cmd: "discard" },

  // Field notes (SPEC §6)
  { key: "n", panel: "global", action: "note.new", desc: "Note on hunk under cursor (else file)", group: "NOTES", hint: { keys: "n", label: "note" } },
  { key: "N", panel: "global", action: "note.newFile", desc: "Note on the whole file", group: "NOTES" },
  { key: "f", panel: "global", action: "notes.transmit", desc: "Transmit notes to Claude", group: "NOTES", hint: { keys: "f", label: "transmit" }, cmd: "transmit" },
  { key: "j", panel: "notes", action: "notes.down", desc: "Next queued note", group: "NOTES" },
  { key: "k", panel: "notes", action: "notes.up", desc: "Previous queued note", group: "NOTES" },
  { key: "Backspace", panel: "notes", action: "notes.remove", desc: "Remove queued note", group: "NOTES" },
  { key: "Delete", panel: "notes", action: "notes.remove", desc: "Remove queued note", group: "NOTES" },
  { key: "ctrl+c", panel: "global", action: "transmit.cancel", desc: "Stop a running resume", group: "NOTES" },
  { key: "", panel: "global", action: "mode.clipboard", desc: "Transmit mode: clipboard", group: "NOTES", cmd: "transmit-mode clipboard" },
  { key: "", panel: "global", action: "mode.file", desc: "Transmit mode: .git/debrief/feedback.md", group: "NOTES", cmd: "transmit-mode file" },
  { key: "", panel: "global", action: "mode.resume", desc: "Transmit mode: claude --resume (opt-in)", group: "NOTES", cmd: "transmit-mode resume" },

  // Diff — vim scrolling, as in git-ui
  { key: "j", panel: "diff", action: "diff.lineDown", desc: "Line down", group: "DIFF" },
  { key: "k", panel: "diff", action: "diff.lineUp", desc: "Line up", group: "DIFF" },
  { key: "ArrowDown", panel: "diff", action: "diff.lineDown", desc: "Line down", group: "DIFF" },
  { key: "ArrowUp", panel: "diff", action: "diff.lineUp", desc: "Line up", group: "DIFF" },
  { key: "g", panel: "diff", action: "diff.top", desc: "Top", group: "DIFF" },
  { key: "G", panel: "diff", action: "diff.bottom", desc: "Bottom", group: "DIFF" },
  { key: "ctrl+d", panel: "global", action: "diff.halfDown", desc: "Half page down", group: "DIFF" },
  { key: "ctrl+u", panel: "global", action: "diff.halfUp", desc: "Half page up", group: "DIFF" },
  { key: "ctrl+f", panel: "global", action: "diff.pageDown", desc: "Page down", group: "DIFF" },
  { key: "ctrl+b", panel: "global", action: "diff.pageUp", desc: "Page up", group: "DIFF" },
  { key: "]", panel: "global", action: "hunk.next", desc: "Next hunk", group: "DIFF", hint: { keys: "]/[", label: "hunk" } },
  { key: "[", panel: "global", action: "hunk.prev", desc: "Previous hunk", group: "DIFF" },
  { key: "o", panel: "global", action: "hunk.expand", desc: "Expand / collapse a large hunk", group: "DIFF" },

  // Filters
  { key: "/", panel: "global", action: "filter.query", desc: "Filter by path", group: "FILTER" },
  { key: "1", panel: "global", action: "filter.all", desc: "Show all files", group: "FILTER", cmd: "all" },
  { key: "2", panel: "global", action: "filter.open", desc: "Show open files", group: "FILTER", cmd: "open-files" },
  { key: "3", panel: "global", action: "filter.flagged", desc: "Show flagged files", group: "FILTER", cmd: "flagged" },
  { key: "m", panel: "global", action: "filter.mask", desc: "Mask generated & lockfiles", group: "FILTER", hint: { keys: "m", label: "mask" }, cmd: "mask" },
  { key: "!", panel: "global", action: "flag.jump", desc: "Jump to flagged line", group: "FILTER" },

  // Session and grouping
  { key: "t", panel: "global", action: "grouping.toggle", desc: "Group by intent / file tree", group: "SESSION", hint: { keys: "t", label: "tree" }, cmd: "tree" },
  { key: "S", panel: "global", action: "session.pick", desc: "Pick Claude session", group: "SESSION", hint: { keys: ":session", label: "session" }, cmd: "session" },
  { key: ":", panel: "global", action: "palette.open", desc: "Command palette", group: "SESSION" },

  // Look (SPEC §2 tweaks), palette only
  { key: "", panel: "global", action: "accent.cyan", desc: "Accent: cyan", group: "LOOK", cmd: "accent cyan" },
  { key: "", panel: "global", action: "accent.green", desc: "Accent: green", group: "LOOK", cmd: "accent green" },
  { key: "", panel: "global", action: "accent.amber", desc: "Accent: amber", group: "LOOK", cmd: "accent amber" },
  { key: "", panel: "global", action: "accent.red", desc: "Accent: red", group: "LOOK", cmd: "accent red" },
  { key: "", panel: "global", action: "scanlines.toggle", desc: "Scanlines on / off", group: "LOOK", cmd: "scanlines" },
  { key: "", panel: "global", action: "settings.claudePath", desc: "Path to claude CLI (empty: auto-detect)", group: "NOTES", cmd: "claude-path <path>" },

  // Global
  { key: "⌘O", panel: "global", action: "repo.open", desc: "Open repository", group: "GLOBAL", cmd: "open" },
  { key: "R", panel: "global", action: "refresh", desc: "Refresh", group: "GLOBAL", cmd: "refresh" },
  { key: "?", panel: "global", action: "help.toggle", desc: "Help", group: "GLOBAL", hint: { keys: "?", label: "help" }, cmd: "help" },
  { key: "Escape", panel: "global", action: "close", desc: "Close / cancel", group: "GLOBAL" },
];

export function findBinding(panel: PanelId, key: string): Binding | undefined {
  if (!key) return undefined;
  return (
    BINDINGS.find((b) => b.panel === panel && b.key === key) ||
    BINDINGS.find((b) => b.panel === "global" && b.key === key)
  );
}

/** First key bound to `action`, formatted for a button label. */
export function keyFor(action: string): string {
  const b = BINDINGS.find((x) => x.action === action);
  return b ? keyLabel(b.key) : "";
}

export function keyLabel(key: string): string {
  if (key === " ") return "space";
  if (key === "Escape") return "esc";
  if (key.startsWith("ctrl+")) return "^" + key.slice(5);
  return (
    { ArrowDown: "↓", ArrowUp: "↑", ArrowLeft: "←", ArrowRight: "→", Tab: "tab" } as Record<string, string>
  )[key] ?? key;
}
