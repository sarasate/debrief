import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  DiscardResult,
  Accent,
  FileDiff,
  Note,
  Settings,
  TransmitMode,
  TransmitOutcome,
  Ledger,
  RepoInfo,
  RepoStatus,
  ReviewModel,
  SessionInfo,
  StageResult,
  Verdict,
} from "./types";

export const api = {
  repoOpen: (path: string) => tauriInvoke<RepoInfo>("repo_open", { path }),
  repoCurrent: () => tauriInvoke<RepoInfo | null>("repo_current"),
  repoStatus: () => tauriInvoke<RepoStatus>("repo_status"),
  diffFile: (path: string) => tauriInvoke<FileDiff>("diff_file", { path }),
  sessionsList: () => tauriInvoke<SessionInfo[]>("sessions_list"),
  ledgerLoad: (sessionId: string) => tauriInvoke<Ledger>("ledger_load", { sessionId }),
  /** `sessionId` null picks the newest session for the repo. */
  reviewModel: (sessionId: string | null) => tauriInvoke<ReviewModel>("review_model", { sessionId }),
  /** `sessionId` is the model's `stateKey`. */
  reviewSetViewed: (sessionId: string, path: string, oid: string | null, viewed: boolean) =>
    tauriInvoke<void>("review_set_viewed", { sessionId, path, oid, viewed }),
  reviewSetVerdict: (sessionId: string, hunkId: string, verdict: Verdict | null) =>
    tauriInvoke<void>("review_set_verdict", { sessionId, hunkId, verdict }),
  stageCleared: (sessionId: string) => tauriInvoke<StageResult>("stage_cleared", { sessionId }),
  /** Rewrites files on disk: only after the confirm modal. */
  discardReverted: (sessionId: string) => tauriInvoke<DiscardResult>("discard_reverted", { sessionId }),
  notesAdd: (sessionId: string, path: string, hunk: { id: string; header: string } | null, text: string) =>
    tauriInvoke<Note>("notes_add", { sessionId, path, hunkId: hunk?.id ?? null, hunkHeader: hunk?.header ?? null, text }),
  notesRemove: (sessionId: string, id: string) => tauriInvoke<void>("notes_remove", { sessionId, id }),
  notesTransmit: (sessionId: string, force: boolean) => tauriInvoke<TransmitOutcome>("notes_transmit", { sessionId, force }),
  transmitCancel: () => tauriInvoke<boolean>("transmit_cancel"),
  settingsGet: () => tauriInvoke<Settings>("settings_get"),
  /** `claudePath: ""` clears it back to auto-detect. */
  settingsSet: (patch: { transmitMode?: TransmitMode; claudePath?: string; accent?: Accent; scanlines?: boolean }) =>
    tauriInvoke<Settings>("settings_set", {
      transmitMode: patch.transmitMode ?? null,
      claudePath: patch.claudePath ?? null,
      accent: patch.accent ?? null,
      scanlines: patch.scanlines ?? null,
    }),
};

/** Tauri rejects with the serialized AppError, which is a plain string. */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
