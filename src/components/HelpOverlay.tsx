import { useUI } from "../store/ui";
import { BINDINGS, keyLabel, type HelpGroup } from "../lib/keymap";

const ORDER: HelpGroup[] = ["NAVIGATION", "REVIEW", "NOTES", "DIFF", "HISTORY", "FILTER", "SESSION", "CONSOLE", "LOOK", "GLOBAL"];

/** One row per (group, desc), listing every key bound to it. */
function rows(group: HelpGroup) {
  const out: { label: string; keys: string[] }[] = [];
  for (const b of BINDINGS) {
    if (b.group !== group) continue;
    const row = out.find((r) => r.label === b.desc);
    const k = b.key ? keyLabel(b.key) : ":" + b.cmd;
    if (!row) out.push({ label: b.desc, keys: [k] });
    else if (!row.keys.includes(k)) row.keys.push(k);
  }
  return out;
}

const GROUPS = ORDER.map((title) => ({ title, rows: rows(title) }));

export function HelpOverlay() {
  const open = useUI((s) => s.helpOpen);
  const toggle = useUI((s) => s.toggleHelp);
  if (!open) return null;

  return (
    <div
      onClick={toggle}
      className="absolute inset-0 z-[92] flex items-center justify-center p-6 bg-ink-void/[.82]"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-[900px] max-w-[94%] max-h-[86vh] overflow-y-auto bg-bg-panel border border-hud shadow-[0_0_70px_color-mix(in_srgb,var(--ac)_22%,transparent)]"
      >
        <header className="flex justify-between items-center px-[22px] py-4 border-b border-hud/20">
          <span className="font-chrome font-bold tracking-[0.28em] text-[15px] text-hud [text-shadow:0_0_14px_color-mix(in_srgb,var(--ac)_55%,transparent)]">
            KEYBINDINGS // OPERATOR MANUAL
          </span>
          <span className="text-[10px] tracking-[0.2em] text-ink-dimmer">ESC TO CLOSE</span>
        </header>
        <div className="grid grid-cols-2 gap-x-[34px] gap-y-[26px] p-[22px]">
          {GROUPS.map((g) => (
            <div key={g.title}>
              <div className="font-chrome font-semibold tracking-[0.2em] text-[11px] text-ink-dim mb-[11px]">
                {g.title}
              </div>
              <div className="flex flex-col gap-2 text-[12px]">
                {g.rows.map((r) => (
                  <div key={r.label} className="flex justify-between gap-4">
                    <span className="text-ink-mid">{r.label}</span>
                    <span className="text-hud whitespace-nowrap">{r.keys.join(" / ")}</span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
