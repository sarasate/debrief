import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { HudFrame } from "../components/HudFrame";
import { StatusChip } from "../components/StatusChip";
import { useChangeset, useDiffFile, useReview, useStatus } from "../hooks/useRepo";
import { jumpToLine, runAction } from "../hooks/useKeybindings";
import { groupOf, splitPath } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { keyFor } from "../lib/keymap";
import { useUI } from "../store/ui";
import type { DiffHunk, DiffLine, Verdict } from "../lib/types";

const pad2 = (n: number) => String(n).padStart(2, "0");

const toolBtn =
  "h-[30px] px-[9px] border border-hud/[.22] bg-transparent text-ink-base text-[11px] disabled:opacity-40";

export function DiffPanel() {
  const path = useUI((s) => s.selectedPath);
  const hunkId = useUI((s) => s.hunkId);
  const collapsed = useUI((s) => s.collapsed);
  const { data: status } = useStatus();
  const { order } = useChangeset();
  const { data: diff, isLoading, error } = useDiffFile(path);
  const file = status?.files.find((f) => f.path === path);
  const qc = useQueryClient();
  const { data: viewed } = useReview((m) => (path ? !!m.files[path]?.viewed : false));
  const { data: verdicts } = useReview((m) => m.verdicts);

  // Keep the hunk cursor on a hunk that exists; a refresh can drop it.
  useEffect(() => {
    if (!diff) return;
    if (!hunkId || !diff.hunks.some((h) => h.id === hunkId)) {
      useUI.getState().setHunk(diff.hunks[0]?.id ?? null, false);
    }
  }, [diff, hunkId]);

  // A new file starts at the top.
  useEffect(() => {
    const el = document.querySelector<HTMLElement>('[data-panel-scroll="diff"]');
    if (el) el.scrollTop = 0;
  }, [path]);

  const idx = path ? order.indexOf(path) : -1;
  const folded = path && splitPath(path).dirs.some((_, i, d) => collapsed[d.slice(0, i + 1).join("/")]);
  const position =
    idx !== -1 ? `${pad2(idx + 1)} / ${pad2(order.length)}` : !path ? "" : folded ? "FOLDED" : "NOT IN FILTER";
  const { name, dirs } = splitPath(path ?? "");

  return (
    <HudFrame
      id="diff"
      title="Diff analysis"
      className="flex-1"
      bg="bg-bg-panelDeep"
      toolbar={
        <>
          <span className="min-w-0 text-[11.5px] whitespace-nowrap overflow-hidden text-ellipsis">
            <span className="text-ink-faint">{dirs.length ? dirs.join("/") + "/" : ""}</span>
            <span className="text-hud">{name}</span>
          </span>
          {file && <StatusChip status={file.status} inline />}
          <span className="flex-1" />
          <span className="text-[10.5px] tracking-[0.12em] text-ink-faint whitespace-nowrap">{position}</span>
          <button
            type="button"
            aria-label="Previous file"
            className={`dc-hov ${toolBtn}`}
            onClick={() => void runAction("file.prev", qc)}
          >
            <span className="text-hud">{keyFor("file.prev").toUpperCase()}</span> ↑
          </button>
          <button
            type="button"
            aria-label="Next file"
            className={`dc-hov ${toolBtn}`}
            onClick={() => void runAction("file.next", qc)}
          >
            <span className="text-hud">{keyFor("file.next").toUpperCase()}</span> ↓
          </button>
          <label className="h-[30px] flex items-center gap-[7px] px-[10px] border border-hud/[.22] text-[10.5px] tracking-[0.12em] text-ink-base cursor-pointer has-[:disabled]:opacity-40">
            <input
              type="checkbox"
              disabled={!file}
              checked={!!viewed}
              onChange={() => void runAction("file.viewed", qc)}
              className="m-0 accent-[theme(colors.sig.ok)]"
            />
            VIEWED
          </label>
          <button
            type="button"
            disabled={!file}
            onClick={() => void runAction("file.clear", qc)}
            className="h-[30px] px-3 border-0 bg-hud text-ink-void font-chrome font-bold tracking-[0.14em] text-[11px] shadow-[0_0_18px_color-mix(in_srgb,var(--ac)_40%,transparent)] disabled:opacity-40"
          >
            SPC · CLEAR &amp; NEXT
          </button>
        </>
      }
    >
      <div
        data-panel-scroll="diff"
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 px-[14px] pt-3 pb-5"
      >
        {path && <Briefing path={path} />}
        {path && <FlagBanner path={path} />}

        {!path && <Notice text={status && !status.files.length ? "NO SIGNAL · WORKTREE CLEAN" : "NO SIGNAL · SELECT A FILE"} />}
        {path && error && <Notice text={errText(error)} danger />}
        {path && isLoading && <Notice text="DECODING…" />}
        {diff?.isBinary && <Notice text="BINARY FILE — NO PREVIEW" />}
        {diff && !diff.isBinary && diff.hunks.length === 0 && <Notice text="NO TEXTUAL CHANGES" />}
        {diff?.hunks.map((h) => (
          <Hunk key={h.id} hunk={h} current={h.id === hunkId} verdict={verdicts?.[h.id] ?? null} />
        ))}
      </div>
    </HudFrame>
  );
}

function Briefing({ path }: { path: string }) {
  const { data: model } = useReview();
  const group = groupOf(model, path);
  const file = group?.files.find((f) => f.path === path);
  const turnTitle = (i: number) => model?.turns.find((t) => t.index === i)?.title ?? "";

  return (
    <div className="flex-none flex flex-col gap-[5px] px-[13px] py-[11px] bg-sig-agent/[.07] border border-sig-agent/30">
      <span className="text-[10px] tracking-[0.22em] text-sig-agent whitespace-nowrap overflow-hidden text-ellipsis">
        AGENT BRIEFING · {group ? group.title : model ? "NOT IN ANY GROUP" : "SCANNING"}
      </span>
      {group?.prompt && (
        <span className="text-[11px] leading-[1.55] text-ink-dim whitespace-pre-wrap break-words">
          <span className="text-sig-agent">TURN {group.turn} ›</span> {group.prompt}
        </span>
      )}
      <span className="text-[12px] leading-[1.6] text-ink-light whitespace-pre-wrap break-words">
        {group?.briefing ?? "No briefing for this file."}
      </span>
      {file && (file.tools.length > 0 || file.alsoTurns.length > 0) && (
        <span className="text-[10.5px] tracking-[0.08em] text-ink-faint">
          {file.tools.length > 0 && <>via {file.tools.join(", ")}</>}
          {file.alsoTurns.map((t) => (
            <span key={t}>
              {" · "}also touched in turn {t}
              {turnTitle(t) && <span className="text-ink-dim"> ({turnTitle(t)})</span>}
            </span>
          ))}
        </span>
      )}
    </div>
  );
}

function FlagBanner({ path }: { path: string }) {
  const { data: flags } = useReview((m) => m.files[path]?.flags);
  if (!flags?.length) return null;
  return (
    <div className="flex-none flex flex-col gap-[5px] px-[13px] py-[11px] bg-sig-warn/[.08] border border-sig-warn/45">
      <span className="text-[10px] tracking-[0.22em] text-sig-warn">
        ⚠ FLAGGED FOR CLOSER LOOK{flags.length > 1 ? ` · ${flags.length}` : ""}
      </span>
      {flags.map((f, i) => (
        <span key={i} className="flex items-baseline gap-2 text-[12px] leading-[1.6] text-sig-warnInk">
          <span className="flex-1 min-w-0 break-words">{f.reason}</span>
          {f.line != null && (
            <button
              type="button"
              onClick={() => jumpToLine(f.line!)}
              className="dc-hov flex-none h-[22px] px-2 border border-sig-warn/45 text-[10px] tracking-[0.12em] text-sig-warn"
            >
              ! L{f.line}
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

function Notice({ text, danger = false }: { text: string; danger?: boolean }) {
  return (
    <div
      className={[
        "px-3 py-7 text-center text-[11px] tracking-[0.14em] border border-dashed border-hud/20 break-words",
        danger ? "text-sig-danger" : "text-ink-faint",
      ].join(" ")}
    >
      {text}
    </div>
  );
}

const verdictBtn = "dc-hov h-7 px-[10px] text-[10.5px] tracking-[0.14em] border";
const VERDICT_OFF = "border-hud/20 bg-transparent text-ink-dim";
const KEEP_ON = "border-sig-okDark bg-sig-okDark/[.22] text-sig-ok";
const REVERT_ON = "border-sig-dangerDark bg-sig-dangerDark/[.22] text-sig-deleteHi";

function Hunk({ hunk, current, verdict }: { hunk: DiffHunk; current: boolean; verdict: Verdict | null }) {
  const qc = useQueryClient();
  const decide = (action: "hunk.keep" | "hunk.revert") => (e: React.MouseEvent) => {
    e.stopPropagation();
    useUI.getState().setHunk(hunk.id);
    void runAction(action, qc);
  };
  return (
    <div
      data-hunk={hunk.id}
      onClick={() => useUI.getState().setHunk(hunk.id)}
      className={["bg-bg-deep border", current ? "border-hud/40" : "border-hud/[.12]"].join(" ")}
    >
      <div className="flex items-center gap-2 pl-3 pr-[6px] py-[5px] border-b border-hud/10 bg-hud/[.03]">
        <span className="min-w-0 text-[11.5px] text-ink-faint whitespace-nowrap overflow-hidden text-ellipsis">
          {hunk.header}
        </span>
        <span className="flex-1" />
        {verdict === "revert" && (
          <span className="text-[10px] tracking-[0.16em] text-sig-danger whitespace-nowrap">MARKED FOR DISCARD</span>
        )}
        <button type="button" onClick={decide("hunk.keep")} className={`${verdictBtn} ${verdict === "keep" ? KEEP_ON : VERDICT_OFF}`}>
          <span className="font-bold">Y</span> KEEP
        </button>
        <button type="button" onClick={decide("hunk.revert")} className={`${verdictBtn} ${verdict === "revert" ? REVERT_ON : VERDICT_OFF}`}>
          <span className="font-bold">X</span> REVERT
        </button>
      </div>
      <div className={["overflow-x-auto py-[6px]", verdict === "revert" ? "opacity-[.35]" : ""].join(" ")}>
        {hunk.lines.map((l, i) => (
          <Line key={i} line={l} />
        ))}
      </div>
    </div>
  );
}

const LINE_CLASS = { addition: "dc-diff-add", deletion: "dc-diff-del", context: "dc-diff-ctx" } as const;
const SIGN = { addition: "+", deletion: "−", context: " " } as const;

function Line({ line }: { line: DiffLine }) {
  return (
    <div data-line data-new-line={line.newLineno ?? undefined} className={`flex min-w-max text-[12px] leading-[1.92] ${LINE_CLASS[line.kind]}`}>
      <span className="w-10 flex-none text-right pr-2 text-ink-darkest select-none">{line.oldLineno ?? ""}</span>
      <span className="w-10 flex-none text-right pr-2 text-ink-darkest select-none">{line.newLineno ?? ""}</span>
      <span className="w-[18px] flex-none text-center select-none">{SIGN[line.kind]}</span>
      <span className="whitespace-pre pr-6">{line.content}</span>
    </div>
  );
}
