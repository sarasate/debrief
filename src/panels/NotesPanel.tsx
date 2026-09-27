import { HudFrame } from "../components/HudFrame";
import { splitPath } from "../lib/changeset";
import { useUI } from "../store/ui";

// Notes and transmit arrive in M6; the composer is laid out but inert.
export function NotesPanel() {
  const path = useUI((s) => s.selectedPath);
  const name = path ? splitPath(path).name : "—";

  return (
    <HudFrame
      title="Field notes"
      className="flex-1"
      badge={<span className="px-[6px] py-px border border-sig-agent/50 text-sig-agent">→ CLAUDE · 0</span>}
    >
      <div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 px-3 py-[10px]">
        <div className="px-[10px] py-[14px] border border-dashed border-hud/20 text-[11px] leading-[1.6] text-ink-faint">
          No notes queued. Notes are sent to the Claude session as one batch.
        </div>
      </div>
      <div className="flex-none flex flex-col gap-2 px-3 pt-[10px] pb-3 border-t border-hud/10">
        <label htmlFor="note" className="text-[10px] tracking-[0.16em] text-ink-dim">
          NOTE ON <span className="text-hud">{name}</span>
        </label>
        <textarea
          id="note"
          rows={3}
          disabled
          placeholder="> move the realm check into the middleware…"
          className="resize-none border border-hud/[.18] bg-bg-deep text-ink-light px-[10px] py-2 text-[11.5px] leading-[1.55] outline-none placeholder:text-ink-dimmer disabled:opacity-60"
        />
        <div className="grid grid-cols-2 gap-[6px]">
          <button
            type="button"
            disabled
            className="h-9 border border-hud/25 bg-transparent text-ink-light text-[10.5px] tracking-[0.14em] disabled:opacity-40"
          >
            <span className="text-hud font-bold">N</span> QUEUE NOTE
          </button>
          <button
            type="button"
            disabled
            className="h-9 border-0 bg-sig-agent text-ink-agentVoid font-chrome font-bold text-[11px] tracking-[0.14em] shadow-[0_0_18px_theme(colors.sig.agent/40%)] disabled:opacity-40"
          >
            F · TRANSMIT
          </button>
        </div>
      </div>
    </HudFrame>
  );
}
