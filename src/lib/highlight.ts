// Syntax highlighting for diff lines (M7), with shiki's css-variables
// theme so the colours live in hud.css. Grammars load on first use.

import type { HighlighterCore, ThemedToken } from "shiki/core";

/** Hunks longer than this start collapsed (large-diff guard, M7). */
export const COLLAPSE_LINES = 300;

/** Lines above this are shown plain; tokenizing them isn't worth the wait. */
const MAX_LINES = 2000;
const CACHE_MAX = 400;

const GRAMMARS: Record<string, () => Promise<unknown>> = {
  typescript: () => import("shiki/langs/typescript.mjs"),
  tsx: () => import("shiki/langs/tsx.mjs"),
  javascript: () => import("shiki/langs/javascript.mjs"),
  jsx: () => import("shiki/langs/jsx.mjs"),
  json: () => import("shiki/langs/json.mjs"),
  rust: () => import("shiki/langs/rust.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  html: () => import("shiki/langs/html.mjs"),
  markdown: () => import("shiki/langs/markdown.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
  toml: () => import("shiki/langs/toml.mjs"),
  python: () => import("shiki/langs/python.mjs"),
  go: () => import("shiki/langs/go.mjs"),
  shellscript: () => import("shiki/langs/shellscript.mjs"),
  sql: () => import("shiki/langs/sql.mjs"),
};

const EXT: Record<string, string> = {
  ts: "typescript", mts: "typescript", cts: "typescript", tsx: "tsx",
  js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "jsx",
  json: "json", jsonc: "json", rs: "rust", css: "css", html: "html",
  md: "markdown", mdx: "markdown", yml: "yaml", yaml: "yaml", toml: "toml",
  py: "python", go: "go", sh: "shellscript", bash: "shellscript", zsh: "shellscript", sql: "sql",
};

export function langFor(path: string): string | null {
  const name = path.split("/").pop() ?? "";
  if (name === "Dockerfile" || name.startsWith(".env")) return "shellscript";
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  return EXT[ext] ?? null;
}

let core: Promise<HighlighterCore> | null = null;
const loading = new Map<string, Promise<void>>();

function highlighter() {
  core ??= (async () => {
    const { createHighlighterCore, createCssVariablesTheme } = await import("shiki/core");
    const { createJavaScriptRegexEngine } = await import("shiki/engine/javascript");
    return createHighlighterCore({
      themes: [createCssVariablesTheme({ name: "hud", variablePrefix: "--shiki-", fontStyle: true })],
      langs: [],
      engine: createJavaScriptRegexEngine(),
    });
  })();
  return core;
}

async function ensureLang(h: HighlighterCore, lang: string) {
  if (h.getLoadedLanguages().includes(lang)) return;
  let p = loading.get(lang);
  if (!p) {
    p = GRAMMARS[lang]()
      .then((m) => h.loadLanguage((m as { default: Parameters<HighlighterCore["loadLanguage"]>[0] }).default))
      .then(() => undefined);
    loading.set(lang, p);
  }
  await p;
}

const cache = new Map<string, ThemedToken[][]>();

/**
 * Tokens per line for one hunk, tokenized as a block so multi-line strings
 * and comments inside the hunk come out right. Null when there is no
 * grammar or the hunk is too big; callers then render plain text.
 */
export async function tokenize(key: string, path: string, lines: string[]): Promise<ThemedToken[][] | null> {
  const hit = cache.get(key);
  if (hit) return hit;
  const lang = langFor(path);
  if (!lang || lines.length > MAX_LINES) return null;
  try {
    const h = await highlighter();
    await ensureLang(h, lang);
    const { tokens } = h.codeToTokens(lines.join("\n"), { lang, theme: "hud" });
    if (cache.size >= CACHE_MAX) cache.delete(cache.keys().next().value!);
    cache.set(key, tokens);
    return tokens;
  } catch {
    return null; // a grammar failing to load just means plain text
  }
}
