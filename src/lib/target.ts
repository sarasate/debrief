// Switching what is reviewed: the working tree or a branch (docs/PLAN.md).

import type { QueryClient } from "@tanstack/react-query";
import { api } from "./invoke";
import { useUI } from "../store/ui";
import type { Target } from "./types";

export async function switchTarget(qc: QueryClient, target: Target) {
  const range = await api.targetSet(target);
  const ui = useUI.getState();
  ui.setTarget(target);
  // BY COMMIT only exists for a branch.
  if (!range && ui.grouping === "commit") ui.setGrouping("intent");
  ui.select(null);
  ui.unfoldAll();
  await qc.invalidateQueries({ queryKey: ["repo"] });
  ui.setOutput(
    range
      ? `reviewing ${range.head} ← ${range.base} · ${range.ahead} commit${range.ahead === 1 ? "" : "s"}` +
          (range.includesWorktree ? " + uncommitted" : "")
      : "reviewing the working tree",
  );
}
