import { HudFrame } from "../components/HudFrame";
import { useQueryClient } from "@tanstack/react-query";
import { useReview } from "../hooks/useRepo";
import { runAction } from "../hooks/useKeybindings";
import { progress, revertTargets } from "../lib/review";

// Noise files count in the header totals but not in progress (SPEC §3.4).
export function OpsPanel() {
  const { data } = useReview();
  const qc = useQueryClient();
  const staged = Object.values(data?.files ?? {}).filter((m) => m.viewed).length;
  // Everything `d` would discard, noise included.
  const toDiscard = revertTargets(data).reduce((n, t) => n + t.ids.length, 0);
  const p = progress(data);

  return (
    <HudFrame title="Ops console" className="flex-none" badge={<span className="text-[10.5px] text-hud">{p.total ? `${p.pct}%` : "—"}</span>}>
      <div className="flex flex-col gap-3 px-[13px] py-[14px]">
        <div className="flex justify-between text-[10.5px] tracking-[0.14em] text-ink-dim">
          <span>FILES CLEARED</span>
          <span className="text-ink-light">
            {p.viewed} / {p.total}
          </span>
        </div>
        <div className="h-2 p-px bg-bg-deep border border-hud/20">
          <div className="h-1 bg-hud transition-[width] duration-300 shadow-[0_0_10px_var(--ac)]" style={{ width: `${p.pct}%` }} />
        </div>
        {p.total > 0 && p.viewed === p.total && (
          <div className="px-[10px] py-[7px] border border-hud/40 bg-hud/[.08] text-[10.5px] tracking-[0.16em] text-hud">
            ALL CLEAR · {p.pending ? `${p.pending} HUNK${p.pending === 1 ? "" : "S"} UNDECIDED · ` : ""}A TO STAGE
          </div>
        )}
        <div className="grid grid-cols-3 gap-[6px]">
          <Counter value={p.kept} label="KEPT" cls="text-sig-ok" />
          <Counter value={p.reverted} label="REVERT" cls="text-sig-danger" />
          <Counter value={p.pending} label="OPEN" cls="text-ink-light" />
        </div>
        <div className="grid grid-cols-2 gap-[6px]">
          <button
            type="button"
            disabled={staged === 0}
            onClick={() => void runAction("ops.stage", qc)}
            title="Stage every cleared file, minus hunks marked revert"
            className="dc-hov h-[38px] px-[10px] text-left border border-hud/25 bg-transparent text-ink-light text-[10.5px] tracking-[0.14em] disabled:opacity-40"
          >
            <span className="text-hud font-bold">A</span> STAGE {staged}
          </button>
          <button
            type="button"
            disabled={toDiscard === 0}
            onClick={() => void runAction("ops.discard", qc)}
            title="Reverse-apply hunks marked revert (asks first)"
            className="dc-hov h-[38px] px-[10px] text-left border border-sig-danger/40 bg-transparent text-sig-dangerInk text-[10.5px] tracking-[0.14em] disabled:opacity-40"
          >
            <span className="font-bold">D</span> DISCARD {toDiscard}
          </button>
        </div>
      </div>
    </HudFrame>
  );
}

function Counter({ value, label, cls }: { value: number; label: string; cls: string }) {
  return (
    <div className="flex flex-col gap-[2px] p-2 border border-hud/[.12]">
      <span className={`font-chrome text-[20px] font-semibold ${cls}`}>{value}</span>
      <span className="text-[9.5px] tracking-[0.16em] text-ink-dim">{label}</span>
    </div>
  );
}
