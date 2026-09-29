import { useEffect, useRef } from "react";
import { useUI } from "../store/ui";

/** Streams a resume-mode run (SPEC §6). Esc closes it once the run ends. */
export function OutputDrawer() {
  const t = useUI((s) => s.transmit);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [t?.lines.length]);

  if (!t) return null;
  const state = t.running
    ? "RUNNING · ^C TO STOP"
    : `EXIT ${t.code ?? "KILLED"} · ${t.sent ? "NOTES SENT" : "NOTES KEPT IN QUEUE"} · ESC TO CLOSE`;

  return (
    <div className="relative z-[20] flex-none max-h-[38vh] flex flex-col mx-[9px] mb-[9px] bg-bg-panel border border-sig-agent/50 shadow-[0_0_30px_theme(colors.sig.agent/18%)]">
      <header className="flex-none flex items-center justify-between px-[13px] py-2 border-b border-sig-agent/25">
        <span className="font-chrome font-semibold tracking-[0.2em] text-[11px] text-sig-agent">▣ CLAUDE › RESUME</span>
        <span
          className={[
            "text-[10px] tracking-[0.16em]",
            t.running ? "text-sig-agent animate-pulse" : t.sent ? "text-sig-ok" : "text-sig-warn",
          ].join(" ")}
        >
          {state}
        </span>
      </header>
      <div className="flex-1 min-h-0 overflow-y-auto px-[13px] py-2 text-[11.5px] leading-[1.6]">
        {t.lines.length === 0 && <div className="text-ink-faint">waiting for output…</div>}
        {t.lines.map((l, i) => (
          <div key={i} className={["whitespace-pre-wrap break-words", l.stream === "stderr" ? "text-sig-delete" : "text-ink-light"].join(" ")}>
            {l.line}
          </div>
        ))}
        <div ref={endRef} />
      </div>
    </div>
  );
}
