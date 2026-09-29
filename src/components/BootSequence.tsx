import { useEffect, useState } from "react";
import { useUI } from "../store/ui";
import { useRepoCurrent, useReview } from "../hooks/useRepo";
import type { ReviewModel } from "../lib/types";

type Kind = "dim" | "ok" | "ctx" | "agent" | "grant";

const DELAYS = [120, 420, 720, 1020, 1320, 1640, 2040];
const DONE_AT = 2640;
const WIDTH = 38;

function row(label: string, value: string) {
  return `> ${label} ${".".repeat(Math.max(3, WIDTH - label.length))} ${value}`;
}

/** Ported from git-ui, with DEBRIEF lines filled from the real model as it
 * arrives ("…" until then). */
function lines(repoName: string | null | undefined, m: ReviewModel | undefined): { text: string; kind: Kind }[] {
  const wait = "…";
  const noRepo = repoName === null;
  const flagged = m ? Object.values(m.files).filter((f) => f.flags.length > 0).length : 0;
  return [
    { text: "DEADBOLT // AGENT DEBRIEF", kind: "dim" },
    { text: row("linking repository", noRepo ? "NONE" : (repoName ?? wait).toUpperCase()), kind: noRepo ? "dim" : "ok" },
    { text: row("reading worktree", noRepo ? "SKIPPED" : m ? `${m.status.files.length} FILES` : wait), kind: "ctx" },
    {
      text: row("locating claude session", noRepo ? "SKIPPED" : m ? (m.session ? "FOUND" : "NONE") : wait),
      kind: m?.session ? "agent" : "ctx",
    },
    { text: row("parsing ledger", noRepo ? "SKIPPED" : m ? `${m.turns.length} TURNS` : wait), kind: m?.session ? "agent" : "ctx" },
    { text: row("scanning for flags", noRepo ? "SKIPPED" : m ? (flagged ? `${flagged} FLAGGED` : "CLEAR") : wait), kind: flagged ? "ctx" : "ok" },
    { text: noRepo ? ">> NO REPOSITORY LINKED" : ">> DEBRIEF READY", kind: "grant" },
  ];
}

const KIND_CLASS: Record<Kind, string> = {
  dim: "text-ink-dimmer",
  ok: "text-sig-ok",
  ctx: "text-ink-dim",
  agent: "text-sig-agent",
  grant: "mt-[6px] font-bold tracking-[0.1em] text-hud [text-shadow:0_0_14px_color-mix(in_srgb,var(--ac)_50%,transparent)]",
};

export function BootSequence() {
  const booting = useUI((s) => s.booting);
  const finish = useUI((s) => s.finishBoot);
  const { data: repo } = useRepoCurrent();
  const { data: model } = useReview();
  const [shown, setShown] = useState(0);

  useEffect(() => {
    if (!booting) return;
    const timers = DELAYS.map((d, i) => setTimeout(() => setShown(i + 1), d));
    const end = setTimeout(finish, DONE_AT);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " " || e.key === "Escape") {
        e.preventDefault();
        finish();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      timers.forEach(clearTimeout);
      clearTimeout(end);
      window.removeEventListener("keydown", onKey);
    };
  }, [booting, finish]);

  if (!booting) return null;

  const all = lines(repo === undefined ? undefined : repo?.name ?? null, model);
  const progress = Math.round((shown / all.length) * 100) + "%";

  return (
    <div onClick={finish} className="absolute inset-0 z-[120] flex flex-col items-center justify-center cursor-pointer bg-bg-base">
      <div className="dc-scanlines" />
      <div className="dc-vignette" />
      <div className="w-[560px] max-w-[86%] relative">
        <div className="font-chrome font-bold tracking-[0.42em] text-[38px] text-center mb-[6px] text-hud [text-shadow:0_0_34px_color-mix(in_srgb,var(--ac)_65%,transparent)]">
          DEADBOLT
        </div>
        <div className="text-center text-[10px] tracking-[0.52em] mb-[30px] text-ink-dimmer">
          <span className="text-sig-agent">DEBRIEF</span> · AGENT CHANGE REVIEW · v0.1.0
        </div>
        <div className="min-h-[168px] text-[12.5px] leading-[2.05] whitespace-pre">
          {all.slice(0, shown).map((ln, i) => (
            <div key={i} className={KIND_CLASS[ln.kind]}>
              {ln.text}
            </div>
          ))}
          <span className="inline-block align-middle w-[9px] h-[15px] bg-hud shadow-[0_0_10px_var(--ac)] animate-blink" />
        </div>
        <div className="mt-5 h-[3px] bg-hud/[.14]">
          <div className="h-full bg-hud shadow-[0_0_14px_var(--ac)] transition-[width] duration-200" style={{ width: progress }} />
        </div>
        <div className="text-center mt-[15px] text-[10px] tracking-[0.32em] text-ink-darkest">PRESS ⏎ OR CLICK TO SKIP</div>
      </div>
    </div>
  );
}
