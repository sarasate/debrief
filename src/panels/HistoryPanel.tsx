import { Fragment, useEffect, useMemo } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { HudFrame } from "../components/HudFrame";
import { useBranchLog, useReview, useStatus } from "../hooks/useRepo";
import { runAction } from "../hooks/useKeybindings";
import { useNow, agoLabel } from "../hooks/useNow";
import { visibleLog } from "../lib/history";
import { errText } from "../lib/invoke";
import type { LogEntry, RefLabel } from "../lib/types";
import { useUI } from "../store/ui";

/** Fetch the next page once the cursor or the scroll gets this close to the end. */
const PREFETCH_ROWS = 20;

/** The active branch's first-parent log, in place of the changeset (M13). */
export function HistoryPanel() {
  const { data, error, isLoading, hasNextPage, isFetchingNextPage, fetchNextPage } = useBranchLog(true);
  const query = useUI((s) => s.logQuery);
  const cursor = useUI((s) => s.logCursor);
  const shown = useUI((s) => s.logSha);
  const entries = useMemo(() => visibleLog(data, query), [data, query]);
  const log = data?.pages[0];
  const now = useNow(30_000);

  const nearEnd = (i: number) => i >= entries.length - PREFETCH_ROWS;
  const more = () => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  };
  useEffect(() => {
    const i = cursor ? entries.findIndex((e) => e.sha === cursor) : -1;
    if (i !== -1 && nearEnd(i)) more();
  }, [cursor, entries.length]);

  // The divider goes above the first commit below the merge-base.
  const firstBase = log?.mergeBase ? entries.findIndex((e) => !e.onBranch) : -1;

  return (
    <HudFrame
      id="history"
      title="History"
      className="w-[360px] flex-none"
      badge={
        log && (
          <span className="px-[6px] py-px border border-hud/[.45] text-hud whitespace-nowrap">
            {log.head}
            {log.base && <span className="text-ink-faint"> ← {log.base}</span>}
          </span>
        )
      }
    >
      <div className="flex-none flex flex-col gap-[9px] px-3 py-[10px] border-b border-hud/10">
        <label className="flex items-center gap-2 h-8 px-[10px] bg-bg-deep border border-hud/[.18]">
          <span className="text-hud">/</span>
          <span className="sr-only">Filter commits</span>
          <input
            data-log-filter
            type="text"
            // Shas and subjects, not prose (see CommandPalette).
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            {...{ writingsuggestions: "false" }}
            placeholder="filter subject, author, sha…"
            value={query}
            onChange={(e) => useUI.getState().setLogQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.currentTarget.blur();
                useUI.getState().setFocus("history");
              }
            }}
            className="flex-1 min-w-0 border-0 outline-none bg-transparent text-ink-light text-[12px] placeholder:text-ink-dimmer"
          />
        </label>
        {query && hasNextPage && (
          <span className="text-[10px] tracking-[0.1em] text-ink-faint">filters the loaded commits · scroll down for older ones</span>
        )}
      </div>

      <nav
        aria-label="Branch history"
        data-panel-scroll="history"
        onScroll={(e) => {
          const el = e.currentTarget;
          if (el.scrollTop + el.clientHeight > el.scrollHeight - PREFETCH_ROWS * 40) more();
        }}
        className="flex-1 min-h-0 overflow-y-auto px-[6px] pt-1 pb-[10px]"
      >
        <ReviewRow active={cursor === null} shown={shown === null} />
        {error && <Empty text={errText(error)} danger />}
        {isLoading && <Empty text="READING HISTORY…" />}
        {log && entries.length === 0 && <Empty text={query ? "NOTHING MATCHES" : "NO COMMITS YET"} />}
        {entries.map((e, i) => (
          <Fragment key={e.sha}>
            {i === firstBase && <Divider base={log!.base!} />}
            <CommitRow entry={e} now={now} active={cursor === e.sha} shown={shown === e.sha} />
          </Fragment>
        ))}
        {isFetchingNextPage && <Empty text="READING OLDER COMMITS…" />}
      </nav>
    </HudFrame>
  );
}

function Empty({ text, danger = false }: { text: string; danger?: boolean }) {
  return (
    <div className={["px-3 py-7 text-center text-[11px] tracking-[0.14em] break-words", danger ? "text-sig-danger" : "text-ink-faint"].join(" ")}>
      {text}
    </div>
  );
}

function Divider({ base }: { base: string }) {
  return (
    <div className="flex items-center gap-2 px-2 pt-3 pb-[5px] text-[10px] tracking-[0.18em] text-ink-faint">
      <span className="flex-1 border-t border-hud/20" />
      LEFT {base.toUpperCase()} HERE
      <span className="flex-1 border-t border-hud/20" />
    </div>
  );
}

function Brackets() {
  return (
    <>
      <span className="dc-corner-inner tl" />
      <span className="dc-corner-inner tr" />
      <span className="dc-corner-inner bl" />
      <span className="dc-corner-inner br" />
    </>
  );
}

function rowClass(active: boolean) {
  return [
    "relative w-full flex items-center gap-[9px] text-left py-[5px] pl-2 pr-[10px] border",
    active ? "border-hud/30 bg-hud/[.09]" : "border-transparent bg-transparent",
  ].join(" ");
}

/** Pick a row with the mouse: cursor and open in one go. */
function usePick() {
  const qc = useQueryClient();
  return (sha: string | null) => {
    useUI.getState().setFocus("history");
    useUI.getState().setLogCursor(sha);
    void runAction("log.open", qc);
  };
}

function ReviewRow({ active, shown }: { active: boolean; shown: boolean }) {
  const pick = usePick();
  const { data: status } = useStatus();
  const { data: range } = useReview((m) => m.range);
  const n = status?.files.length ?? 0;
  const label = range && !range.includesWorktree ? `BRANCH ${range.head.toUpperCase()}` : "WORKING TREE";
  return (
    <button type="button" data-log="review" onClick={() => pick(null)} className={`${rowClass(active)} min-h-[34px]`}>
      {active && <Brackets />}
      <span className="w-[10px] text-hud">{shown ? "▸" : ""}</span>
      <span className="flex-1 min-w-0 text-[11px] tracking-[0.14em] text-hud whitespace-nowrap overflow-hidden text-ellipsis">
        CURRENT REVIEW · {label} · {n} FILE{n === 1 ? "" : "S"}
      </span>
    </button>
  );
}

export function RefChips({ refs }: { refs: RefLabel[] }) {
  return (
    <span className="flex flex-wrap gap-1">
      {refs.map((r) => (
        <span
          key={r.kind + r.name}
          className={[
            "px-[5px] py-px border text-[9.5px] tracking-[0.08em] whitespace-nowrap",
            r.kind === "branch" ? "border-hud/50 text-hud" : r.kind === "tag" ? "border-sig-warn/50 text-sig-warn" : "border-hud/20 text-ink-dim",
          ].join(" ")}
        >
          {r.kind === "tag" ? "◆ " : ""}
          {r.name}
        </span>
      ))}
    </span>
  );
}

function CommitRow({ entry: e, now, active, shown }: { entry: LogEntry; now: number; active: boolean; shown: boolean }) {
  const pick = usePick();
  return (
    <button type="button" data-log={e.sha} title={e.subject} onClick={() => pick(e.sha)} className={`${rowClass(active)} items-start`}>
      {active && <Brackets />}
      <span className="w-[10px] flex-none text-hud leading-[1.6]">{shown ? "▸" : ""}</span>
      <span className="flex-1 min-w-0 flex flex-col gap-[2px]">
        <span className="flex items-baseline gap-2 min-w-0">
          <span className={["flex-none text-[11px]", e.claude ? "text-sig-agent" : "text-hud"].join(" ")}>{e.short}</span>
          <span className={["min-w-0 text-[12px] whitespace-nowrap overflow-hidden text-ellipsis", active ? "text-ink-bright" : e.onBranch ? "text-ink-light" : "text-ink-dim"].join(" ")}>
            {e.subject}
          </span>
        </span>
        <span className="flex items-center gap-[6px] min-w-0 text-[10px] text-ink-faint whitespace-nowrap">
          <span className="min-w-0 overflow-hidden text-ellipsis">{e.author}</span>
          <span className="flex-none">· {agoLabel(e.time, now).toLowerCase()}</span>
          {e.claude && <span className="flex-none px-1 border border-sig-agent/50 text-sig-agent tracking-[0.1em]">CLAUDE</span>}
          {e.merge && <span className="flex-none px-1 border border-hud/25 text-ink-dim tracking-[0.1em]">MERGE</span>}
          <span className="flex-1" />
          <span className="flex-none">{e.files}f</span>
          <span className="flex-none text-sig-add">+{e.adds}</span>
          <span className="flex-none text-sig-delete">−{e.dels}</span>
        </span>
        {e.refs.length > 0 && <RefChips refs={e.refs} />}
      </span>
    </button>
  );
}
