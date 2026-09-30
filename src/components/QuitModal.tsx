import { useEffect, useRef } from "react";
import { useUI } from "../store/ui";
import { api, errText } from "../lib/invoke";

/** Quitting with a command still running in the console asks first (M11). */
export function QuitModal() {
  const open = useUI((s) => s.modal === "quit");
  const close = () => useUI.getState().openModal(null);
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) setTimeout(() => boxRef.current?.focus(), 0);
  }, [open]);

  if (!open) return null;

  const quit = () => api.consoleQuit().catch((e) => useUI.getState().emitToast("err", errText(e)));

  return (
    <div onClick={close} className="absolute inset-0 z-[93] flex items-center justify-center p-6 bg-ink-void/[.82]">
      <div
        ref={boxRef}
        tabIndex={-1}
        role="alertdialog"
        aria-labelledby="quit-title"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // As in the discard modal: Enter on the dialog itself confirms,
          // Enter on a focused button is that button's click.
          if (e.key === "Enter" && e.target === e.currentTarget) {
            e.preventDefault();
            void quit();
          }
        }}
        className="w-[520px] max-w-[94%] flex flex-col bg-bg-panel border border-sig-warn outline-none shadow-[0_0_60px_theme(colors.sig.warn/20%)]"
      >
        <header className="flex justify-between items-center px-[22px] py-4 border-b border-sig-warn/30">
          <span id="quit-title" className="font-chrome font-bold tracking-[0.24em] text-[14px] text-sig-warn">
            A COMMAND IS STILL RUNNING
          </span>
          <span className="text-[10px] tracking-[0.2em] text-ink-dimmer">ESC TO STAY</span>
        </header>
        <div className="px-[22px] py-4 text-[12px] leading-[1.6] text-ink-base">
          Something is still running in the console. Quitting stops it, the same way closing a terminal window does.
        </div>
        <footer className="flex justify-end gap-[6px] px-[22px] py-4 border-t border-sig-warn/20">
          <button
            type="button"
            onClick={close}
            className="dc-hov h-9 px-4 border border-hud/25 bg-transparent text-ink-light text-[10.5px] tracking-[0.14em]"
          >
            ESC · STAY
          </button>
          <button
            type="button"
            onClick={() => void quit()}
            className="h-9 px-4 border-0 bg-sig-warn text-ink-void font-chrome font-bold text-[11px] tracking-[0.14em]"
          >
            ⏎ · QUIT AND STOP IT
          </button>
        </footer>
      </div>
    </div>
  );
}
