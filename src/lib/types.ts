export interface RepoInfo {
  workdir: string;
  name: string;
}

export interface HeadInfo {
  branch: string | null;
  sha: string;
  detached: boolean;
}

export type FileStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "typechange"
  | "untracked"
  | "conflicted";

export interface ChangedFile {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  isBinary: boolean;
  adds: number;
  dels: number;
  hunks: number;
}

export interface RepoStatus {
  repo: RepoInfo;
  head: HeadInfo | null;
  files: ChangedFile[];
  adds: number;
  dels: number;
  /** ms since epoch */
  lastWrite: number | null;
}

export type DiffLineKind = "context" | "addition" | "deletion";

export interface DiffLine {
  kind: DiffLineKind;
  content: string;
  oldLineno: number | null;
  newLineno: number | null;
}

export interface DiffHunk {
  id: string;
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  isBinary: boolean;
  hunks: DiffHunk[];
}

// Transcripts (SPEC §3.2) and intent groups (§3.3)

export type EditTool = "Edit" | "MultiEdit" | "Write" | "NotebookEdit";

export interface SessionInfo {
  id: string;
  title: string;
  firstPrompt: string | null;
  cwd: string;
  startedAt: string | null;
  /** ms since epoch, newest write to the transcript or its subagents */
  updatedAt: number;
  lastEditAt: string | null;
  turns: number;
  edits: number;
  path: string;
}

export interface LedgerEntry {
  path: string;
  tool: EditTool;
  timestamp: string;
  turn: number;
}

export interface Turn {
  index: number;
  prompt: string;
  summary: string;
  files: string[];
  timestamp: string;
  commands: string[];
}

export interface Ledger {
  turns: Turn[];
  entries: LedgerEntry[];
}

export type GroupKind = "turn" | "unattributed" | "generated";

export interface GroupFile {
  path: string;
  tools: EditTool[];
  alsoTurns: number[];
}

export interface IntentGroup {
  id: string;
  kind: GroupKind;
  title: string;
  briefing: string;
  turn: number | null;
  prompt: string | null;
  files: GroupFile[];
}

// Noise (SPEC §3.4) and flags (§3.5)

export type FlagKind = "unattributed" | "marker" | "testRemoval" | "secret" | "conflict" | "large";

export interface Flag {
  kind: FlagKind;
  /** Never contains a secret's value. */
  reason: string;
  line: number | null;
}

export interface FileMeta {
  noise: boolean;
  flags: Flag[];
}

export interface NoiseConfig {
  globs: string[];
  source: "defaults" | "file";
  /** Why .debrief.toml was ignored. */
  error: string | null;
}

export interface ReviewModel {
  status: RepoStatus;
  session: SessionInfo | null;
  turns: { index: number; title: string }[];
  groups: IntentGroup[];
  files: Record<string, FileMeta>;
  noise: NoiseConfig;
}
