import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useUI } from "../store/ui";
import { findBinding } from "../lib/keymap";
import { buildChangeset, splitPath } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { openRepoDialog } from "../lib/openRepo";
import { pageLines, reveal, scrollPanelByLines, scrollPanelTo } from "../lib/scroll";
import { QK } from "./useRepo";
import type { FileDiff, ReviewModel } from "../lib/types";

export function useKeybindings() {
  const qc = useQueryClient();

  useEffect(() => {
    const handler = async (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const isEditing =
        t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
      if (isEditing) {
        if (e.key === "Escape") t.blur();
        return;
      }

      const ui = useUI.getState();

      // Cmd/Ctrl-O — open repo
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "o" && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        await runAction("repo.open", qc);
        return;
      }
      if (ui.helpOpen && e.key !== "Escape" && e.key !== "?") return;
      if (ui.palette && e.key !== "Escape") return;

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

function currentView(qc: QueryClient) {
  const ui = useUI.getState();
  return buildChangeset(qc.getQueryData<ReviewModel>(QK.review(ui.sessionId)), ui);
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

function selectFile(path: string | null) {
  if (!path) return;
  useUI.getState().select(path);
  requestAnimationFrame(() => reveal("changeset", `[data-path="${CSS.escape(path)}"]`));
}

/** Run a BINDINGS action. Buttons call this too, so mouse and keys share one path. */
export async function runAction(action: string, qc: QueryClient) {
  const ui = useUI.getState();

  switch (action) {
    // files
    case "file.next": selectFile(step(currentView(qc).order, ui.selectedPath, 1)); return;
    case "file.prev": selectFile(step(currentView(qc).order, ui.selectedPath, -1)); return;
    case "file.first": selectFile(currentView(qc).order[0] ?? null); return;
    case "file.last": selectFile(currentView(qc).order.slice(-1)[0] ?? null); return;

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
    case "grouping.toggle":
      ui.toggleGrouping();
      ui.setOutput(useUI.getState().grouping === "intent" ? "grouped by intent" : "grouped by file tree");
      return;
    case "palette.open": ui.openPalette("commands"); return;
    case "session.pick": ui.openPalette("sessions"); return;

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
    case "hunk.next":
    case "hunk.prev": {
      const diff = qc.getQueryData<FileDiff>(QK.diffFile(ui.selectedPath));
      const ids = diff?.hunks.map((h) => h.id) ?? [];
      if (!ids.length) return;
      const i = ui.hunkId ? ids.indexOf(ui.hunkId) : -1;
      const n = action === "hunk.next" ? Math.min(i + 1, ids.length - 1) : Math.max(i - 1, 0);
      ui.setHunk(ids[n]);
      reveal("diff", `[data-hunk="${ids[n]}"]`, "start");
      ui.setOutput(`hunk ${n + 1} / ${ids.length}`);
      return;
    }

    // filters
    case "filter.query":
      document.querySelector<HTMLInputElement>("[data-filter-input]")?.select();
      return;
    case "filter.all": ui.setFilter("all"); return;
    case "filter.open": ui.setFilter("open"); return;
    case "filter.flagged": ui.setFilter("flagged"); return;

    // global
    case "repo.open": await openRepoDialog(qc); return;
    case "refresh":
      await qc.invalidateQueries({ queryKey: ["repo"] });
      ui.setOutput("resynced with worktree");
      return;
    case "help.toggle": ui.toggleHelp(); return;
    case "close":
      if (ui.palette) ui.openPalette(null);
      else if (ui.helpOpen) ui.toggleHelp();
      return;
  }
}
