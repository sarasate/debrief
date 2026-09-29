// The Changeset list as data: which files pass the filters, the rows to draw
// and the order j/k walk. Pure, so the key handler and the panel agree.

import type { FileFilter, Grouping } from "../store/ui";
import type { ChangedFile, GroupKind, IntentGroup, ReviewModel } from "./types";

export type Row =
  | { kind: "group"; id: string; group: GroupKind; title: string; count: number; adds: number; dels: number }
  | { kind: "folder"; key: string; name: string; pad: number; open: boolean }
  | { kind: "file"; file: ChangedFile; pad: number; showDir: boolean };

export interface ChangesetView {
  /** Files matching the path query and the noise mask — what the filter pill counts are based on. */
  base: ChangedFile[];
  /** `base` narrowed by the ALL / OPEN / FLAGGED filter. */
  visible: ChangedFile[];
  rows: Row[];
  /** Paths of the file rows on screen, top to bottom. */
  order: string[];
  /** Noise files in the changeset, masked or not. */
  noiseCount: number;
}

export interface ChangesetOpts {
  query: string;
  filter: FileFilter;
  collapsed: Record<string, boolean>;
  grouping: Grouping;
  maskNoise: boolean;
}

/** Not yet cleared (SPEC §4). */
export function isOpen(model: ReviewModel | undefined, path: string): boolean {
  return !model?.files[path]?.viewed;
}

export function isNoise(model: ReviewModel | undefined, path: string): boolean {
  return !!model?.files[path]?.noise;
}

export function isFlagged(model: ReviewModel | undefined, path: string): boolean {
  return (model?.files[path]?.flags.length ?? 0) > 0;
}

export function filterCounts(model: ReviewModel | undefined, base: ChangedFile[]): Record<FileFilter, number> {
  return {
    all: base.length,
    open: base.filter((f) => isOpen(model, f.path)).length,
    flagged: base.filter((f) => isFlagged(model, f.path)).length,
  };
}

export function splitPath(path: string) {
  const parts = path.split("/");
  return { name: parts[parts.length - 1], dirs: parts.slice(0, -1) };
}

/** The intent group a path belongs to. */
export function groupOf(model: ReviewModel | undefined, path: string | null): IntentGroup | undefined {
  if (!model || !path) return undefined;
  return model.groups.find((g) => g.files.some((f) => f.path === path));
}

export function buildChangeset(model: ReviewModel | undefined, opts: ChangesetOpts): ChangesetView {
  const files = model?.status.files ?? [];
  const q = opts.query.trim().toLowerCase();
  const noiseCount = files.filter((f) => isNoise(model, f.path)).length;
  const base = files.filter(
    (f) => (!opts.maskNoise || !isNoise(model, f.path)) && (!q || f.path.toLowerCase().includes(q)),
  );
  const visible = base.filter((f) =>
    opts.filter === "open" ? isOpen(model, f.path) : opts.filter === "flagged" ? isFlagged(model, f.path) : true,
  );
  const rows =
    opts.grouping === "intent" && model ? intentRows(model, visible) : treeRows(visible, opts.collapsed);
  return { base, visible, noiseCount, ...rows };
}

function intentRows(model: ReviewModel, visible: ChangedFile[]) {
  const byPath = new Map(visible.map((f) => [f.path, f]));
  const rows: Row[] = [];
  const order: string[] = [];
  const placed = new Set<string>();
  for (const g of model.groups) {
    const fs = g.files.flatMap((gf) => byPath.get(gf.path) ?? []);
    if (!fs.length) continue;
    rows.push({
      kind: "group",
      id: g.id,
      group: g.kind,
      title: g.title,
      count: fs.length,
      adds: fs.reduce((n, f) => n + f.adds, 0),
      dels: fs.reduce((n, f) => n + f.dels, 0),
    });
    for (const f of fs) {
      rows.push({ kind: "file", file: f, pad: 10, showDir: true });
      order.push(f.path);
      placed.add(f.path);
    }
  }
  // The model's status and groups come from one call, so nothing should be
  // left over; list stragglers at the end rather than hide them.
  for (const f of visible) {
    if (placed.has(f.path)) continue;
    rows.push({ kind: "file", file: f, pad: 10, showDir: true });
    order.push(f.path);
  }
  return { rows, order };
}

function treeRows(visible: ChangedFile[], collapsed: Record<string, boolean>) {
  const sorted = [...visible].sort((a, b) => (a.path < b.path ? -1 : 1));
  const folded = (dirs: string[], upto: number) => {
    for (let k = 1; k <= upto; k++) if (collapsed[dirs.slice(0, k).join("/")]) return true;
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
      rows.push({ kind: "folder", key, name: dirs[d], pad: 8 + d * 14, open: !collapsed[key] });
    }
    prev = dirs;
    if (!folded(dirs, dirs.length)) {
      rows.push({ kind: "file", file: f, pad: 8 + dirs.length * 14 + 14, showDir: false });
      order.push(f.path);
    }
  }
  return { rows, order };
}
