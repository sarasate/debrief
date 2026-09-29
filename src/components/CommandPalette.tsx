import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useUI } from "../store/ui";
import { BINDINGS, keyLabel } from "../lib/keymap";
import { errText } from "../lib/invoke";
import { runAction } from "../hooks/useKeybindings";
import { useReview, useSessions } from "../hooks/useRepo";
import { agoLabel } from "../hooks/useNow";

interface Item {
  key: string;
  label: string;
  detail: string;
  mark?: string;
  run: () => Promise<void> | void;
}

const COMMANDS = BINDINGS.filter((b) => b.cmd).map((b) => ({ cmd: b.cmd!, desc: b.desc, action: b.action, key: b.key }));

export function CommandPalette() {
  const mode = useUI((s) => s.palette);
  const close = () => useUI.getState().openPalette(null);
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const { data: sessions, isLoading } = useSessions(mode === "sessions");
  const { data: current } = useReview((m) => m.session?.id ?? null);
  const pinned = useUI((s) => s.sessionId);

  useEffect(() => {
    if (!mode) return;
    setQ("");
    setSel(0);
    setTimeout(() => inputRef.current?.focus(), 0);
  }, [mode]);

  const items = useMemo<Item[]>(() => {
    const needle = q.trim().toLowerCase();
    if (mode === "commands") {
      return COMMANDS.filter((c) => !needle || c.cmd.includes(needle) || c.desc.toLowerCase().includes(needle)).map(
        (c) => ({
          key: c.action,
          label: ":" + c.cmd,
          detail: c.key ? `${c.desc.toLowerCase()} · ${keyLabel(c.key)}` : c.desc.toLowerCase(),
          run: async () => {
            // `:session` swaps the palette over instead of closing it.
            if (c.action !== "session.pick") close();
            await runAction(c.action, qc);
          },
        }),
      );
    }
    const now = Date.now();
    const list: Item[] = (sessions ?? []).map((s) => ({
      key: s.id,
      label: s.title,
      detail: `${s.turns} turn${s.turns === 1 ? "" : "s"} · ${s.edits} edit${s.edits === 1 ? "" : "s"} · ${agoLabel(s.updatedAt, now).toLowerCase()} · ${s.id.slice(0, 8)}`,
      mark: s.id === current ? (pinned ? "PINNED" : "LIVE") : undefined,
      run: () => {
        close();
        useUI.getState().setSession(s.id);
        useUI.getState().setOutput("session pinned · " + s.title);
      },
    }));
    const auto: Item = {
      key: "auto",
      label: "Follow newest session",
      detail: "switches by itself when a new Claude session starts",
      mark: pinned ? undefined : "ON",
      run: () => {
        close();
        useUI.getState().setSession(null);
        useUI.getState().setOutput("following the newest session");
      },
    };
    return [auto, ...list].filter(
      (i) => !needle || i.label.toLowerCase().includes(needle) || i.detail.toLowerCase().includes(needle),
    );
  }, [mode, q, sessions, current, pinned, qc]);

  if (!mode) return null;

  async function exec(i: Item) {
    try {
      await i.run();
    } catch (e) {
      useUI.getState().emitToast("err", errText(e));
    }
  }

  return (
    <div onClick={close} className="absolute inset-0 z-[90] flex items-start justify-center pt-[13vh] bg-ink-void/[.74]">
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-[680px] max-w-[90%] bg-bg-panel border border-hud shadow-[0_0_60px_color-mix(in_srgb,var(--ac)_24%,transparent)]"
      >
        <div className="flex items-center gap-3 px-[18px] py-[15px] border-b border-hud/20">
          <span className="text-[20px] text-hud [text-shadow:0_0_12px_color-mix(in_srgb,var(--ac)_60%,transparent)]">
            {mode === "sessions" ? "⌁" : ":"}
          </span>
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") { e.preventDefault(); close(); }
              if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(items.length - 1, s + 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(0, s - 1)); }
              if (e.key === "Enter" && items[sel]) { e.preventDefault(); void exec(items[sel]); }
            }}
            placeholder={mode === "sessions" ? "filter sessions by title…" : "session · tree · refresh · open · help …"}
            className="flex-1 bg-transparent border-none outline-none text-[15px] text-ink-bright placeholder:text-ink-dimmer"
          />
          {mode === "sessions" && (
            <span className="text-[10px] tracking-[0.2em] text-sig-agent whitespace-nowrap">CLAUDE SESSIONS</span>
          )}
        </div>
        <ul className="max-h-[48vh] overflow-y-auto p-[6px]">
          {items.map((it, i) => (
            <li
              key={it.key}
              onClick={() => void exec(it)}
              onMouseMove={() => setSel(i)}
              className="relative cursor-pointer flex items-center gap-4 px-[13px] py-[10px]"
            >
              {i === sel && <div className="absolute inset-0 pointer-events-none bg-hud/[.11] border-l-2 border-hud" />}
              <span className="relative z-[1] flex-1 min-w-0 flex flex-col gap-[2px]">
                <span className="text-[13px] text-ink-light whitespace-nowrap overflow-hidden text-ellipsis">{it.label}</span>
                <span className="text-[11px] text-ink-dimmer whitespace-nowrap overflow-hidden text-ellipsis">{it.detail}</span>
              </span>
              {it.mark && (
                <span className="relative z-[1] px-[6px] py-px text-[10px] tracking-[0.16em] border border-sig-agent/50 text-sig-agent">
                  {it.mark}
                </span>
              )}
            </li>
          ))}
          {mode === "sessions" && isLoading && <li className="px-3 py-2 text-ink-dimmer text-xs">scanning transcripts…</li>}
          {mode === "sessions" && !isLoading && !sessions?.length && (
            <li className="px-3 py-2 text-ink-dimmer text-xs">no Claude Code sessions found for this repo</li>
          )}
          {items.length === 0 && mode === "commands" && <li className="px-3 py-2 text-ink-dimmer italic text-xs">no matches</li>}
        </ul>
      </div>
    </div>
  );
}
