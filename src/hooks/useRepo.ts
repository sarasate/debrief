import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "../lib/invoke";
import { buildChangeset } from "../lib/changeset";
import { useUI } from "../store/ui";
import type { ReviewModel, Target } from "../lib/types";

/** A stable string for a target, for query keys. */
export function targetKey(t: Target): string {
  return t.kind === "worktree" ? "worktree" : `branch:${t.head}:${t.base ?? ""}`;
}

// Everything under "repo" is refetched when the watcher fires.
export const QK = {
  current: ["repo", "current"] as const,
  review: (sessionId: string | null, target: string) => ["repo", "review", sessionId, target] as const,
  targets: ["repo", "targets"] as const,
  sessions: ["repo", "sessions"] as const,
  settings: ["settings"] as const,
  diffFile: (path: string | null, target: string) => ["repo", "diff", target, path] as const,
  fileLines: (path: string | null, target: string, sha: string | null) => ["repo", "file", target, path, sha] as const,
  log: (target: string) => ["repo", "log", target] as const,
  commit: (sha: string | null) => ["repo", "commit", sha] as const,
};

/** The review query key for the UI's current session pin and target. */
export function reviewKey() {
  const s = useUI.getState();
  return QK.review(s.sessionId, targetKey(s.target));
}

export function diffKey(path: string | null) {
  return QK.diffFile(path, targetKey(useUI.getState().target));
}

export function useRepoCurrent() {
  return useQuery({ queryKey: QK.current, queryFn: api.repoCurrent, staleTime: Infinity });
}

/** Status, session and intent groups in one snapshot, so they always agree. */
export function useReview<T = ReviewModel>(select?: (m: ReviewModel) => T) {
  const { data: repo } = useRepoCurrent();
  const sessionId = useUI((s) => s.sessionId);
  const target = useUI((s) => targetKey(s.target));
  return useQuery({
    queryKey: QK.review(sessionId, target),
    queryFn: () => api.reviewModel(sessionId),
    enabled: !!repo,
    select,
    // Keep showing the last snapshot while a pinned session switches.
    placeholderData: (prev) => prev,
  });
}

const selectStatus = (m: ReviewModel) => m.status;

export function useStatus() {
  return useReview(selectStatus);
}

export function useSessions(enabled: boolean) {
  return useQuery({ queryKey: QK.sessions, queryFn: api.sessionsList, enabled });
}

export function useSettings() {
  return useQuery({ queryKey: QK.settings, queryFn: api.settingsGet, staleTime: Infinity });
}

export function useTargets(enabled: boolean) {
  return useQuery({ queryKey: QK.targets, queryFn: api.targetList, enabled });
}

export function useDiffFile(path: string | null) {
  const target = useUI((s) => targetKey(s.target));
  return useQuery({
    queryKey: QK.diffFile(path, target),
    queryFn: () => api.diffFile(path!),
    enabled: !!path,
  });
}

/** Only fetched while the full-file view is on. `sha`: the file in that commit. */
export function useFileLines(path: string | null, enabled: boolean, sha: string | null = null) {
  const target = useUI((s) => targetKey(s.target));
  return useQuery({
    queryKey: QK.fileLines(path, target, sha),
    queryFn: () => api.fileLines(path!, sha),
    enabled: !!path && enabled,
  });
}

export const LOG_PAGE = 200;

export function logKey() {
  return QK.log(targetKey(useUI.getState().target));
}

/** The active branch's log, a page at a time (M13). */
export function useBranchLog(enabled: boolean) {
  const target = useUI((s) => targetKey(s.target));
  return useInfiniteQuery({
    queryKey: QK.log(target),
    queryFn: ({ pageParam }) => api.branchLog(pageParam, LOG_PAGE),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => (last.more ? pages.length * LOG_PAGE : undefined),
    enabled,
  });
}

export function useCommit(sha: string | null) {
  return useQuery({ queryKey: QK.commit(sha), queryFn: () => api.commitDiff(sha!), enabled: !!sha });
}

export function useChangeset() {
  const { data } = useReview();
  const query = useUI((s) => s.query);
  const filter = useUI((s) => s.filter);
  const collapsed = useUI((s) => s.collapsed);
  const grouping = useUI((s) => s.grouping);
  const maskNoise = useUI((s) => s.maskNoise);
  return useMemo(
    () => buildChangeset(data, { query, filter, collapsed, grouping, maskNoise }),
    [data, query, filter, collapsed, grouping, maskNoise],
  );
}
