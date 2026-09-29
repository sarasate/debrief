import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useUI } from "../store/ui";
import { findBinding } from "../lib/keymap";
import { buildChangeset, isOpen, splitPath } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { openRepoDialog } from "../lib/openRepo";
import { pageLines, reveal, scrollPanelByLines, scrollPanelTo } from "../lib/scroll";
import { QK } from "./useRepo";
import type { FileDiff, ReviewModel, Verdict } from "../lib/types";
import { currentModel, setVerdict, setViewed } from "../lib/review";

export function useKeybindings() {
  const qc = useQueryClient();

  useEffect(() => {
    const handler = async (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && isTextEntry(t)) {
        if (e.key === "Escape") t.blur();
        return;
      }
      // A clicked button or checkbox keeps focus; let space and friends
      // reach the bindings instead of re-clicking it.
      if (t && t !== document.body && (t.tagName === "BUTTON" || t.tagName === "INPUT")) t.blur();

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

function isTextEntry(t: HTMLElement): boolean {
  if (t.isContentEditable || t.tagName === "TEXTAREA") return true;
  if (t.tagName !== "INPUT") return false;
  return !["checkbox", "radio", "button", "submit", "range"].includes((t as HTMLInputElement).type);
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

/** Scroll the diff to a new-side line number and select its hunk. */
export function jumpToLine(line: number) {
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

/** Run a BINDINGS action. Buttons call this too, so mouse and keys share one path. */
export async function runAction(action: string, qc: QueryClient) {
  const ui = useUI.getState();

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
      ui.setOutput(`${where} → ${next === "keep" ? "kept" : next === "revert" ? "marked for discard" : "undecided"}`);
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
    case "filter.mask": {
      ui.toggleMask();
      const model = qc.getQueryData<ReviewModel>(QK.review(ui.sessionId));
      const n = Object.values(model?.files ?? {}).filter((m) => m.noise).length;
      ui.setOutput(`${useUI.getState().maskNoise ? "masked" : "unmasked"} ${n} generated & lock file${n === 1 ? "" : "s"}`);
      return;
    }
    case "flag.jump": {
      const model = qc.getQueryData<ReviewModel>(QK.review(ui.sessionId));
      const line = ui.selectedPath ? model?.files[ui.selectedPath]?.flags.find((f) => f.line != null)?.line : null;
      if (line == null) { ui.setOutput("no flagged line in this file"); return; }
      jumpToLine(line);
      return;
    }

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
