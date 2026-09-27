import type { FileStatus } from "../lib/types";

const CHIP: Record<FileStatus, { ch: string; cls: string }> = {
  added: { ch: "A", cls: "bg-sig-ok/[.16] text-sig-ok" },
  modified: { ch: "M", cls: "bg-sig-warn/[.16] text-sig-warn" },
  deleted: { ch: "D", cls: "bg-sig-danger/[.16] text-sig-deleteHi" },
  renamed: { ch: "R", cls: "bg-hud/[.16] text-hud" },
  typechange: { ch: "T", cls: "bg-ink-dim/[.16] text-ink-dim" },
  untracked: { ch: "?", cls: "border border-ink-dimmer text-ink-dim" },
  conflicted: { ch: "U", cls: "bg-sig-danger/[.16] text-sig-danger" },
};

/** The 16px square A/M/D/R/? chip from the file rows. `inline` is the header variant. */
export function StatusChip({ status, inline = false }: { status: FileStatus; inline?: boolean }) {
  const c = CHIP[status];
  return (
    <span
      title={status}
      className={[
        "flex-none font-bold",
        inline
          ? "text-[10.5px] px-[5px]"
          : "w-4 h-4 flex items-center justify-center text-[10px]",
        c.cls,
      ].join(" ")}
    >
      {c.ch}
    </span>
  );
}
