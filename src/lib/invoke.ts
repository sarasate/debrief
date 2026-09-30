import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
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
  Range,
  Target,
  TargetList,
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
  targetList: () => tauriInvoke<TargetList>("target_list"),
  targetSet: (target: Target) => tauriInvoke<Range | null>("target_set", { target }),
  stageCleared: (sessionId: string) => tauriInvoke<StageResult>("stage_cleared", { sessionId }),
  /** Rewrites files on disk: only after the confirm modal. */
  discardReverted: (sessionId: string) => tauriInvoke<DiscardResult>("discard_reverted", { sessionId }),
  notesAdd: (sessionId: string, path: string, hunk: { id: string; header: string } | null, text: string) =>
    tauriInvoke<Note>("notes_add", { sessionId, path, hunkId: hunk?.id ?? null, hunkHeader: hunk?.header ?? null, text }),
  notesRemove: (sessionId: string, id: string) => tauriInvoke<void>("notes_remove", { sessionId, id }),
  /** `sessionId` is the state key; `resumeSession` the Claude session the feedback is for. */
  notesTransmit: (sessionId: string, resumeSession: string | null, force: boolean) =>
    tauriInvoke<TransmitOutcome>("notes_transmit", { sessionId, resumeSession, force }),
  transmitCancel: () => tauriInvoke<boolean>("transmit_cancel"),
  settingsGet: () => tauriInvoke<Settings>("settings_get"),
  /** `claudePath: ""` clears it back to auto-detect. */
  settingsSet: (patch: {
    transmitMode?: TransmitMode;
    claudePath?: string;
    accent?: Accent;
    scanlines?: boolean;
    consoleHeight?: number;
    terminalApp?: string;
  }) =>
    tauriInvoke<Settings>("settings_set", {
      transmitMode: patch.transmitMode ?? null,
      claudePath: patch.claudePath ?? null,
      accent: patch.accent ?? null,
      scanlines: patch.scanlines ?? null,
      consoleHeight: patch.consoleHeight ?? null,
      terminalApp: patch.terminalApp ?? null,
    }),
  // Console (M11). Only ever fed what the user types.
  consoleOpen: (cols: number, rows: number, onOutput: Channel<ArrayBuffer>) =>
    tauriInvoke<number>("console_open", { cols, rows, onOutput }),
  consoleWrite: (id: number, data: string) => tauriInvoke<void>("console_write", { id, data }),
  consoleResize: (id: number, cols: number, rows: number) => tauriInvoke<void>("console_resize", { id, cols, rows }),
  consoleClose: (id: number) => tauriInvoke<boolean>("console_close", { id }),
  consoleQuit: () => tauriInvoke<void>("console_quit"),
  openInTerminal: () => tauriInvoke<string>("open_in_terminal"),
};

/** Tauri rejects with the serialized AppError, which is a plain string. */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
