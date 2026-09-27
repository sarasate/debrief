import { HudFrame } from "../components/HudFrame";
import { useReview } from "../hooks/useRepo";

// Review state (viewed, verdicts) lands in M4/M5. Until then nothing is
// cleared or decided, and every hunk counts as open. Noise files count in
// the header totals but not here (SPEC §3.4).
export function OpsPanel() {
  const { data } = useReview();
  const files = (data?.status.files ?? []).filter((f) => !data?.files[f.path]?.noise);
  const cleared = 0;
  const pct = files.length ? Math.round((cleared / files.length) * 100) : 0;
  const hunks = files.reduce((n, f) => n + f.hunks, 0);

  return (
    <HudFrame title="Ops console" className="flex-none" badge={<span className="text-[10.5px] text-hud">{pct}%</span>}>
      <div className="flex flex-col gap-3 px-[13px] py-[14px]">
        <div className="flex justify-between text-[10.5px] tracking-[0.14em] text-ink-dim">
          <span>FILES CLEARED</span>
          <span className="text-ink-light">
            {cleared} / {files.length}
          </span>
        </div>
        <div className="h-2 p-px bg-bg-deep border border-hud/20">
          <div className="h-1 bg-hud shadow-[0_0_10px_var(--ac)]" style={{ width: `${pct}%` }} />
        </div>
        <div className="grid grid-cols-3 gap-[6px]">
          <Counter value={0} label="KEPT" cls="text-sig-ok" />
          <Counter value={0} label="REVERT" cls="text-sig-danger" />
          <Counter value={hunks} label="OPEN" cls="text-ink-light" />
        </div>
        <div className="grid grid-cols-2 gap-[6px]">
          <button
            type="button"
            disabled
            className="h-[38px] px-[10px] text-left border border-hud/25 bg-transparent text-ink-light text-[10.5px] tracking-[0.14em] disabled:opacity-40"
          >
            <span className="text-hud font-bold">A</span> STAGE {cleared}
          </button>
          <button
            type="button"
            disabled
            className="h-[38px] px-[10px] text-left border border-sig-danger/40 bg-transparent text-sig-dangerInk text-[10.5px] tracking-[0.14em] disabled:opacity-40"
          >
            <span className="font-bold">D</span> DISCARD 0
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
