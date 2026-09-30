import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ITheme, Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { HudFrame } from "../components/HudFrame";
import { QK, useRepoCurrent, useSettings } from "../hooks/useRepo";
import { api, errText } from "../lib/invoke";
import { useUI } from "../store/ui";

const MIN_HEIGHT = 15;
const MAX_HEIGHT = 85;

/** The xterm theme, read from the --term-* variables in hud.css. */
function readTheme(): ITheme {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(`--term-${name}`).trim();
  return {
    background: v("bg"),
    foreground: v("fg"),
    cursor: css.getPropertyValue("--ac").trim(),
    cursorAccent: v("bg"),
    selectionBackground: v("selection"),
    black: v("black"),
    red: v("red"),
    green: v("green"),
    yellow: v("yellow"),
    blue: v("blue"),
    magenta: v("magenta"),
    cyan: v("cyan"),
    white: v("white"),
    brightBlack: v("bright-black"),
    brightRed: v("bright-red"),
    brightGreen: v("bright-green"),
    brightYellow: v("bright-yellow"),
    brightBlue: v("bright-blue"),
    brightMagenta: v("bright-magenta"),
    brightCyan: v("bright-cyan"),
    brightWhite: v("bright-white"),
  };
}

/**
 * The console drawer (docs/PLAN.md M11). The shell starts on first open and
 * keeps running while hidden, so a command carries on and the scrollback
 * survives. It only ever receives what the user types or pastes.
 */
export function ConsolePanel() {
  const qc = useQueryClient();
  const open = useUI((s) => s.consoleOpen);
  const focused = useUI((s) => s.focus === "console");
  const restart = useUI((s) => s.consoleRestart);
  const { data: repo } = useRepoCurrent();
  const { data: settings } = useSettings();
  const [height, setHeight] = useState<number | null>(null);
  const [exited, setExited] = useState<number | null | undefined>(undefined);
  const hostRef = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const idRef = useRef<number | null>(null);
  const startedFor = useRef<string | null>(null);

  const shownHeight = height ?? settings?.consoleHeight ?? 40;

  /** Create the terminal widget once, on first open. xterm.js loads only
   * then, so it stays out of the startup bundle. */
  async function ensureTerminal(): Promise<Terminal | null> {
    if (term.current || !hostRef.current) return term.current;
    const [{ Terminal }, { FitAddon }] = await Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit")]);
    if (term.current || !hostRef.current) return term.current;
    const t = new Terminal({
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: 12,
      lineHeight: 1.25,
      cursorBlink: true,
      macOptionIsMeta: true,
      scrollback: 5000,
      allowTransparency: true,
      theme: readTheme(),
    });
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(hostRef.current);
    // macOS keys, as in Terminal.app: ⌘ is the app's, ctrl is the shell's.
    t.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || !e.metaKey) return true;
      const k = e.key.toLowerCase();
      if (k === "c") {
        if (t.hasSelection()) void navigator.clipboard.writeText(t.getSelection());
        return false; // never an interrupt
      }
      if (k === "k") {
        t.clear();
        return false;
      }
      if (k === "a") {
        t.selectAll();
        return false;
      }
      // ⌘V pastes through the Edit menu's paste event (bracketed when the
      // shell asks for it); ⌘J and friends bubble to the app's handler.
      return k === "v";
    });
    t.onData((data) => {
      const id = idRef.current;
      if (id != null) void api.consoleWrite(id, data).catch(() => undefined);
    });
    t.onResize(({ cols, rows }) => {
      const id = idRef.current;
      if (id != null) void api.consoleResize(id, cols, rows).catch(() => undefined);
    });
    term.current = t;
    fit.current = f;
    // xterm.js loads lazily, so the focus effect may have run before this
    // terminal existed: take the keyboard now if the console should have it.
    const ui = useUI.getState();
    if (ui.consoleOpen && ui.focus === "console") t.focus();
    return t;
  }

  /** Start a shell in the current repo, replacing any previous one. */
  async function startShell() {
    const t = await ensureTerminal();
    if (!t || !repo) return;
    if (idRef.current != null) await api.consoleClose(idRef.current).catch(() => undefined);
    idRef.current = null;
    t.reset();
    setExited(undefined);
    fit.current?.fit();
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (buf) => t.write(new Uint8Array(buf));
    try {
      idRef.current = await api.consoleOpen(t.cols, t.rows, channel);
      startedFor.current = repo.workdir;
    } catch (e) {
      t.write(`\r\n\x1b[31mconsole: ${errText(e)}\x1b[0m\r\n`);
    }
  }

  // Opening the console starts a shell unless one is already there for
  // this repo. Opening another repo closes the old shell in the backend, so
  // the next open starts fresh. An exited shell waits for ⌘⇧J.
  useEffect(() => {
    if (!open || !repo) return;
    if (startedFor.current !== repo.workdir) void startShell();
    requestAnimationFrame(() => fit.current?.fit());
  }, [open, repo?.workdir]);

  // ⌘⇧J (the store bumps a counter).
  useEffect(() => {
    if (restart > 0) void startShell();
  }, [restart]);

  // Focus follows the store: the console takes the keyboard when focused.
  useEffect(() => {
    if (open && focused) term.current?.focus();
    if (!focused) term.current?.blur();
  }, [open, focused]);

  // Accent changes recolour the cursor and selection.
  useEffect(() => {
    if (term.current) term.current.options.theme = readTheme();
  }, [settings?.accent]);

  // Refit when the drawer or window changes size.
  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    let frame = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => open && fit.current?.fit());
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [open]);

  // The shell ended (`exit`, or killed).
  useEffect(() => {
    const unlisten = listen<{ id: number; code: number | null }>("console://exit", (e) => {
      if (e.payload.id !== idRef.current) return;
      idRef.current = null;
      setExited(e.payload.code);
      term.current?.write(
        `\r\n\x1b[2m[shell exited${e.payload.code != null ? ` with ${e.payload.code}` : ""} · ⌘⇧J to restart]\x1b[0m\r\n`,
      );
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, []);

  function startDrag(e: React.MouseEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const start = shownHeight;
    let latest = start;
    const move = (ev: MouseEvent) => {
      const delta = ((startY - ev.clientY) / window.innerHeight) * 100;
      latest = Math.round(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, start + delta)));
      setHeight(latest);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      void api.settingsSet({ consoleHeight: latest }).then((s) => qc.setQueryData(QK.settings, s));
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  if (!repo) return null;
  return (
    <div
      data-console
      className="relative z-[6] flex-none flex flex-col mx-[9px] mb-[9px]"
      style={{ height: `${shownHeight}vh`, display: open ? "flex" : "none" }}
    >
      <div
        onMouseDown={startDrag}
        title="Drag to resize"
        className="absolute -top-[6px] left-0 right-0 h-[8px] cursor-row-resize z-[9]"
      />
      <HudFrame
        id="console"
        title="Console"
        className="flex-1"
        bg="bg-bg-deep"
        badge={
          <span className="text-ink-faint whitespace-nowrap">
            <span className="text-hud">{repo.name}</span>
            {exited !== undefined ? " · exited · ⌘⇧J restart" : " · ⌘J hide · ⌘K clear"}
          </span>
        }
      >
        <div
          ref={hostRef}
          onClick={() => useUI.getState().setFocus("console")}
          className="dc-console flex-1 min-h-0"
        />
      </HudFrame>
    </div>
  );
}
