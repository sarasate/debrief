import { useEffect } from "react";
import { useQueryClient, type InfiniteData, type QueryClient } from "@tanstack/react-query";
import { useUI } from "../store/ui";
import { findBinding, keyLabel } from "../lib/keymap";
import { buildChangeset, isOpen, splitPath } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { openRepoDialog } from "../lib/openRepo";
import { pageLines, rememberScroll, reveal, scrollPanelByLines, scrollPanelTo } from "../lib/scroll";
import { QK, diffKey, logKey, reviewKey } from "./useRepo";
import type { Accent, BranchLog, CommitDetail, DarkTheme, DiffHunk, FileDiff, ReviewModel, Settings, ThemeMode, Verdict } from "../lib/types";
import { logOrder, visibleLog } from "../lib/history";
import { committedReverts, currentModel, readOnlyReason, revertTargets, setVerdict, setViewed, stageCleared } from "../lib/review";
import { focusComposer, removeNote, setTransmitMode, transmit } from "../lib/notes";
import { api } from "../lib/invoke";
import { COLLAPSE_LINES } from "../lib/highlight";
import { resolveTheme, systemIsDark } from "../lib/theme";

export function useKeybindings() {
  const qc = useQueryClient();

  useEffect(() => {
    const handler = async (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      // ⌘J / ⌘⇧J toggle and restart the console from anywhere, inside it
      // and inside text fields included: shells never receive ⌘ combos.
      if (e.metaKey && !e.ctrlKey && !e.altKey && e.key.toLowerCase() === "j" && !useUI.getState().booting) {
        e.preventDefault();
        await runAction(e.shiftKey ? "console.restart" : "console.toggle", qc);
        return;
      }
      // Everything else typed into the console belongs to the shell,
      // Escape included (vim).
      if (t?.closest("[data-console]")) return;
      if (t && isTextEntry(t)) {
        if (e.key === "Escape") t.blur();
        return;
      }
      // A clicked button or checkbox keeps focus; let space and friends
      // reach the bindings instead of re-clicking it.
      if (t && t !== document.body && (t.tagName === "BUTTON" || t.tagName === "INPUT")) t.blur();

      const ui = useUI.getState();
      if (ui.booting) return; // BootSequence handles its own keys

      // Cmd/Ctrl-O — open repo
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "o" && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        await runAction("repo.open", qc);
        return;
      }
      if (ui.helpOpen && e.key !== "Escape" && e.key !== "?") return;
      if (ui.palette && e.key !== "Escape") return;
      if (ui.modal) {
        // The modal handles its own keys; Escape always backs out.
        if (e.key === "Escape") { e.preventDefault(); ui.openModal(null); }
        return;
      }

      // Ctrl-combos are opt-in: they only match bindings that name them
      // explicitly ("ctrl+d"), so every other browser shortcut stays intact.
      const ctrl = e.ctrlKey && !e.metaKey && !e.altKey;
      const key = ctrl ? "ctrl+" + e.key.toLowerCase() : e.key;

      const b = findBinding(ui.focus, key);
      if (!b) return;
      // Don't consume browser-critical combos
      if (e.metaKey || e.altKey) return;
      e.preventDefault();

      try {
        await runAction(b.action, qc);
      } catch (err) {
        ui.pulseError(ui.focus);
        ui.emitToast("err", errText(err));
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [qc]);
}

function isTextEntry(t: HTMLElement): boolean {
  if (t.isContentEditable || t.tagName === "TEXTAREA") return true;
  if (t.tagName !== "INPUT") return false;
  return !["checkbox", "radio", "button", "submit", "range"].includes((t as HTMLInputElement).type);
}

function currentView(qc: QueryClient) {
  const ui = useUI.getState();
  return buildChangeset(qc.getQueryData<ReviewModel>(reviewKey()), ui);
}

/**
 * Step through `order`, wrapping. A selection that isn't in `order` (filtered
 * out or folded away) steps to its sorted neighbour.
 */
function step(order: string[], cur: string | null, delta: 1 | -1): string | null {
  if (!order.length) return null;
  const i = cur ? order.indexOf(cur) : -1;
  if (i !== -1) return order[(i + delta + order.length) % order.length];
  if (!cur) return order[0];
  const after = order.findIndex((p) => p > cur);
  if (delta === 1) return order[after === -1 ? 0 : after];
  const before = (after === -1 ? order.length : after) - 1;
  return order[before < 0 ? order.length - 1 : before];
}

/** Scroll the diff to a new-side line number and select its hunk,
 * expanding a collapsed large hunk first. */
export function jumpToLine(line: number) {
  const hunks = [...document.querySelectorAll<HTMLElement>('[data-panel-scroll="diff"] [data-hunk]')];
  const holder = hunks.find((h) => Number(h.dataset.newStart) <= line && line <= Number(h.dataset.newEnd));
  if (holder && !holder.querySelector("[data-line]")) {
    useUI.getState().toggleExpanded(holder.dataset.hunk!, true);
    requestAnimationFrame(() => requestAnimationFrame(() => jumpToLine(line)));
    return;
  }
  const el = document.querySelector<HTMLElement>(`[data-panel-scroll="diff"] [data-new-line="${line}"]`);
  if (!el) { useUI.getState().setOutput(`line ${line} is outside the diff`); return; }
  const hunk = el.closest<HTMLElement>("[data-hunk]")?.dataset.hunk;
  if (hunk) useUI.getState().setHunk(hunk);
  el.scrollIntoView({ block: "center" });
  el.classList.add("dc-flash");
  setTimeout(() => el.classList.remove("dc-flash"), 900);
  useUI.getState().setOutput(`jumped to line ${line}`);
}

function selectFile(path: string | null) {
  if (!path) return;
  useUI.getState().select(path);
  requestAnimationFrame(() => reveal("changeset", `[data-path="${CSS.escape(path)}"]`));
}

/** Review actions that make no sense on a past commit. */
const REVIEW_ONLY = new Set(["file.clear", "file.viewed", "hunk.keep", "hunk.revert", "note.new", "note.newFile", "flag.jump"]);

/** Hunks on screen: the shown commit's, across its files, else the selected file's. */
function shownHunks(qc: QueryClient): { hunks: DiffHunk[]; cursor: string | null; set: (id: string) => void } {
  const ui = useUI.getState();
  if (ui.logSha) {
    const c = qc.getQueryData<CommitDetail>(QK.commit(ui.logSha));
    return { hunks: c?.files.flatMap((f) => f.hunks) ?? [], cursor: ui.logHunk, set: ui.setLogHunk };
  }
  const diff = qc.getQueryData<FileDiff>(diffKey(ui.selectedPath));
  return { hunks: diff?.hunks ?? [], cursor: ui.hunkId, set: (id) => ui.setHunk(id) };
}

function moveLogCursor(sha: string | null) {
  useUI.getState().setLogCursor(sha);
  requestAnimationFrame(() => reveal("history", `[data-log="${sha ?? "review"}"]`));
}

/** Show the commit under the log cursor, or go back to the review. */
function openLogCursor(qc: QueryClient) {
  const ui = useUI.getState();
  const sha = ui.logCursor;
  if (!sha) {
    if (ui.logSha) ui.showCommit(null);
    ui.setOutput("current review");
    return;
  }
  if (!ui.logSha) rememberScroll("diff");
  ui.showCommit(sha);
  const e = visibleLog(qc.getQueryData<InfiniteData<BranchLog>>(logKey()), "").find((x) => x.sha === sha);
  ui.setOutput(`commit ${e?.short ?? sha.slice(0, 7)} · ${keyLabel("Escape")} back to the review`);
}

/** Run a BINDINGS action. Buttons call this too, so mouse and keys share one path. */
export async function runAction(action: string, qc: QueryClient) {
  const ui = useUI.getState();

  if (ui.logSha && REVIEW_ONLY.has(action)) {
    ui.setOutput(`viewing a past commit · ${keyLabel("Escape")} back to the review`);
    return;
  }

  switch (action) {
    // files
    case "file.next": selectFile(step(currentView(qc).order, ui.selectedPath, 1)); return;
    case "file.prev": selectFile(step(currentView(qc).order, ui.selectedPath, -1)); return;
    case "file.first": selectFile(currentView(qc).order[0] ?? null); return;
    case "file.last": selectFile(currentView(qc).order.slice(-1)[0] ?? null); return;

    // review
    case "file.viewed": {
      const path = ui.selectedPath;
      const meta = path ? currentModel(qc)?.files[path] : undefined;
      if (!path || !meta) return;
      await setViewed(qc, path, !meta.viewed);
      ui.setOutput(splitPath(path).name + (meta.viewed ? " reopened" : " cleared"));
      return;
    }
    case "file.clear": {
      const path = ui.selectedPath;
      const model = currentModel(qc);
      if (!path || !model?.files[path]) return;
      // Pick the next open file before this one leaves an OPEN-filtered list.
      const order = currentView(qc).order;
      const at = order.indexOf(path);
      const rest = at === -1 ? order : [...order.slice(at + 1), ...order.slice(0, at)];
      const next = rest.find((p) => p !== path && isOpen(model, p));
      // The cache is patched synchronously; move on while the write lands.
      const saved = model.files[path].viewed ? Promise.resolve() : setViewed(qc, path, true);
      const name = splitPath(path).name;
      if (next) {
        selectFile(next);
        ui.setOutput(`${name} cleared · next ${splitPath(next).name}`);
      } else {
        ui.setOutput(`${name} cleared · no open files left`);
      }
      await saved;
      return;
    }
    case "hunk.keep":
    case "hunk.revert": {
      const v: Verdict = action === "hunk.keep" ? "keep" : "revert";
      const model = currentModel(qc);
      const ids = ui.selectedPath ? model?.files[ui.selectedPath]?.hunkIds ?? [] : [];
      const id = ui.hunkId && ids.includes(ui.hunkId) ? ui.hunkId : null;
      if (!model || !id) { ui.setOutput("no hunk under the cursor · ]/[ to pick one"); return; }
      const next = model.verdicts[id] === v ? null : v;
      await setVerdict(qc, id, next);
      const where = `hunk ${ids.indexOf(id) + 1}/${ids.length} of ${splitPath(ui.selectedPath!).name}`;
      const meta = model.files[ui.selectedPath!];
      const committed = !!model.range && !(meta?.uncommitted ?? []).includes(id);
      const revert = committed ? "revert requested (committed)" : "marked for discard";
      ui.setOutput(`${where} → ${next === "keep" ? "kept" : next === "revert" ? revert : "undecided"}`);
      return;
    }

    // notes
    case "note.new":
    case "note.newFile":
      if (!ui.selectedPath) { ui.setOutput("select a file to write a note on"); return; }
      focusComposer(action === "note.newFile" ? "file" : "auto");
      return;
    case "notes.transmit": await transmit(qc); return;
    case "notes.down":
    case "notes.up": {
      const n = currentModel(qc)?.notes.length ?? 0;
      if (!n) return;
      const d = action === "notes.down" ? 1 : -1;
      ui.setNotesCursor(Math.max(0, Math.min(n - 1, ui.notesCursor + d)));
      return;
    }
    case "notes.remove": {
      const note = currentModel(qc)?.notes[ui.notesCursor];
      if (!note) return;
      await removeNote(qc, note.id);
      ui.setNotesCursor(Math.max(0, ui.notesCursor - 1));
      return;
    }
    case "transmit.cancel":
      if (await api.transmitCancel()) ui.setOutput("resume stopped · notes stay queued");
      return;
    case "mode.clipboard": await setTransmitMode(qc, "clipboard"); return;
    case "mode.file": await setTransmitMode(qc, "file"); return;
    case "mode.resume": await setTransmitMode(qc, "resume"); return;
    case "accent.cyan":
    case "accent.green":
    case "accent.amber":
    case "accent.red": {
      const accent = action.split(".")[1] as Accent;
      qc.setQueryData(QK.settings, await api.settingsSet({ accent }));
      ui.setOutput("accent · " + accent);
      return;
    }
    case "theme.system":
    case "theme.light":
    case "theme.dark": {
      const themeMode = action.split(".")[1] as ThemeMode;
      const s = await api.settingsSet({ themeMode });
      qc.setQueryData(QK.settings, s);
      ui.setOutput(`theme · ${themeMode} (${resolveTheme(s.themeMode, s.darkTheme, systemIsDark())})`);
      return;
    }
    case "theme.deadbolt":
    case "theme.ember": {
      const darkTheme = action.split(".")[1] as DarkTheme;
      qc.setQueryData(QK.settings, await api.settingsSet({ themeMode: "dark", darkTheme }));
      ui.setOutput("theme · " + darkTheme);
      return;
    }
    case "scanlines.toggle": {
      const on = !(qc.getQueryData<Settings>(QK.settings)?.scanlines ?? true);
      qc.setQueryData(QK.settings, await api.settingsSet({ scanlines: on }));
      ui.setOutput("scanlines " + (on ? "on" : "off"));
      return;
    }

    // console
    case "console.toggle":
      ui.setConsoleOpen(!ui.consoleOpen);
      if (!useUI.getState().consoleOpen) (document.activeElement as HTMLElement | null)?.blur();
      return;
    case "console.restart": ui.restartConsole(); ui.setOutput("console · restarting shell"); return;
    case "terminal.open": {
      const app = await api.openInTerminal();
      ui.setOutput(`opened the repo in ${app}`);
      return;
    }

    case "ops.stage": {
      const why = readOnlyReason(currentModel(qc));
      if (why) { ui.setOutput(why); return; }
      await stageCleared(qc);
      return;
    }
    case "ops.discard": {
      const model = currentModel(qc);
      const why = readOnlyReason(model);
      if (why) { ui.setOutput(why); return; }
      if (!revertTargets(model).length) {
        const committed = committedReverts(model);
        const meta = ui.selectedPath ? model?.files[ui.selectedPath] : undefined;
        const cursorCommitted = !!model?.range && !!ui.hunkId && !(meta?.uncommitted ?? []).includes(ui.hunkId);
        ui.setOutput(
          cursorCommitted
            ? "the hunk under the cursor is committed · discard reaches only uncommitted hunks"
            : committed
            ? `${committed} committed hunk${committed === 1 ? "" : "s"} marked revert stay as requests · only uncommitted hunks can be discarded`
            : "no hunks marked revert · x to mark one",
        );
        return;
      }
      ui.openModal("discard");
      return;
    }

    // tree
    case "tree.fold": {
      if (ui.grouping !== "tree") { ui.setOutput("folders fold in file tree mode · t to switch"); return; }
      if (!ui.selectedPath) return;
      const dir = splitPath(ui.selectedPath).dirs.join("/");
      if (!dir) { ui.setOutput(ui.selectedPath + " sits at the repo root"); return; }
      ui.toggleFolder(dir);
      ui.setOutput((useUI.getState().collapsed[dir] ? "folded " : "unfolded ") + dir + "/");
      return;
    }
    case "tree.unfoldAll": ui.unfoldAll(); ui.setOutput("all folders unfolded"); return;

    // session and grouping
    case "grouping.toggle": {
      ui.toggleGrouping(!!currentModel(qc)?.range);
      const g = useUI.getState().grouping;
      ui.setOutput(g === "intent" ? "grouped by intent" : g === "commit" ? "grouped by commit" : "grouped by file tree");
      return;
    }
    case "palette.open": ui.openPalette("commands"); return;
    case "session.pick": ui.openPalette("sessions"); return;
    case "target.pick": ui.openPalette("targets"); return;

    // focus
    case "focus.next": ui.cyclePanel(1); return;
    case "focus.prev": ui.cyclePanel(-1); return;

    // diff — vim scrolling
    case "diff.lineDown": scrollPanelByLines("diff", 1); return;
    case "diff.lineUp": scrollPanelByLines("diff", -1); return;
    case "diff.top": scrollPanelTo("diff", "top"); return;
    case "diff.bottom": scrollPanelTo("diff", "bottom"); return;
    case "diff.halfDown": scrollPanelByLines("diff", Math.ceil(pageLines("diff") / 2)); return;
    case "diff.halfUp": scrollPanelByLines("diff", -Math.ceil(pageLines("diff") / 2)); return;
    case "diff.pageDown": scrollPanelByLines("diff", pageLines("diff")); return;
    case "diff.pageUp": scrollPanelByLines("diff", -pageLines("diff")); return;
    case "file.full": {
      ui.toggleFullFile();
      const on = useUI.getState().fullFile;
      ui.setOutput(on ? "whole file · unchanged lines shown" : "changes only");
      if (on && ui.hunkId) requestAnimationFrame(() => reveal("diff", `[data-hunk="${ui.hunkId}"]`, "start"));
      return;
    }
    case "hunk.expand": {
      const shown = shownHunks(qc);
      const h = shown.hunks.find((x) => x.id === shown.cursor);
      if (!h) return;
      if (h.lines.length <= COLLAPSE_LINES) { ui.setOutput("this hunk isn't collapsed"); return; }
      ui.toggleExpanded(h.id);
      ui.setOutput(`${useUI.getState().expanded[h.id] ? "expanded" : "collapsed"} hunk · ${h.lines.length} lines`);
      return;
    }
    case "hunk.next":
    case "hunk.prev": {
      const shown = shownHunks(qc);
      const ids = shown.hunks.map((h) => h.id);
      if (!ids.length) return;
      const i = shown.cursor ? ids.indexOf(shown.cursor) : -1;
      const n = action === "hunk.next" ? Math.min(i + 1, ids.length - 1) : Math.max(i - 1, 0);
      shown.set(ids[n]);
      reveal("diff", `[data-hunk="${ids[n]}"]`, "start");
      ui.setOutput(`hunk ${n + 1} / ${ids.length}`);
      return;
    }

    // filters
    case "filter.query":
      document.querySelector<HTMLInputElement>(ui.leftView === "history" ? "[data-log-filter]" : "[data-filter-input]")?.select();
      return;

    // branch history (M13)
    case "history.toggle": {
      const to = ui.leftView === "history" ? "changeset" : "history";
      ui.setLeftView(to);
      ui.setOutput(to === "history" ? `branch history · ⏎ shows a commit` : "changeset");
      return;
    }
    case "log.next":
    case "log.prev":
    case "log.first":
    case "log.last": {
      const order = logOrder(visibleLog(qc.getQueryData<InfiniteData<BranchLog>>(logKey()), ui.logQuery));
      const i = order.indexOf(ui.logCursor);
      const n =
        action === "log.first" ? 0
        : action === "log.last" ? order.length - 1
        : action === "log.next" ? Math.min((i === -1 ? 0 : i) + 1, order.length - 1)
        : Math.max((i === -1 ? 0 : i) - 1, 0);
      moveLogCursor(order[n]);
      return;
    }
    case "log.open": openLogCursor(qc); return;
    case "filter.all": ui.setFilter("all"); return;
    case "filter.open": ui.setFilter("open"); return;
    case "filter.flagged": ui.setFilter("flagged"); return;
    case "filter.mask": {
      ui.toggleMask();
      const model = qc.getQueryData<ReviewModel>(reviewKey());
      const n = Object.values(model?.files ?? {}).filter((m) => m.noise).length;
      ui.setOutput(`${useUI.getState().maskNoise ? "masked" : "unmasked"} ${n} generated & lock file${n === 1 ? "" : "s"}`);
      return;
    }
    case "flag.jump": {
      const model = qc.getQueryData<ReviewModel>(reviewKey());
      const line = ui.selectedPath ? model?.files[ui.selectedPath]?.flags.find((f) => f.line != null)?.line : null;
      if (line == null) { ui.setOutput("no flagged line in this file"); return; }
      jumpToLine(line);
      return;
    }

    // global
    case "repo.open": await openRepoDialog(qc); return;
    case "workspace.pick": {
      const settings = qc.getQueryData<Settings>(QK.settings);
      if (!settings?.workspaceRoot) { ui.setOutput(`no workspace root · ${keyLabel("⌘O")} a folder of repos`); return; }
      ui.openPalette("projects");
      return;
    }
    case "workspace.clear": {
      qc.setQueryData(QK.settings, await api.workspaceClear());
      qc.removeQueries({ queryKey: ["workspace"] });
      ui.setOutput("workspace root cleared");
      return;
    }
    case "refresh":
      if (qc.getQueryData<Settings>(QK.settings)?.workspaceRoot) {
        qc.setQueryData(QK.workspace, await api.workspaceScan(true));
        void qc.invalidateQueries({ queryKey: ["workspace", "peeks"] });
      }
      await qc.invalidateQueries({ queryKey: ["repo"] });
      ui.setOutput("resynced with worktree");
      return;
    case "help.toggle": ui.toggleHelp(); return;
    case "close":
      if (ui.palette) ui.openPalette(null);
      else if (ui.transmit && !ui.transmit.running) ui.setTransmit(null);
      else if (ui.helpOpen) ui.toggleHelp();
      else if (ui.logSha) {
        ui.showCommit(null);
        ui.setOutput("back to the review");
      } else if (ui.leftView === "history") ui.setLeftView("changeset");
      return;
  }
}
