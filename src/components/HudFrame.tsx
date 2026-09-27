import type { ReactNode } from "react";
import { useUI } from "../store/ui";
import type { PanelId } from "../lib/keymap";

interface HudFrameProps {
  /** Panels with an id take focus on click and show the focus brackets. */
  id?: PanelId;
  title: string;
  badge?: ReactNode;
  /** Extra header content after the title (the diff header is a toolbar). */
  toolbar?: ReactNode;
  children: ReactNode;
  className?: string;
  bg?: string; // override panel bg (e.g. diff slightly darker)
}

export function HudFrame({
  id,
  title,
  badge,
  toolbar,
  children,
  className = "",
  bg = "bg-bg-panel",
}: HudFrameProps) {
  const focus = useUI((s) => s.focus);
  const errPanel = useUI((s) => s.errPanel);
  const setFocus = useUI((s) => s.setFocus);
  const active = !!id && focus === id;
  const erring = !!id && errPanel === id;

  return (
    <section
      onClick={() => id && setFocus(id)}
      className={[
        "relative flex flex-col min-h-0 min-w-0 border border-hud/[.13]",
        bg,
        erring ? "dc-shake" : "",
        className,
      ].join(" ")}
    >
      {active && (
        <div className="absolute inset-0 pointer-events-none z-[8] border border-hud shadow-[0_0_26px_color-mix(in_srgb,var(--ac)_18%,transparent),inset_0_0_26px_color-mix(in_srgb,var(--ac)_5%,transparent)]">
          <span className="dc-corner-outer tl" />
          <span className="dc-corner-outer tr" />
          <span className="dc-corner-outer bl" />
          <span className="dc-corner-outer br" />
        </div>
      )}
      <header
        className={[
          "flex items-center flex-none px-[13px] border-b border-hud/10",
          toolbar ? "gap-3 py-[9px]" : "justify-between py-[11px]",
        ].join(" ")}
      >
        <span className="font-chrome font-semibold tracking-[0.2em] text-[11.5px] text-ink-dim whitespace-nowrap">
          ▣ {title.toUpperCase()}
        </span>
        {toolbar}
        {badge && <span className="text-[10px]">{badge}</span>}
      </header>
      {children}
    </section>
  );
}
