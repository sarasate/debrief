// Themes (docs/PLAN.md M12). The settings pick a mode and a dark theme;
// this resolves them against the system appearance into the `data-theme`
// that src/styles/themes.css keys on. src-tauri/src/theme.rs mirrors
// `resolveTheme` for the native window.

import { useSyncExternalStore } from "react";
import type { DarkTheme, ThemeMode, ThemeName } from "./types";

const DARK_QUERY = "(prefers-color-scheme: dark)";
/** Last theme settings, for the pre-paint script in index.html only. The
 * settings file stays the source of truth. */
const CACHE_KEY = "debrief.theme";

export function resolveTheme(mode: ThemeMode, dark: DarkTheme, systemDark: boolean): ThemeName {
  if (mode === "light") return "daylight";
  if (mode === "dark") return dark;
  return systemDark ? dark : "daylight";
}

export function systemIsDark(): boolean {
  return window.matchMedia(DARK_QUERY).matches;
}

function subscribe(onChange: () => void) {
  const mq = window.matchMedia(DARK_QUERY);
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}

/** The system appearance, live (macOS "Auto" included). */
export function useSystemDark(): boolean {
  return useSyncExternalStore(subscribe, systemIsDark);
}

/** Sets `data-theme` on <html> and remembers the settings for the next
 * launch's first paint. */
export function applyTheme(name: ThemeName, mode: ThemeMode, dark: DarkTheme) {
  document.documentElement.dataset.theme = name;
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ mode, dark }));
  } catch {
    // Only the pre-paint guess is lost; the settings still apply.
  }
}
