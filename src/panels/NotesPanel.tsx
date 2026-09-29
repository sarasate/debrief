import { useQueryClient } from "@tanstack/react-query";
import { HudFrame } from "../components/HudFrame";
import { useDiffFile, useReview, useSettings } from "../hooks/useRepo";
import { runAction } from "../hooks/useKeybindings";
import { errText } from "../lib/invoke";
import { noteTarget, queueNote, removeNote } from "../lib/notes";
import { useUI } from "../store/ui";
import type { TransmitMode } from "../lib/types";

const MODE_LABEL: Record<TransmitMode, string> = {
  clipboard: "clipboard",
  file: ".git/debrief/feedback.md",
  resume: "claude --resume",
};

export function NotesPanel() {
  const qc = useQueryClient();
  const { data: notes = [] } = useReview((m) => m.notes);
  const { data: discards = 0 } = useReview((m) => m.unreportedDiscards);
  const { data: settings } = useSettings();
  const focus = useUI((s) => s.focus);
  const cursor = useUI((s) => s.notesCursor);
  const draft = useUI((s) => s.noteDraft);
  const running = useUI((s) => !!s.transmit?.running);
  // Re-render on the inputs noteTarget reads.
  const path = useUI((s) => s.selectedPath);
  useUI((s) => [s.hunkId, s.hunkPinned, s.noteScope].join());
  useDiffFile(path);
  const target = noteTarget(qc);
  const canSend = (notes.length > 0 || discards > 0) && !running;

  const guard = (p: Promise<unknown>) => p.catch((e) => useUI.getState().emitToast("err", errText(e)));

  return (
    <HudFrame
      id="notes"
      title="Field notes"
      className="flex-1"
      badge={<span className="px-[6px] py-px border border-sig-agent/50 text-sig-agent">→ CLAUDE · {notes.length}</span>}
    >
      <div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 px-3 py-[10px]">
        {notes.map((n, i) => {
          const selected = focus === "notes" && i === Math.min(cursor, notes.length - 1);
          return (
            <div
              key={n.id}
              onClick={() => useUI.getState().setNotesCursor(i)}
              className={[
                "relative flex flex-col gap-1 px-[10px] py-2 bg-bg-deep border",
                selected ? "border-hud/40" : "border-hud/[.12]",
              ].join(" ")}
            >
              <div className="flex items-center gap-[6px]">
                <span
                  title={n.path}
                  className="flex-1 min-w-0 text-[10.5px] text-hud whitespace-nowrap overflow-hidden text-ellipsis"
                >
                  @ {n.path.split("/").pop()}
                  {n.hunkHeader && <span className="text-ink-faint"> · {n.hunkHeader.split(" @@")[0]} @@</span>}
                </span>
                <button
                  type="button"
                  aria-label="Remove note"
                  onClick={(e) => {
                    e.stopPropagation();
                    void guard(removeNote(qc, n.id));
                  }}
                  className="w-[22px] h-[22px] border-0 bg-transparent text-ink-faint text-[12px] hover:text-ink-light"
                >
                  ×
                </button>
              </div>
              <span className="text-[11.5px] leading-[1.55] text-ink-base whitespace-pre-wrap break-words">{n.text}</span>
            </div>
          );
        })}
        {notes.length === 0 && (
          <div className="px-[10px] py-[14px] border border-dashed border-hud/20 text-[11px] leading-[1.6] text-ink-faint">
            No notes queued. Notes are sent to the Claude session as one batch.
          </div>
        )}
        {discards > 0 && (
          <div className="text-[10.5px] tracking-[0.08em] text-ink-faint">
            + {discards} discarded hunk{discards === 1 ? "" : "s"} will be listed in the next transmit
          </div>
        )}
      </div>
      <div className="flex-none flex flex-col gap-2 px-3 pt-[10px] pb-3 border-t border-hud/10">
        <label htmlFor="note" className="text-[10px] tracking-[0.16em] text-ink-dim whitespace-nowrap overflow-hidden text-ellipsis">
          NOTE ON <span className="text-hud">{target?.name ?? "—"}</span>
          {target?.hunk && (
            <span className="text-ink-faint">
              {" "}
              · HUNK {target.hunk.index}/{target.hunk.of}
            </span>
          )}
        </label>
        <textarea
          id="note"
          data-note-input
          rows={3}
          disabled={!target}
          value={draft}
          onChange={(e) => useUI.getState().setNoteDraft(e.target.value)}
          onKeyDown={(e) => {
            // Enter queues; shift+Enter is a newline.
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void guard(queueNote(qc));
            }
          }}
          placeholder="> what should Claude change here…  (⏎ queue · ⇧⏎ newline)"
          className="resize-none border border-hud/[.18] focus:border-hud/50 bg-bg-deep text-ink-light px-[10px] py-2 text-[11.5px] leading-[1.55] outline-none placeholder:text-ink-dimmer disabled:opacity-60"
        />
        <div className="grid grid-cols-2 gap-[6px]">
          <button
            type="button"
            disabled={!draft.trim() || !target}
            onClick={() => void guard(queueNote(qc))}
            className="dc-hov h-9 border border-hud/25 bg-transparent text-ink-light text-[10.5px] tracking-[0.14em] disabled:opacity-40"
          >
            <span className="text-hud font-bold">⏎</span> QUEUE NOTE
          </button>
          <button
            type="button"
            disabled={!canSend}
            onClick={() => void guard(runAction("notes.transmit", qc))}
            className="h-9 border-0 bg-sig-agent text-ink-agentVoid font-chrome font-bold text-[11px] tracking-[0.14em] shadow-[0_0_18px_theme(colors.sig.agent/40%)] disabled:opacity-40"
          >
            {running ? "TRANSMITTING…" : "F · TRANSMIT"}
          </button>
        </div>
        <span className="text-[10px] tracking-[0.08em] text-ink-faint whitespace-nowrap overflow-hidden text-ellipsis">
          via <span className="text-sig-agent">{MODE_LABEL[settings?.transmitMode ?? "clipboard"]}</span> · :transmit-mode to change
        </span>
      </div>
    </HudFrame>
  );
}
