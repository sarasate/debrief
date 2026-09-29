import { useEffect, useState } from "react";
import type { ThemedToken } from "shiki/core";
import { COLLAPSE_LINES, tokenize } from "../lib/highlight";
import { useQueryClient } from "@tanstack/react-query";
import { HudFrame } from "../components/HudFrame";
import { StatusChip } from "../components/StatusChip";
import { useChangeset, useDiffFile, useReview, useStatus } from "../hooks/useRepo";
import { jumpToLine, runAction } from "../hooks/useKeybindings";
import { groupOf, splitPath } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { keyFor } from "../lib/keymap";
import { useUI } from "../store/ui";
import type { DiffHunk, DiffLine, TurnKey, Verdict } from "../lib/types";
import { agoLabel } from "../hooks/useNow";

const pad2 = (n: number) => String(n).padStart(2, "0");

const toolBtn =
  "flex-none whitespace-nowrap h-[30px] px-[9px] border border-hud/[.22] bg-transparent text-ink-base text-[11px] disabled:opacity-40";

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
  // Branch review: hunks stage/discard can't reach (null = worktree review).
  const { data: uncommitted } = useReview((m) => (m.range && path ? m.files[path]?.uncommitted ?? [] : null));

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
          {/* The folder gives way before the file name does. */}
          <span title={path ?? undefined} className="min-w-0 flex overflow-hidden text-[11.5px] whitespace-nowrap">
            <span className="min-w-0 shrink-[100] text-ink-faint overflow-hidden text-ellipsis">{dirs.length ? dirs.join("/") + "/" : ""}</span>
            <span className="min-w-0 shrink text-hud overflow-hidden text-ellipsis">{name}</span>
          </span>
          {file && <StatusChip status={file.status} inline />}
          <span className="flex-1" />
          <span className="flex-none text-[10.5px] tracking-[0.12em] text-ink-faint whitespace-nowrap">{position}</span>
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
          <label className="flex-none whitespace-nowrap h-[30px] flex items-center gap-[7px] px-[10px] border border-hud/[.22] text-[10.5px] tracking-[0.12em] text-ink-base cursor-pointer has-[:disabled]:opacity-40">
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
            className="flex-none whitespace-nowrap h-[30px] px-3 border-0 bg-hud text-ink-void font-chrome font-bold tracking-[0.14em] text-[11px] shadow-[0_0_18px_color-mix(in_srgb,var(--ac)_40%,transparent)] disabled:opacity-40"
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

        {!path && status && !status.files.length && <CleanState />}
        {!path && status && status.files.length > 0 && <Notice text="NO SIGNAL · SELECT A FILE" />}
        {path && error && <Notice text={errText(error)} danger />}
        {path && isLoading && <Notice text="DECODING…" />}
        {diff?.isBinary && <Notice text="BINARY FILE — NO PREVIEW" />}
        {diff && !diff.isBinary && diff.hunks.length === 0 && <Notice text="NO TEXTUAL CHANGES" />}
        {diff?.hunks.map((h) => (
          <Hunk
            key={h.id}
            path={diff.path}
            hunk={h}
            current={h.id === hunkId}
            verdict={verdicts?.[h.id] ?? null}
            committed={!!uncommitted && !uncommitted.includes(h.id)}
          />
        ))}
      </div>
    </HudFrame>
  );
}

function Briefing({ path }: { path: string }) {
  const { data: model } = useReview();
  const grouping = useUI((s) => s.grouping);
  const group = groupOf(model, path, grouping);
  const file = group?.files.find((f) => f.path === path);
  const sessions = new Set(model?.turns.map((t) => t.sessionId)).size;
  const turnLabel = (k: TurnKey) => {
    const t = model?.turns.find((x) => x.sessionId === k.sessionId && x.index === k.index);
    const s = sessions > 1 ? `S${[...new Set(model!.turns.map((x) => x.sessionId))].indexOf(k.sessionId) + 1} ` : "";
    return { text: `${s}turn ${k.index}`, title: t?.title ?? "" };
  };
  const commit = group?.commit;
  // Commit groups speak for a commit, not for Claude, unless Claude co-authored it.
  const agent = !commit || commit.claude;

  return (
    <div
      className={[
        "flex-none flex flex-col gap-[5px] px-[13px] py-[11px] border",
        agent ? "bg-sig-agent/[.07] border-sig-agent/30" : "bg-hud/[.04] border-hud/25",
      ].join(" ")}
    >
      <span
        className={[
          "text-[10px] tracking-[0.22em] whitespace-nowrap overflow-hidden text-ellipsis",
          agent ? "text-sig-agent" : "text-hud",
        ].join(" ")}
      >
        {commit ? "COMMIT" : "AGENT BRIEFING"} · {group ? group.title : model ? "NOT IN ANY GROUP" : "SCANNING"}
      </span>
      {commit ? (
        <span className="text-[11px] leading-[1.55] text-ink-dim">
          <span className={agent ? "text-sig-agent" : "text-hud"}>{commit.short}</span> · {commit.author} ·{" "}
          {agoLabel(commit.time, Date.now()).toLowerCase()}
          {commit.claude && <span className="text-sig-agent"> · CO-AUTHORED BY CLAUDE</span>}
        </span>
      ) : (
        group?.prompt && (
          <span className="text-[11px] leading-[1.55] text-ink-dim whitespace-pre-wrap break-words">
            <span className="text-sig-agent">TURN {group.turn} ›</span> {group.prompt}
          </span>
        )
      )}
      <span className="text-[12px] leading-[1.6] text-ink-light whitespace-pre-wrap break-words">
        {group?.briefing ?? "No briefing for this file."}
      </span>
      {file && (file.tools.length > 0 || file.alsoTurns.length > 0 || file.alsoCommits.length > 0) && (
        <span className="text-[10.5px] tracking-[0.08em] text-ink-faint">
          {file.tools.length > 0 && <>via {file.tools.join(", ")}</>}
          {file.alsoTurns.map((k) => {
            const l = turnLabel(k);
            return (
              <span key={k.sessionId + ":" + k.index}>
                {" · "}also touched in {l.text}
                {l.title && <span className="text-ink-dim"> ({l.title})</span>}
              </span>
            );
          })}
          {file.alsoCommits.length > 0 && <> · also in {file.alsoCommits.join(", ")}</>}
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

/** The worktree matches HEAD: nothing for Claude left behind. */
function CleanState() {
  const { data: model } = useReview();
  const head = model?.status.head;
  const session = model?.session;
  const range = model?.range;
  if (range) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="relative w-[420px] max-w-full flex flex-col items-center gap-3 px-8 py-10 text-center">
          <span className="dc-corner-outer tl" />
          <span className="dc-corner-outer tr" />
          <span className="dc-corner-outer bl" />
          <span className="dc-corner-outer br" />
          <span className="font-chrome font-bold tracking-[0.3em] text-[18px] text-hud">NO CHANGES</span>
          <span className="text-[10.5px] tracking-[0.3em] text-ink-dim uppercase">
            {range.head} matches {range.base}
          </span>
          <span className="text-[11.5px] leading-[1.6] text-ink-faint">
            Nothing on this branch since it left {range.base}. B picks another branch or the working tree.
          </span>
        </div>
      </div>
    );
  }
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="relative w-[420px] max-w-full flex flex-col items-center gap-3 px-8 py-10 text-center">
        <span className="dc-corner-outer tl" />
        <span className="dc-corner-outer tr" />
        <span className="dc-corner-outer bl" />
        <span className="dc-corner-outer br" />
        <span className="font-chrome font-bold tracking-[0.3em] text-[18px] text-hud [text-shadow:0_0_18px_color-mix(in_srgb,var(--ac)_50%,transparent)]">
          NO SIGNAL
        </span>
        <span className="text-[10.5px] tracking-[0.3em] text-ink-dim">WORKTREE CLEAN</span>
        <span className="text-[11.5px] leading-[1.6] text-ink-faint">
          Nothing uncommitted{head ? ` on ${head.branch ?? "a detached HEAD"} at ${head.sha.slice(0, 6)}` : ""}.
          {session ? " Watching the Claude session; its next write shows up here." : " Debrief refreshes when the worktree changes."}
        </span>
      </div>
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

const verdictBtn = "dc-hov flex-none whitespace-nowrap h-7 px-[10px] text-[10.5px] tracking-[0.14em] border";
const VERDICT_OFF = "border-hud/20 bg-transparent text-ink-dim";
const KEEP_ON = "border-sig-okDark bg-sig-okDark/[.22] text-sig-ok";
const REVERT_ON = "border-sig-dangerDark bg-sig-dangerDark/[.22] text-sig-deleteHi";

function useTokens(path: string, hunk: DiffHunk, enabled: boolean) {
  const [tokens, setTokens] = useState<ThemedToken[][] | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    setTokens(null);
    void tokenize(`${path}:${hunk.id}`, path, hunk.lines.map((l) => l.content)).then((t) => live && setTokens(t));
    return () => {
      live = false;
    };
  }, [path, hunk, enabled]);
  return tokens;
}

function Hunk({
  path,
  hunk,
  current,
  verdict,
  committed,
}: {
  path: string;
  hunk: DiffHunk;
  current: boolean;
  verdict: Verdict | null;
  committed: boolean;
}) {
  const qc = useQueryClient();
  const large = hunk.lines.length > COLLAPSE_LINES;
  const open = useUI((s) => !large || !!s.expanded[hunk.id]);
  const tokens = useTokens(path, hunk, open);
  const decide = (action: "hunk.keep" | "hunk.revert") => (e: React.MouseEvent) => {
    e.stopPropagation();
    useUI.getState().setHunk(hunk.id);
    void runAction(action, qc);
  };
  return (
    <div
      data-hunk={hunk.id}
      data-new-start={hunk.newStart}
      data-new-end={hunk.newStart + hunk.newLines - 1}
      onClick={() => useUI.getState().setHunk(hunk.id)}
      className={["bg-bg-deep border", current ? "border-hud/40" : "border-hud/[.12]"].join(" ")}
    >
      <div className="flex items-center gap-2 pl-3 pr-[6px] py-[5px] border-b border-hud/10 bg-hud/[.03]">
        <span className="min-w-0 text-[11.5px] text-ink-faint whitespace-nowrap overflow-hidden text-ellipsis">
          {hunk.header}
        </span>
        <span className="flex-1" />
        {committed && (
          <span
            title="Already committed on the branch: it can be marked, but not staged or discarded"
            className="flex-none px-[6px] py-px border border-hud/25 text-[10px] tracking-[0.16em] text-ink-dim whitespace-nowrap"
          >
            COMMITTED
          </span>
        )}
        {verdict === "revert" && (
          <span className="flex-none text-[10px] tracking-[0.16em] text-sig-danger whitespace-nowrap">
            {committed ? "REVERT REQUESTED" : "MARKED FOR DISCARD"}
          </span>
        )}
        <button type="button" onClick={decide("hunk.keep")} className={`${verdictBtn} ${verdict === "keep" ? KEEP_ON : VERDICT_OFF}`}>
          <span className="font-bold">Y</span> KEEP
        </button>
        <button type="button" onClick={decide("hunk.revert")} className={`${verdictBtn} ${verdict === "revert" ? REVERT_ON : VERDICT_OFF}`}>
          <span className="font-bold">X</span> REVERT
        </button>
      </div>
      {open ? (
        <div className={["overflow-x-auto py-[6px]", verdict === "revert" ? "opacity-[.35]" : ""].join(" ")}>
          {hunk.lines.map((l, i) => (
            <Line key={i} line={l} tokens={tokens?.[i]} />
          ))}
        </div>
      ) : (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            useUI.getState().setHunk(hunk.id);
            useUI.getState().toggleExpanded(hunk.id, true);
          }}
          className="dc-hov w-full py-3 text-center text-[10.5px] tracking-[0.16em] text-ink-faint"
        >
          ⋯ {hunk.lines.length.toLocaleString()} LINES COLLAPSED ·{" "}
          <span className="text-hud">{keyFor("hunk.expand")}</span> TO EXPAND
        </button>
      )}
    </div>
  );
}

const LINE_CLASS = { addition: "dc-diff-add", deletion: "dc-diff-del", context: "dc-diff-ctx" } as const;
const SIGN = { addition: "+", deletion: "−", context: " " } as const;

function Line({ line, tokens }: { line: DiffLine; tokens?: ThemedToken[] }) {
  return (
    <div
      data-line
      data-new-line={line.newLineno ?? undefined}
      className={`flex min-w-max text-[12px] leading-[1.92] ${LINE_CLASS[line.kind]} ${tokens ? "dc-syntax" : ""}`}
    >
      <span className="w-10 flex-none text-right pr-2 text-ink-darkest select-none">{line.oldLineno ?? ""}</span>
      <span className="w-10 flex-none text-right pr-2 text-ink-darkest select-none">{line.newLineno ?? ""}</span>
      <span className="w-[18px] flex-none text-center select-none">{SIGN[line.kind]}</span>
      <span className="whitespace-pre pr-6">
        {tokens
          ? tokens.map((t, i) => (
              <span key={i} style={{ color: t.color, fontStyle: t.fontStyle === 1 ? "italic" : undefined }}>
                {t.content}
              </span>
            ))
          : line.content}
      </span>
    </div>
  );
}
