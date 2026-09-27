import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "../lib/invoke";
import { buildChangeset } from "../lib/changeset";
import { useUI } from "../store/ui";

// Everything under "repo" is refetched when the watcher fires.
export const QK = {
  current: ["repo", "current"] as const,
  status: ["repo", "status"] as const,
  diffFile: (path: string | null) => ["repo", "diff", path] as const,
};

export function useRepoCurrent() {
  return useQuery({ queryKey: QK.current, queryFn: api.repoCurrent, staleTime: Infinity });
}

export function useStatus() {
  const { data: repo } = useRepoCurrent();
  return useQuery({ queryKey: QK.status, queryFn: api.repoStatus, enabled: !!repo });
}

export function useDiffFile(path: string | null) {
  return useQuery({
    queryKey: QK.diffFile(path),
    queryFn: () => api.diffFile(path!),
    enabled: !!path,
  });
}

export function useChangeset() {
  const { data } = useStatus();
  const query = useUI((s) => s.query);
  const filter = useUI((s) => s.filter);
  const collapsed = useUI((s) => s.collapsed);
  return useMemo(
    () => buildChangeset(data?.files ?? [], { query, filter, collapsed }),
    [data, query, filter, collapsed],
  );
}
