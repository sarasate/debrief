import { useUI } from "../store/ui";
import { BINDINGS } from "../lib/keymap";

const HINTS = BINDINGS.flatMap((b) => (b.hint ? [b.hint] : []));

export function CommandBar() {
  const output = useUI((s) => s.output);

  return (
    <footer className="relative z-10 flex-none h-10 flex items-center gap-[14px] pl-[14px] pr-3 border-t border-hud/20 bg-bg-deep">
      <span className="flex-none whitespace-nowrap px-[10px] py-[3px] bg-hud/20 border border-hud text-hud font-chrome font-bold text-[11px] tracking-[0.14em]">
        CMD ›
      </span>
      <span title={output} className="min-w-0 text-[12px] text-ink-mid whitespace-nowrap overflow-hidden text-ellipsis">
        {output}
        <span className="text-hud animate-blink">_</span>
      </span>
      <span className="flex-1" />
      <span className="text-[10.5px] tracking-[0.08em] text-ink-faint whitespace-nowrap">
        {HINTS.map((h, i) => (
          <span key={h.keys}>
            {i > 0 && " · "}
            <span className="text-hud">{h.keys}</span> {h.label}
          </span>
        ))}
      </span>
    </footer>
  );
}
