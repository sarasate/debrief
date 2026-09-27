import { create } from "zustand";
import type { PanelId } from "../lib/keymap";

export type Grouping = "intent" | "tree";
export type FileFilter = "all" | "open" | "flagged";
export type PaletteMode = "commands" | "sessions";

interface UIState {
  focus: PanelId;
  grouping: Grouping;
  filter: FileFilter;
  /** Hide generated files and lockfiles (SPEC §3.4). */
  maskNoise: boolean;
  query: string;
  /** Folder paths folded in FILE TREE mode. */
  collapsed: Record<string, boolean>;
  /** Selection is by path so it survives refreshes that reorder the list. */
  selectedPath: string | null;
  /** Hunk cursor, by stable hunk id. */
  hunkId: string | null;
  /** Pinned session; null follows the newest one, so a new session shows up by itself. */
  sessionId: string | null;
  palette: PaletteMode | null;
  helpOpen: boolean;
  toast: { kind: "ok" | "err"; text: string; id: number } | null;
  errPanel: PanelId | null;
  output: string;

  setFocus: (p: PanelId) => void;
  cyclePanel: (dir: 1 | -1) => void;
  setFilter: (f: FileFilter) => void;
  toggleMask: () => void;
  toggleGrouping: () => void;
  setSession: (id: string | null) => void;
  openPalette: (mode: PaletteMode | null) => void;
  setQuery: (q: string) => void;
  toggleFolder: (dir: string) => void;
  setFolder: (dir: string, collapsed: boolean) => void;
  unfoldAll: () => void;
  select: (path: string | null) => void;
  setHunk: (id: string | null) => void;
  toggleHelp: () => void;
  setOutput: (text: string) => void;
  emitToast: (kind: "ok" | "err", text: string) => void;
  clearToast: () => void;
  pulseError: (p: PanelId) => void;
}

// Cycled via h/l/Tab. Ops console and field notes join once they have
// something to navigate.
const PANEL_ORDER: PanelId[] = ["changeset", "diff"];

export const useUI = create<UIState>((set, get) => ({
  focus: "diff",
  grouping: "intent",
  filter: "all",
  maskNoise: true,
  query: "",
  collapsed: {},
  selectedPath: null,
  hunkId: null,
  sessionId: null,
  palette: null,
  helpOpen: false,
  toast: null,
  errPanel: null,
  output: "system ready · awaiting input",

  setFocus: (p) => set({ focus: p }),
  cyclePanel: (dir) => {
    const i = PANEL_ORDER.indexOf(get().focus);
    const n = ((i === -1 ? 0 : i) + dir + PANEL_ORDER.length) % PANEL_ORDER.length;
    set({ focus: PANEL_ORDER[n] });
  },
  setFilter: (f) => set({ filter: f }),
  toggleMask: () => set((s) => ({ maskNoise: !s.maskNoise })),
  toggleGrouping: () => set((s) => ({ grouping: s.grouping === "intent" ? "tree" : "intent" })),
  setSession: (id) => set({ sessionId: id }),
  openPalette: (mode) => set({ palette: mode }),
  setQuery: (q) => set({ query: q }),
  toggleFolder: (dir) =>
    set((s) => ({ collapsed: { ...s.collapsed, [dir]: !s.collapsed[dir] } })),
  setFolder: (dir, collapsed) =>
    set((s) => ({ collapsed: { ...s.collapsed, [dir]: collapsed } })),
  unfoldAll: () => set({ collapsed: {} }),
  select: (path) =>
    set((s) => (s.selectedPath === path ? {} : { selectedPath: path, hunkId: null })),
  setHunk: (id) => set({ hunkId: id }),
  toggleHelp: () => set((s) => ({ helpOpen: !s.helpOpen })),
  setOutput: (text) => set({ output: text }),
  emitToast: (kind, text) => set({ toast: { kind, text, id: Date.now() } }),
  clearToast: () => set({ toast: null }),
  pulseError: (p) => {
    set({ errPanel: p });
    setTimeout(() => {
      if (get().errPanel === p) set({ errPanel: null });
    }, 250);
  },
}));
