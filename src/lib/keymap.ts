export type PanelId = "changeset" | "diff";

export type HelpGroup = "NAVIGATION" | "DIFF" | "FILTER" | "GLOBAL";

export interface Binding {
  key: string; // KeyboardEvent.key, or "ctrl+x"
  panel: PanelId | "global";
  action: string; // logical action id
  desc: string;
  group: HelpGroup;
  /** Shown in the command bar, in table order. */
  hint?: { keys: string; label: string };
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
  { key: "z", panel: "global", action: "tree.fold", desc: "Fold / unfold current folder", group: "NAVIGATION" },
  { key: "Z", panel: "global", action: "tree.unfoldAll", desc: "Unfold all folders", group: "NAVIGATION" },
  { key: "Tab", panel: "global", action: "focus.next", desc: "Next panel", group: "NAVIGATION" },
  { key: "l", panel: "global", action: "focus.next", desc: "Next panel", group: "NAVIGATION" },
  { key: "ArrowRight", panel: "global", action: "focus.next", desc: "Next panel", group: "NAVIGATION" },
  { key: "h", panel: "global", action: "focus.prev", desc: "Previous panel", group: "NAVIGATION" },
  { key: "ArrowLeft", panel: "global", action: "focus.prev", desc: "Previous panel", group: "NAVIGATION" },

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

  // Filters
  { key: "/", panel: "global", action: "filter.query", desc: "Filter by path", group: "FILTER", hint: { keys: "/", label: "filter" } },
  { key: "1", panel: "global", action: "filter.all", desc: "Show all files", group: "FILTER" },
  { key: "2", panel: "global", action: "filter.open", desc: "Show open files", group: "FILTER" },
  { key: "3", panel: "global", action: "filter.flagged", desc: "Show flagged files", group: "FILTER" },

  // Global
  { key: "⌘O", panel: "global", action: "repo.open", desc: "Open repository", group: "GLOBAL", hint: { keys: "⌘o", label: "open" } },
  { key: "R", panel: "global", action: "refresh", desc: "Refresh", group: "GLOBAL" },
  { key: "?", panel: "global", action: "help.toggle", desc: "Help", group: "GLOBAL", hint: { keys: "?", label: "help" } },
  { key: "Escape", panel: "global", action: "close", desc: "Close / cancel", group: "GLOBAL" },
];

export function findBinding(panel: PanelId, key: string): Binding | undefined {
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
