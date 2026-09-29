import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useUI } from "../store/ui";
import { useReview } from "../hooks/useRepo";
import { api, errText } from "../lib/invoke";
import { discardReverted, revertTargets } from "../lib/review";
import type { DiffHunk } from "../lib/types";

interface Row {
  path: string;
  hunk: DiffHunk | null;
  id: string;
}

/** Confirmation for `d` (SPEC §5): lists every hunk that will be reverse-applied. */
export function DiscardModal() {
  const open = useUI((s) => s.modal === "discard");
  const close = () => useUI.getState().openModal(null);
  const qc = useQueryClient();
  const { data: model } = useReview();
  const [rows, setRows] = useState<Row[] | null>(null);
  const [busy, setBusy] = useState(false);
  const boxRef = useRef<HTMLDivElement>(null);

  // Snapshot the targets when the modal opens, with headers from the diff.
  useEffect(() => {
    if (!open) {
      setRows(null);
      return;
    }
    const targets = revertTargets(model);
    let live = true;
    Promise.all(
      targets.map(async (t) => {
        const diff = await api.diffFile(t.path).catch(() => null);
        return t.ids.map((id) => ({ path: t.path, id, hunk: diff?.hunks.find((h) => h.id === id) ?? null }));
      }),
    ).then((r) => live && setRows(r.flat()));
    setTimeout(() => boxRef.current?.focus(), 0);
    return () => {
      live = false;
    };
    // Snapshot on open only: later refreshes must not change what was confirmed.
  }, [open]);

  if (!open) return null;

  async function confirm() {
    if (busy || !rows?.length) return;
    setBusy(true);
    try {
      await discardReverted(qc);
    } catch (e) {
      useUI.getState().emitToast("err", errText(e));
    } finally {
      setBusy(false);
      close();
    }
  }

  const files = new Set(rows?.map((r) => r.path)).size;
  const n = rows?.length ?? 0;

  return (
    <div onClick={close} className="absolute inset-0 z-[93] flex items-center justify-center p-6 bg-ink-void/[.82]">
      <div
        ref={boxRef}
        tabIndex={-1}
        role="alertdialog"
        aria-labelledby="discard-title"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // Enter on a focused button is that button's click (CANCEL must
          // stay cancel); only Enter on the dialog itself confirms.
          if (e.key === "Enter" && e.target === e.currentTarget) {
            e.preventDefault();
            void confirm();
          }
        }}
        className="w-[640px] max-w-[94%] max-h-[80vh] flex flex-col bg-bg-panel border border-sig-danger outline-none shadow-[0_0_60px_theme(colors.sig.danger/22%)]"
      >
        <header className="flex justify-between items-center px-[22px] py-4 border-b border-sig-danger/30">
          <span id="discard-title" className="font-chrome font-bold tracking-[0.24em] text-[14px] text-sig-danger">
            DISCARD {n} REVERTED HUNK{n === 1 ? "" : "S"}
          </span>
          <span className="text-[10px] tracking-[0.2em] text-ink-dimmer">ESC TO CANCEL</span>
        </header>
        <div className="px-[22px] pt-3 text-[11.5px] leading-[1.6] text-ink-base">
          These hunks will be reverse-applied to the working tree in {files} file{files === 1 ? "" : "s"}. Debrief can't undo this.
          A file that changed since the hunk was marked is left alone.
        </div>
        <ul className="flex-1 min-h-0 overflow-y-auto px-[22px] py-3 flex flex-col gap-[6px]">
          {!rows && <li className="text-[11px] text-ink-faint">reading diff…</li>}
          {rows?.map((r) => (
            <li key={r.id} className="flex items-center gap-3 px-[10px] py-[7px] bg-bg-deep border border-sig-danger/25 text-[11.5px]">
              <span className="min-w-0 flex-1 flex flex-col">
                <span className="text-ink-light whitespace-nowrap overflow-hidden text-ellipsis">{r.path}</span>
                <span className="text-[10.5px] text-ink-faint whitespace-nowrap overflow-hidden text-ellipsis">
                  {r.hunk?.header ?? "hunk no longer in the diff · will be skipped"}
                </span>
              </span>
              {r.hunk && (
                <span className="flex-none text-[10.5px]">
                  <span className="text-sig-add">+{r.hunk.lines.filter((l) => l.kind === "addition").length}</span>{" "}
                  <span className="text-sig-delete">−{r.hunk.lines.filter((l) => l.kind === "deletion").length}</span>
                </span>
              )}
            </li>
          ))}
        </ul>
        <footer className="flex justify-end gap-[6px] px-[22px] py-4 border-t border-sig-danger/20">
          <button
            type="button"
            onClick={close}
            className="dc-hov h-9 px-4 border border-hud/25 bg-transparent text-ink-light text-[10.5px] tracking-[0.14em]"
          >
            ESC · CANCEL
          </button>
          <button
            type="button"
            disabled={busy || !rows?.length}
            onClick={() => void confirm()}
            className="h-9 px-4 border-0 bg-sig-danger text-ink-void font-chrome font-bold text-[11px] tracking-[0.14em] disabled:opacity-40"
          >
            ⏎ · DISCARD {n}
          </button>
        </footer>
      </div>
    </div>
  );
}
