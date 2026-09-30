import type { InfiniteData } from "@tanstack/react-query";
import type { BranchLog, LogEntry } from "./types";

/** Loaded log entries matching the log filter (subject, author or sha). */
export function visibleLog(data: InfiniteData<BranchLog> | undefined, query: string): LogEntry[] {
  const all = data?.pages.flatMap((p) => p.entries) ?? [];
  const q = query.trim().toLowerCase();
  if (!q) return all;
  return all.filter(
    (e) => e.sha.startsWith(q) || e.subject.toLowerCase().includes(q) || e.author.toLowerCase().includes(q),
  );
}

/** Cursor order: the CURRENT REVIEW row (null), then the commits. */
export function logOrder(entries: LogEntry[]): (string | null)[] {
  return [null, ...entries.map((e) => e.sha)];
}
