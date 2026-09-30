// Panels own their scroll containers. The keybinding layer reaches them by
// data attribute rather than threading refs through the store.

export function panelScroller(panel: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-panel-scroll="${panel}"]`);
}

function rowHeight(el: HTMLElement): number {
  const row = el.querySelector<HTMLElement>("[data-line]");
  return row?.offsetHeight || 23;
}

/** Viewport height in diff lines — the unit for ctrl-d / ctrl-f. */
export function pageLines(panel: string): number {
  const el = panelScroller(panel);
  if (!el) return 20;
  return Math.max(1, Math.floor(el.clientHeight / rowHeight(el)));
}

export function scrollPanelByLines(panel: string, lines: number) {
  const el = panelScroller(panel);
  if (!el) return;
  el.scrollTop += lines * rowHeight(el);
}

export function scrollPanelTo(panel: string, edge: "top" | "bottom") {
  const el = panelScroller(panel);
  if (!el) return;
  el.scrollTop = edge === "top" ? 0 : el.scrollHeight;
}

/** Keep `selector`'s element on screen, with a little room around it. */
export function reveal(panel: string, selector: string, align: "nearest" | "start" = "nearest") {
  const el = panelScroller(panel);
  const target = el?.querySelector<HTMLElement>(selector);
  if (!el || !target) return;
  const box = el.getBoundingClientRect();
  const r = target.getBoundingClientRect();
  const pad = 24;
  if (align === "start") {
    el.scrollTop += r.top - box.top - 12;
  } else if (r.top - pad < box.top) {
    el.scrollTop -= box.top - r.top + pad;
  } else if (r.bottom + pad > box.bottom) {
    el.scrollTop += r.bottom - box.bottom + pad;
  }
}

const saved = new Map<string, number>();

/** Keep a panel's scroll position for `restoreScroll` (leaving the review for a commit). */
export function rememberScroll(panel: string) {
  const el = panelScroller(panel);
  if (el) saved.set(panel, el.scrollTop);
}

/** Put back a remembered position; false when there was none. */
export function restoreScroll(panel: string): boolean {
  const top = saved.get(panel);
  const el = panelScroller(panel);
  saved.delete(panel);
  if (top == null || !el) return false;
  el.scrollTop = top;
  return true;
}
