// The Changeset list as data: which files pass the filters, the rows to draw
// and the order j/k walk. Pure, so the key handler and the panel agree.

import type { FileFilter } from "../store/ui";
import type { ChangedFile } from "./types";

export type Row =
  | { kind: "folder"; key: string; name: string; pad: number; open: boolean }
  | { kind: "file"; file: ChangedFile; pad: number };

export interface ChangesetView {
  /** Files matching the path query — what the filter pill counts are based on. */
  base: ChangedFile[];
  /** `base` narrowed by the ALL / OPEN / FLAGGED filter. */
  visible: ChangedFile[];
  rows: Row[];
  /** Paths of the file rows on screen, top to bottom. */
  order: string[];
}

// Review state arrives in M4 and flags in M3; until then every file is open
// and none is flagged.
const isOpen = (_f: ChangedFile) => true;
const isFlagged = (_f: ChangedFile) => false;

export function filterCounts(base: ChangedFile[]): Record<FileFilter, number> {
  return {
    all: base.length,
    open: base.filter(isOpen).length,
    flagged: base.filter(isFlagged).length,
  };
}

export function splitPath(path: string) {
  const parts = path.split("/");
  return { name: parts[parts.length - 1], dirs: parts.slice(0, -1) };
}

export function buildChangeset(
  files: ChangedFile[],
  opts: { query: string; filter: FileFilter; collapsed: Record<string, boolean> },
): ChangesetView {
  const q = opts.query.trim().toLowerCase();
  const base = files.filter((f) => !q || f.path.toLowerCase().includes(q));
  const visible = base.filter((f) =>
    opts.filter === "open" ? isOpen(f) : opts.filter === "flagged" ? isFlagged(f) : true,
  );

  const sorted = [...visible].sort((a, b) => (a.path < b.path ? -1 : 1));
  const folded = (dirs: string[], upto: number) => {
    for (let k = 1; k <= upto; k++) if (opts.collapsed[dirs.slice(0, k).join("/")]) return true;
    return false;
  };

  const rows: Row[] = [];
  const order: string[] = [];
  let prev: string[] = [];
  for (const f of sorted) {
    const { dirs } = splitPath(f.path);
    let shared = 0;
    while (shared < prev.length && shared < dirs.length && prev[shared] === dirs[shared]) shared++;
    for (let d = shared; d < dirs.length; d++) {
      if (folded(dirs, d)) break;
      const key = dirs.slice(0, d + 1).join("/");
      rows.push({ kind: "folder", key, name: dirs[d], pad: 8 + d * 14, open: !opts.collapsed[key] });
    }
    prev = dirs;
    if (!folded(dirs, dirs.length)) {
      rows.push({ kind: "file", file: f, pad: 8 + dirs.length * 14 + 14 });
      order.push(f.path);
    }
  }
  return { base, visible, rows, order };
}
