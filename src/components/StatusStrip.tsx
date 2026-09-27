import { useRepoCurrent, useReview } from "../hooks/useRepo";
import { agoLabel, useNow } from "../hooks/useNow";

export function StatusStrip() {
  const { data: repo } = useRepoCurrent();
  const { data: model } = useReview();
  const data = model?.status;
  const session = model?.session ?? null;
  // Claude's last edit when a session is linked, else the newest dirty file.
  const lastWrite = session?.lastEditAt ? Date.parse(session.lastEditAt) : data?.lastWrite ?? null;
  const now = useNow(5_000);
  const head = data?.head;
  const branch = !data
    ? "—"
    : head?.detached
    ? `(DETACHED ${head.sha.slice(0, 6)})`
    : head?.branch ?? "(NO HEAD)";
  const files = data?.files.length ?? 0;
  const dirty = files > 0;

  return (
    <>
      {/* STATUS STRIP */}
      <header className="relative z-10 flex-none flex items-stretch border-b border-hud/[.26]">
        <div className="flex items-center gap-[11px] px-5 py-[13px] bg-hud text-ink-void font-chrome font-bold tracking-[0.22em] text-[16px] shadow-[0_0_28px_color-mix(in_srgb,var(--ac)_45%,transparent)]">
          <span className="w-[11px] h-[11px] bg-ink-void" />
          DEADBOLT
        </div>
        <div className="flex items-center px-4 border-r border-hud/20 font-chrome font-semibold tracking-[0.3em] text-[12px] text-hud">
          DEBRIEF
        </div>
        <div className="flex-1 min-w-0 flex items-center gap-7 px-5 text-[11px] tracking-[0.13em] text-ink-label overflow-hidden">
          <span className="whitespace-nowrap">
            REPO <span className="text-ink-light uppercase">{repo?.name ?? "—"}</span>
          </span>
          <span className="whitespace-nowrap">
            BRANCH{" "}
            <span className="uppercase text-hud [text-shadow:0_0_10px_color-mix(in_srgb,var(--ac)_60%,transparent)]">
              {branch}
            </span>
          </span>
          <span className="whitespace-nowrap">
            HEAD <span className="text-ink-light">{head?.sha.slice(0, 6) || "——————"}</span>
          </span>
          <span className="whitespace-nowrap">
            AGENT <span className={session ? "text-sig-agent" : "text-ink-faint"}>{session ? "CLAUDE" : "NO SESSION"}</span> · LAST WRITE{" "}
            <span className="text-ink-light">{agoLabel(lastWrite, now)}</span>
          </span>
        </div>
        <div className="flex items-center flex-none">
          <span className="px-[18px] text-[11px] tracking-[0.12em] text-ink-label whitespace-nowrap">
            +<span className="text-sig-add">{data?.adds ?? 0}</span> −
            <span className="text-sig-delete">{data?.dels ?? 0}</span>
          </span>
          <span
            className={[
              "px-[18px] h-full flex items-center gap-2 border-l border-hud/20 text-[11px] tracking-[0.12em] whitespace-nowrap",
              dirty ? "text-sig-warn" : "text-hud",
            ].join(" ")}
          >
            <span
              className={[
                "w-[7px] h-[7px] rounded-full animate-pulse",
                dirty
                  ? "bg-sig-warn shadow-[0_0_10px_theme(colors.sig.warn)]"
                  : "bg-hud shadow-[0_0_10px_var(--ac)]",
              ].join(" ")}
            />
            {!data ? "NO LINK" : dirty ? "WORKTREE DIRTY" : "WORKTREE CLEAN"}
          </span>
        </div>
      </header>

      {/* CLASSIFIED TAPE */}
      <div className="relative z-10 flex-none px-5 py-1 border-b border-hud/10 bg-hud/[.03] text-[9.5px] tracking-[0.34em] text-[color:color-mix(in_srgb,var(--ac)_34%,theme(colors.ink.deepest))] whitespace-nowrap overflow-hidden">
        // AGENT DEBRIEF // UNCOMMITTED CHANGES // SESSION: {session ? session.title.toUpperCase() : "NONE LINKED"} //{" "}
        {files} FILES TOUCHED // REVIEW BEFORE COMMIT // EYES ONLY //
      </div>
    </>
  );
}
