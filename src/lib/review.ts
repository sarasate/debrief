// Review-state writes (SPEC §4). The cache is patched first so the UI
// answers at key-repeat speed, then the model is refetched from disk.

import type { QueryClient } from "@tanstack/react-query";
import { api } from "./invoke";
import { QK } from "../hooks/useRepo";
import { useUI } from "../store/ui";
import type { ReviewModel, Verdict } from "./types";

export function currentModel(qc: QueryClient): ReviewModel | undefined {
  return qc.getQueryData<ReviewModel>(QK.review(useUI.getState().sessionId));
}

function patch(qc: QueryClient, f: (m: ReviewModel) => ReviewModel) {
  qc.setQueryData<ReviewModel>(QK.review(useUI.getState().sessionId), (m) => (m ? f(m) : m));
}

function refetch(qc: QueryClient) {
  return qc.invalidateQueries({ queryKey: QK.review(useUI.getState().sessionId) });
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
