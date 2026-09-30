import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { HudFrame } from "../components/HudFrame";
import { StatusChip } from "../components/StatusChip";
import { useCommit, useFileLines } from "../hooks/useRepo";
import { runAction } from "../hooks/useKeybindings";
import { splitPath } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { keyFor, keyLabel } from "../lib/keymap";
import { useUI } from "../store/ui";
import type { FileDiff, LogEntry } from "../lib/types";
import { Hunk, Notice, Unchanged, segments } from "./DiffPanel";
import { RefChips } from "./HistoryPanel";

const toolBtn =
  "flex-none whitespace-nowrap h-[30px] px-[9px] border border-hud/[.22] bg-transparent text-ink-base text-[11px] disabled:opacity-40";

/** A past commit, read-only, in place of the review diff (M13). */
export function CommitPanel({ sha }: { sha: string }) {
  const qc = useQueryClient();
  const { data, isLoading, error } = useCommit(sha);
  const logHunk = useUI((s) => s.logHunk);
  const fullFile = useUI((s) => s.fullFile);

  // Each commit starts at the top, with the cursor on its first hunk.
  useEffect(() => {
    const el = document.querySelector<HTMLElement>('[data-panel-scroll="diff"]');
    if (el) el.scrollTop = 0;
  }, [sha]);
  useEffect(() => {
    const first = data?.files.find((f) => f.hunks.length)?.hunks[0]?.id ?? null;
    if (data && !useUI.getState().logHunk) useUI.getState().setLogHunk(first);
  }, [data]);

  const e = data?.entry;
  return (
    <HudFrame
      id="diff"
      title="Commit"
      className="flex-1"
      bg="bg-bg-panelDeep"
      toolbar={
        <>
          <span className="min-w-0 flex gap-2 overflow-hidden text-[11.5px] whitespace-nowrap">
            <span className={e?.claude ? "text-sig-agent" : "text-hud"}>{e?.short ?? sha.slice(0, 7)}</span>
            <span className="min-w-0 text-ink-light overflow-hidden text-ellipsis">{e?.subject}</span>
          </span>
          <span className="flex-1" />
          {data && (
            <span className="flex-none text-[10.5px] tracking-[0.12em] text-ink-faint whitespace-nowrap">
              {data.files.length + data.omitted} FILE{data.files.length + data.omitted === 1 ? "" : "S"}
            </span>
          )}
          <button
            type="button"
            aria-pressed={fullFile}
            title="Whole file / changes only"
            className={`dc-hov ${toolBtn} ${fullFile ? "border-hud/60 text-hud" : ""}`}
            onClick={() => void runAction("file.full", qc)}
          >
            <span className="text-hud">{keyFor("file.full").toUpperCase()}</span> {fullFile ? "WHOLE FILE" : "CHANGES"}
          </button>
          <button
            type="button"
            onClick={() => void runAction("close", qc)}
            className="flex-none whitespace-nowrap h-[30px] px-3 border-0 bg-hud text-ink-void font-chrome font-bold tracking-[0.14em] text-[11px] shadow-[0_0_18px_color-mix(in_srgb,var(--ac)_40%,transparent)]"
          >
            {keyLabel("Escape").toUpperCase()} · BACK TO REVIEW
          </button>
        </>
      }
    >
      <div data-panel-scroll="diff" className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 px-[14px] pt-3 pb-5">
        {error && <Notice text={errText(error)} danger />}
        {isLoading && <Notice text="DECODING…" />}
        {data && <Header entry={data.entry} parent={data.parent} />}
        {data && data.files.length === 0 && <Notice text={data.entry.merge ? "MERGE · NO CHANGES AGAINST THE FIRST PARENT" : "EMPTY COMMIT"} />}
        {data?.files.map((f) => <CommitFile key={f.path} sha={sha} file={f} cursor={logHunk} fullFile={fullFile} />)}
        {!!data?.omitted && <Notice text={`${data.omitted} MORE FILE${data.omitted === 1 ? "" : "S"} NOT SHOWN`} />}
      </div>
    </HudFrame>
  );
}

function Header({ entry: e, parent }: { entry: LogEntry; parent: string | null }) {
  return (
    <div
      className={[
        "flex-none flex flex-col gap-[5px] px-[13px] py-[11px] border",
        e.claude ? "bg-sig-agent/[.07] border-sig-agent/30" : "bg-hud/[.04] border-hud/25",
      ].join(" ")}
    >
      <span className={["text-[10px] tracking-[0.22em]", e.claude ? "text-sig-agent" : "text-hud"].join(" ")}>
        {e.merge ? "MERGE COMMIT" : "COMMIT"} · {e.sha}
        {e.claude && " · CO-AUTHORED BY CLAUDE"}
      </span>
      <span className="text-[11px] leading-[1.55] text-ink-dim">
        {e.author} · {new Date(e.time).toLocaleString()}
        {parent && <> · parent {parent.slice(0, 7)}</>}
        {e.merge && " (first parent)"}
      </span>
      {e.refs.length > 0 && <RefChips refs={e.refs} />}
      <span className="text-[12.5px] leading-[1.6] text-ink-bright break-words">{e.subject}</span>
      {e.body && <span className="text-[12px] leading-[1.6] text-ink-light whitespace-pre-wrap break-words">{e.body}</span>}
    </div>
  );
}

function CommitFile({ sha, file, cursor, fullFile }: { sha: string; file: FileDiff; cursor: string | null; fullFile: boolean }) {
  const { data: lines, error } = useFileLines(file.path, fullFile && !file.isBinary, sha);
  const { name, dirs } = splitPath(file.path);
  const adds = file.hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === "addition").length, 0);
  const dels = file.hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === "deletion").length, 0);
  return (
    <>
      <div
        title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
        className="sticky top-[-12px] z-[2] flex-none flex items-center gap-2 px-3 py-[6px] bg-bg-panelDeep border-b border-hud/20 text-[11.5px]"
      >
        <StatusChip status={file.status} inline />
        <span className="min-w-0 flex overflow-hidden whitespace-nowrap">
          <span className="min-w-0 shrink-[100] text-ink-faint overflow-hidden text-ellipsis">
            {file.oldPath ? file.oldPath + " → " : ""}
            {dirs.length ? dirs.join("/") + "/" : ""}
          </span>
          <span className="min-w-0 shrink text-hud overflow-hidden text-ellipsis">{name}</span>
        </span>
        <span className="flex-1" />
        {!file.isBinary && (
          <>
            <span className="text-[10.5px] text-sig-add">+{adds}</span>
            <span className="text-[10.5px] text-sig-delete">−{dels}</span>
          </>
        )}
      </div>
      {file.isBinary && <Notice text="BINARY FILE — NO PREVIEW" />}
      {!file.isBinary && file.hunks.length === 0 && <Notice text="NO TEXTUAL CHANGES" />}
      {fullFile && error && <Notice text={errText(error)} danger />}
      {segments(file.hunks, fullFile ? lines : undefined).map((s) =>
        s.kind === "gap" ? (
          <Unchanged key={`gap:${s.newStart}`} path={file.path} gap={s} />
        ) : (
          <Hunk
            key={s.hunk.id}
            path={file.path}
            hunk={s.hunk}
            current={s.hunk.id === cursor}
            verdict={null}
            committed={false}
            readOnly
          />
        ),
      )}
    </>
  );
}
