import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import { StatusStrip } from "./components/StatusStrip";
import { CommandBar } from "./components/CommandBar";
import { HelpOverlay } from "./components/HelpOverlay";
import { CommandPalette } from "./components/CommandPalette";
import { DiscardModal } from "./components/DiscardModal";
import { OutputDrawer } from "./components/OutputDrawer";
import { BootSequence } from "./components/BootSequence";
import { QuitModal } from "./components/QuitModal";
import { ConsolePanel } from "./panels/ConsolePanel";
import { Toast } from "./components/Toast";
import { ChangesetPanel } from "./panels/ChangesetPanel";
import { DiffPanel } from "./panels/DiffPanel";
import { OpsPanel } from "./panels/OpsPanel";
import { NotesPanel } from "./panels/NotesPanel";
import { useKeybindings, runAction } from "./hooks/useKeybindings";
import { useChangeset, useRepoCurrent, useReview, useSettings, useStatus } from "./hooks/useRepo";
import { keyLabel } from "./lib/keymap";
import { applyTheme, resolveTheme, useSystemDark } from "./lib/theme";
import { useUI } from "./store/ui";

export default function App() {
  const qc = useQueryClient();
  const { data: repo, isLoading } = useRepoCurrent();
  useKeybindings();
  useSelectionSync();
  useInvalidationNotice();
  useTransmitEvents();
  useLook();
  // The resume drawer and the console share a slot; the console wins.
  const consoleOpen = useUI((s) => s.consoleOpen);

  // Quitting with a command running in the console asks first.
  useEffect(() => {
    const unlisten = listen("console://quit-requested", () => useUI.getState().openModal("quit"));
    return () => {
      unlisten.then((u) => u());
    };
  }, []);

  // Repo-watcher event
  useEffect(() => {
    const unlisten = listen("repo://changed", () => {
      void qc.invalidateQueries({ queryKey: ["repo"] });
      useUI.getState().setOutput("worktree changed · resynced");
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, [qc]);

  return (
    <div className="dc-root flex flex-col h-screen w-screen overflow-hidden text-[12px]">
      <div className="dc-scanlines" />
      <div className="dc-sweep" />
      <div className="dc-vignette" />

      <StatusStrip />

      <main className="relative z-[5] flex-1 min-h-0 flex gap-[9px] p-[9px]">
        {repo ? (
          <>
            <ChangesetPanel />
            <DiffPanel />
            <div className="w-[330px] flex-none flex flex-col gap-[9px] min-h-0">
              <OpsPanel />
              <NotesPanel />
            </div>
          </>
        ) : (
          !isLoading && <NoRepo onOpen={() => void runAction("repo.open", qc)} />
        )}
      </main>

      <ConsolePanel />
      {!consoleOpen && <OutputDrawer />}
      <CommandBar />
      <CommandPalette />
      <DiscardModal />
      <QuitModal />
      <HelpOverlay />
      <Toast />
      <BootSequence />
    </div>
  );
}

/** Keep a file selected: the first one on open, the first one again if the selected file stops being dirty. */
function useSelectionSync() {
  const { data } = useStatus();
  const { order } = useChangeset();
  const selected = useUI((s) => s.selectedPath);
  useEffect(() => {
    if (!data) return;
    const exists = !!selected && data.files.some((f) => f.path === selected);
    if (!exists) useUI.getState().select(order[0] ?? null);
  }, [data, order, selected]);
}

/** Theme, accent and scanlines from settings, as attributes hud.css and
 * themes.css key on. Until settings load, the pre-paint script's guess in
 * index.html stands. */
function useLook() {
  const { data } = useSettings();
  const systemDark = useSystemDark();
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.accent = data?.accent ?? "cyan";
    root.dataset.scanlines = data?.scanlines === false ? "off" : "on";
  }, [data?.accent, data?.scanlines]);
  useEffect(() => {
    if (!data) return;
    applyTheme(resolveTheme(data.themeMode, data.darkTheme, systemDark), data.themeMode, data.darkTheme);
  }, [data?.themeMode, data?.darkTheme, systemDark]);
}

const MAX_DRAWER_LINES = 2000;

/** Resume-mode output and the review refresh that follows it (SPEC §6, §7). */
function useTransmitEvents() {
  const qc = useQueryClient();
  useEffect(() => {
    const subs = [
      listen("review://updated", () => void qc.invalidateQueries({ queryKey: ["repo", "review"] })),
      listen<{ stream: string; line: string }>("transmit://output", (e) => {
        const ui = useUI.getState();
        const t = ui.transmit ?? { running: true, lines: [], code: null, sent: false };
        ui.setTransmit({ ...t, lines: [...t.lines, e.payload].slice(-MAX_DRAWER_LINES) });
        if (e.payload.stream === "stdout" && e.payload.line.trim()) ui.setOutput("claude › " + e.payload.line);
      }),
      listen<{ code: number | null; sent: boolean }>("transmit://done", (e) => {
        const ui = useUI.getState();
        const t = ui.transmit ?? { running: false, lines: [], code: null, sent: false };
        ui.setTransmit({ ...t, running: false, code: e.payload.code, sent: e.payload.sent });
        if (e.payload.sent) {
          ui.setOutput("transmitted notes to claude · awaiting next turn");
          ui.emitToast("ok", "claude finished the resumed turn");
        } else {
          ui.setOutput(`resume ended (exit ${e.payload.code ?? "killed"}) · notes stay queued`);
          ui.emitToast("err", "resume failed · notes stay queued");
        }
      }),
    ];
    return () => subs.forEach((p) => p.then((u) => u()));
  }, [qc]);
}

/** "auth.guard.ts changed since cleared" when a refresh drops a viewed mark (SPEC §4). */
function useInvalidationNotice() {
  const { data } = useReview((m) => m.invalidated);
  useEffect(() => {
    if (!data?.length) return;
    const name = (p: string) => p.split("/").pop();
    useUI.getState().setOutput(
      data.length === 1 ? `${name(data[0])} changed since cleared` : `${data.length} files changed since cleared · back to open`,
    );
  }, [data]);
}

function NoRepo({ onOpen }: { onOpen: () => void }) {
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="relative w-[440px] flex flex-col gap-4 px-7 py-6 bg-bg-panel border border-hud/[.13]">
        <span className="dc-corner-outer tl" />
        <span className="dc-corner-outer br" />
        <span className="font-chrome font-semibold tracking-[0.2em] text-[11.5px] text-ink-dim">▣ NO REPOSITORY LINKED</span>
        <span className="text-[12px] leading-[1.6] text-ink-base">
          Open the working tree a Claude Code session left dirty. Debrief only reads it; nothing is written until you ask.
        </span>
        <button
          type="button"
          onClick={onOpen}
          className="self-start h-[34px] px-4 bg-hud text-ink-void font-chrome font-bold tracking-[0.14em] text-[11px] shadow-[0_0_18px_color-mix(in_srgb,var(--ac)_40%,transparent)]"
        >
          {keyLabel("⌘O")} · OPEN REPOSITORY
        </button>
      </div>
    </div>
  );
}
