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

export type GroupKind = "turn" | "commit" | "uncommitted" | "unattributed" | "generated";

export interface TurnKey {
  sessionId: string;
  index: number;
}

export interface GroupFile {
  path: string;
  tools: EditTool[];
  alsoTurns: TurnKey[];
  /** BY COMMIT: earlier commits (short sha) that also changed it. */
  alsoCommits: string[];
}

export interface CommitRef {
  sha: string;
  short: string;
  author: string;
  /** ms since epoch */
  time: number;
  /** Has a Co-Authored-By: Claude trailer. */
  claude: boolean;
}

export interface IntentGroup {
  id: string;
  kind: GroupKind;
  title: string;
  briefing: string;
  turn: number | null;
  prompt: string | null;
  files: GroupFile[];
  sessionId: string | null;
  commit: CommitRef | null;
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
  viewed: boolean;
  /** Worktree blob oid, sent back when marking viewed (SPEC §4). */
  oid: string;
  hunkIds: string[];
  /** Branch review: hunks still uncommitted (the only ones stage/discard reach). Null in a worktree review. */
  uncommitted: string[] | null;
}

export type Verdict = "keep" | "revert";

// Review target (docs/PLAN.md "Branch & PR review")

export type Target = { kind: "worktree" } | { kind: "branch"; head: string; base: string | null };

/** A branch target resolved to commits. */
export interface Range {
  head: string;
  base: string;
  headSha: string;
  mergeBaseSha: string;
  ahead: number;
  behind: number;
  /** The branch is checked out, so the review runs on to the working tree. */
  includesWorktree: boolean;
  /** ms since epoch */
  headTime: number;
}

export interface BranchOption {
  name: string;
  ahead: number;
  behind: number;
  checkedOut: boolean;
  updatedAt: number;
  subject: string;
}

export interface TargetList {
  base: string | null;
  branches: BranchOption[];
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
  turns: { sessionId: string; index: number; title: string }[];
  groups: IntentGroup[];
  files: Record<string, FileMeta>;
  noise: NoiseConfig;
  verdicts: Record<string, Verdict>;
  /** Files whose viewed mark this refresh dropped because they changed. */
  invalidated: string[];
  /** The session id the progress is stored under, or "worktree". */
  stateKey: string;
  /** Queued field notes, oldest first. */
  notes: Note[];
  /** Discarded hunks the next transmit will mention. */
  unreportedDiscards: number;
  /** The branch under review; null for the working tree. */
  range: Range | null;
  /** BY COMMIT groups; empty in a worktree review. */
  commitGroups: IntentGroup[];
  /** Committed hunks marked revert the next transmit will ask Claude to revert. */
  pendingRequests: number;
}

// Stage / discard (SPEC §5)

export interface ActionFailure {
  /** Hunk id for discard, path for stage. */
  id: string;
  path: string;
  reason: string;
}

export interface StageResult {
  staged: string[];
  skipped: ActionFailure[];
}

export interface DiscardResult {
  discarded: string[];
  failed: ActionFailure[];
  warnings: string[];
}

// Field notes and transmit (SPEC §6)

export interface Note {
  id: string;
  path: string;
  hunkId?: string;
  hunkHeader?: string;
  text: string;
  createdAt: string;
}

export type TransmitMode = "clipboard" | "file" | "resume";

export type Accent = "cyan" | "green" | "amber" | "red";

export interface Settings {
  lastRepo: string | null;
  transmitMode: TransmitMode;
  claudePath: string | null;
  accent: Accent;
  scanlines: boolean;
}

export type TransmitOutcome =
  | { status: "copied"; notes: number }
  | { status: "written"; notes: number; path: string }
  | { status: "started"; notes: number }
  | { status: "confirm"; message: string };
