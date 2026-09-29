// Review-state writes (SPEC §4). The cache is patched first so the UI
// answers at key-repeat speed, then the model is refetched from disk.

import type { QueryClient } from "@tanstack/react-query";
import { api } from "./invoke";
import { reviewKey } from "../hooks/useRepo";
import { useUI } from "../store/ui";
import type { ReviewModel, Verdict } from "./types";

export function currentModel(qc: QueryClient): ReviewModel | undefined {
  return qc.getQueryData<ReviewModel>(reviewKey());
}

function patch(qc: QueryClient, f: (m: ReviewModel) => ReviewModel) {
  qc.setQueryData<ReviewModel>(reviewKey(), (m) => (m ? f(m) : m));
}

function refetch(qc: QueryClient) {
  return qc.invalidateQueries({ queryKey: reviewKey() });
}

export async function setViewed(qc: QueryClient, path: string, viewed: boolean) {
  const model = currentModel(qc);
  const meta = model?.files[path];
  if (!model || !meta) return;
  patch(qc, (m) => ({ ...m, files: { ...m.files, [path]: { ...m.files[path], viewed } } }));
  try {
    await api.reviewSetViewed(model.stateKey, path, meta.oid, viewed);
  } finally {
    await refetch(qc);
  }
}

export async function setVerdict(qc: QueryClient, hunkId: string, verdict: Verdict | null) {
  const model = currentModel(qc);
  if (!model) return;
  patch(qc, (m) => {
    const verdicts = { ...m.verdicts };
    if (verdict) verdicts[hunkId] = verdict;
    else delete verdicts[hunkId];
    return { ...m, verdicts };
  });
  try {
    await api.reviewSetVerdict(model.stateKey, hunkId, verdict);
  } finally {
    await refetch(qc);
  }
}

/** Progress over relevant (non-noise) files, as the Ops console shows it. */
export function progress(model: ReviewModel | undefined) {
  const relevant = Object.entries(model?.files ?? {}).filter(([, m]) => !m.noise);
  const viewed = relevant.filter(([, m]) => m.viewed).length;
  let kept = 0, reverted = 0, total = 0;
  for (const [, m] of relevant) {
    for (const id of m.hunkIds) {
      total++;
      if (model?.verdicts[id] === "keep") kept++;
      if (model?.verdicts[id] === "revert") reverted++;
    }
  }
  const pct = relevant.length ? Math.round((viewed / relevant.length) * 100) : 0;
  return { viewed, total: relevant.length, pct, kept, reverted, pending: total - kept - reverted };
}

/** `a`: stage cleared files. Reports into the command bar. */
export async function stageCleared(qc: QueryClient) {
  const model = currentModel(qc);
  const ui = useUI.getState();
  if (!model) return;
  if (!Object.values(model.files).some((m) => m.viewed)) {
    ui.setOutput("nothing cleared to stage · spc to clear a file");
    return;
  }
  const r = await api.stageCleared(model.stateKey);
  await qc.invalidateQueries({ queryKey: ["repo"] });
  const n = r.staged.length;
  const skipped = r.skipped.map((s) => `${s.path.split("/").pop()} (${s.reason})`);
  ui.setOutput(
    `staged ${n} cleared file${n === 1 ? "" : "s"}` + (skipped.length ? ` · skipped ${skipped.join(", ")}` : ""),
  );
  if (n) ui.emitToast("ok", `▣ staged ${n} file${n === 1 ? "" : "s"}`);
}

/** Hunks marked revert that still exist, for the confirm modal. */
/** Hunks marked revert that `d` can undo. In a branch review only the
 * uncommitted ones; committed ones stay as requests for Claude. */
export function revertTargets(model: ReviewModel | undefined): { path: string; ids: string[] }[] {
  if (!model) return [];
  return Object.entries(model.files)
    .map(([path, m]) => ({
      path,
      ids: m.hunkIds.filter((id) => model.verdicts[id] === "revert" && (m.uncommitted === null || m.uncommitted.includes(id))),
    }))
    .filter((t) => t.ids.length > 0);
}

/** Hunks marked revert that are already committed (branch review). */
export function committedReverts(model: ReviewModel | undefined): number {
  if (!model?.range) return 0;
  return Object.values(model.files).reduce(
    (n, m) => n + m.hunkIds.filter((id) => model.verdicts[id] === "revert" && !(m.uncommitted ?? []).includes(id)).length,
    0,
  );
}

/** Why `a` / `d` can't run in the current review, or null when they can. */
export function readOnlyReason(model: ReviewModel | undefined): string | null {
  const r = model?.range;
  if (!r || r.includesWorktree) return null;
  return `${r.head} isn't checked out · committed changes are read-only (y/x still record verdicts)`;
}

/** Cleared files `a` would stage: in a branch review, only those with uncommitted changes. */
export function stageableCount(model: ReviewModel | undefined): number {
  return Object.values(model?.files ?? {}).filter(
    (m) => m.viewed && (m.uncommitted === null || m.uncommitted.length > 0),
  ).length;
}

/** After the confirm modal: reverse-apply the reverted hunks. */
export async function discardReverted(qc: QueryClient) {
  const model = currentModel(qc);
  const ui = useUI.getState();
  if (!model) return;
  const r = await api.discardReverted(model.stateKey);
  await qc.invalidateQueries({ queryKey: ["repo"] });
  const n = r.discarded.length;
  ui.setOutput(`discarded ${n} reverted hunk${n === 1 ? "" : "s"}` + (r.failed.length ? ` · ${r.failed.length} failed` : ""));
  if (r.failed.length) {
    const f = r.failed[0];
    ui.emitToast("err", `${f.path ? f.path.split("/").pop() + ": " : ""}${f.reason}` + (r.failed.length > 1 ? ` (+${r.failed.length - 1} more)` : ""));
  } else if (r.warnings.length) {
    ui.emitToast("err", r.warnings[0]);
  } else if (n) {
    ui.emitToast("ok", `discarded ${n} hunk${n === 1 ? "" : "s"}`);
  }
}
