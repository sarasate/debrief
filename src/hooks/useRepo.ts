import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "../lib/invoke";
import { buildChangeset } from "../lib/changeset";
import { useUI } from "../store/ui";
import type { ReviewModel } from "../lib/types";

// Everything under "repo" is refetched when the watcher fires.
export const QK = {
  current: ["repo", "current"] as const,
  review: (sessionId: string | null) => ["repo", "review", sessionId] as const,
  sessions: ["repo", "sessions"] as const,
  diffFile: (path: string | null) => ["repo", "diff", path] as const,
};

export function useRepoCurrent() {
  return useQuery({ queryKey: QK.current, queryFn: api.repoCurrent, staleTime: Infinity });
}

/** Status, session and intent groups in one snapshot, so they always agree. */
export function useReview<T = ReviewModel>(select?: (m: ReviewModel) => T) {
  const { data: repo } = useRepoCurrent();
  const sessionId = useUI((s) => s.sessionId);
  return useQuery({
    queryKey: QK.review(sessionId),
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

export function useDiffFile(path: string | null) {
  return useQuery({
    queryKey: QK.diffFile(path),
    queryFn: () => api.diffFile(path!),
    enabled: !!path,
  });
}

export function useChangeset() {
  const { data } = useReview();
  const query = useUI((s) => s.query);
  const filter = useUI((s) => s.filter);
  const collapsed = useUI((s) => s.collapsed);
  const grouping = useUI((s) => s.grouping);
  return useMemo(
    () => buildChangeset(data, { query, filter, collapsed, grouping }),
    [data, query, filter, collapsed, grouping],
  );
}
