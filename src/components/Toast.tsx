import { useEffect } from "react";
import { useUI } from "../store/ui";

export function Toast() {
  const toast = useUI((s) => s.toast);
  const clear = useUI((s) => s.clearToast);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(clear, 2600);
    return () => clearTimeout(t);
  }, [toast, clear]);

  if (!toast) return null;
  const ok = toast.kind === "ok";

  return (
    <div
      role="status"
      className={[
        "absolute right-[18px] top-16 z-[95] flex items-center gap-[10px] max-w-[340px] px-4 py-[11px] text-[12px] bg-bg-panel text-ink-light border",
        ok
          ? "border-hud shadow-[0_0_34px_color-mix(in_srgb,var(--ac)_26%,transparent)]"
          : "border-sig-danger shadow-[0_0_34px_theme(colors.sig.danger/26%)]",
      ].join(" ")}
    >
      <span
        className={[
          "w-[7px] h-[7px] rounded-full flex-none",
          ok ? "bg-hud shadow-[0_0_10px_var(--ac)]" : "bg-sig-danger shadow-[0_0_10px_theme(colors.sig.danger)]",
        ].join(" ")}
      />
      {toast.text}
    </div>
  );
}
