import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { FileDiff, Ledger, RepoInfo, RepoStatus, ReviewModel, SessionInfo } from "./types";

export const api = {
  repoOpen: (path: string) => tauriInvoke<RepoInfo>("repo_open", { path }),
  repoCurrent: () => tauriInvoke<RepoInfo | null>("repo_current"),
  repoStatus: () => tauriInvoke<RepoStatus>("repo_status"),
  diffFile: (path: string) => tauriInvoke<FileDiff>("diff_file", { path }),
  sessionsList: () => tauriInvoke<SessionInfo[]>("sessions_list"),
  ledgerLoad: (sessionId: string) => tauriInvoke<Ledger>("ledger_load", { sessionId }),
  /** `sessionId` null picks the newest session for the repo. */
  reviewModel: (sessionId: string | null) => tauriInvoke<ReviewModel>("review_model", { sessionId }),
};

/** Tauri rejects with the serialized AppError, which is a plain string. */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
