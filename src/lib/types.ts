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
