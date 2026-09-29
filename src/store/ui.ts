import { create } from "zustand";
import type { PanelId } from "../lib/keymap";
import type { Target } from "../lib/types";

export type Grouping = "intent" | "commit" | "tree";
export type FileFilter = "all" | "open" | "flagged";
export type PaletteMode = "commands" | "sessions" | "targets";
export type ModalId = "discard";

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
  /** The reviewer moved the cursor to this hunk (vs. the automatic first
   * hunk), so `n` scopes a note to it. */
  hunkPinned: boolean;
  /** Hunks over the large-diff limit that the reviewer opened. */
  expanded: Record<string, boolean>;
  noteDraft: string;
  /** "file" after N: the next note ignores the hunk cursor. */
  noteScope: "auto" | "file";
  /** Selected queued note in the Field notes panel. */
  notesCursor: number;
  /** Resume-mode run shown in the output drawer. */
  transmit: { running: boolean; lines: { stream: string; line: string }[]; code: number | null; sent: boolean } | null;
  /** A second `f` before this time sends despite the active-session warning. */
  forceUntil: number;
  /** Pinned session; null follows the newest one, so a new session shows up by itself. */
  sessionId: string | null;
  /** Mirrors the backend's target; part of the review query key. */
  target: Target;
  palette: PaletteMode | null;
  modal: ModalId | null;
  helpOpen: boolean;
  booting: boolean;
  toast: { kind: "ok" | "err"; text: string; id: number } | null;
  errPanel: PanelId | null;
  output: string;

  setFocus: (p: PanelId) => void;
  cyclePanel: (dir: 1 | -1) => void;
  setFilter: (f: FileFilter) => void;
  toggleMask: () => void;
  /** intent → commit → tree; commit only when a branch is under review. */
  toggleGrouping: (hasBranch: boolean) => void;
  setGrouping: (g: Grouping) => void;
  setSession: (id: string | null) => void;
  setTarget: (t: Target) => void;
  openPalette: (mode: PaletteMode | null) => void;
  openModal: (m: ModalId | null) => void;
  setQuery: (q: string) => void;
  toggleFolder: (dir: string) => void;
  setFolder: (dir: string, collapsed: boolean) => void;
  unfoldAll: () => void;
  select: (path: string | null) => void;
  setHunk: (id: string | null, pinned?: boolean) => void;
  toggleExpanded: (id: string, open?: boolean) => void;
  setNoteDraft: (t: string) => void;
  setNoteScope: (s: "auto" | "file") => void;
  setNotesCursor: (i: number) => void;
  setTransmit: (t: UIState["transmit"]) => void;
  setForceUntil: (t: number) => void;
  toggleHelp: () => void;
  finishBoot: () => void;
  setOutput: (text: string) => void;
  emitToast: (kind: "ok" | "err", text: string) => void;
  clearToast: () => void;
  pulseError: (p: PanelId) => void;
}

// Cycled via h/l/Tab. The Ops console has nothing to navigate; its actions
// are global keys.
const PANEL_ORDER: PanelId[] = ["changeset", "diff", "notes"];

export const useUI = create<UIState>((set, get) => ({
  focus: "diff",
  grouping: "intent",
  filter: "all",
  maskNoise: true,
  query: "",
  collapsed: {},
  selectedPath: null,
  hunkId: null,
  hunkPinned: false,
  expanded: {},
  noteDraft: "",
  noteScope: "auto",
  notesCursor: 0,
  transmit: null,
  forceUntil: 0,
  sessionId: null,
  target: { kind: "worktree" },
  palette: null,
  modal: null,
  helpOpen: false,
  booting: true,
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
  toggleGrouping: (hasBranch) =>
    set((s) => {
      const order: Grouping[] = hasBranch ? ["intent", "commit", "tree"] : ["intent", "tree"];
      const i = order.indexOf(s.grouping);
      return { grouping: order[(i + 1) % order.length] };
    }),
  setGrouping: (g) => set({ grouping: g }),
  setSession: (id) => set({ sessionId: id }),
  setTarget: (t) => set({ target: t }),
  openPalette: (mode) => set({ palette: mode }),
  openModal: (m) => set({ modal: m }),
  setQuery: (q) => set({ query: q }),
  toggleFolder: (dir) =>
    set((s) => ({ collapsed: { ...s.collapsed, [dir]: !s.collapsed[dir] } })),
  setFolder: (dir, collapsed) =>
    set((s) => ({ collapsed: { ...s.collapsed, [dir]: collapsed } })),
  unfoldAll: () => set({ collapsed: {} }),
  select: (path) =>
    set((s) => (s.selectedPath === path ? {} : { selectedPath: path, hunkId: null, hunkPinned: false })),
  setHunk: (id, pinned = true) => set({ hunkId: id, hunkPinned: pinned && id != null }),
  toggleExpanded: (id, open) => set((s) => ({ expanded: { ...s.expanded, [id]: open ?? !s.expanded[id] } })),
  setNoteDraft: (t) => set({ noteDraft: t }),
  setNoteScope: (s) => set({ noteScope: s }),
  setNotesCursor: (i) => set({ notesCursor: i }),
  setTransmit: (t) => set({ transmit: t }),
  setForceUntil: (t) => set({ forceUntil: t }),
  toggleHelp: () => set((s) => ({ helpOpen: !s.helpOpen })),
  finishBoot: () => set({ booting: false }),
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
