import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useUI } from "../store/ui";
import { BINDINGS, keyLabel } from "../lib/keymap";
import { api, errText } from "../lib/invoke";
import { runAction } from "../hooks/useKeybindings";
import { QK, usePeeks, useRepoCurrent, useReview, useSessions, useTargets, useWorkspace } from "../hooks/useRepo";
import { switchRepo } from "../lib/openRepo";
import { switchTarget } from "../lib/target";
import { agoLabel } from "../hooks/useNow";

interface Item {
  key: string;
  label: string;
  detail: string;
  mark?: string;
  tags?: { label: string; tone: "agent" | "dim" }[];
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
  const { data: targets, isLoading: targetsLoading } = useTargets(mode === "targets");
  // What the backend is actually reviewing, from the model.
  const { data: activeHead } = useReview((m) => m.range?.head ?? null);
  const { data: current } = useReview((m) => m.session?.id ?? null);
  const pinned = useUI((s) => s.sessionId);
  const { data: scan, isLoading: scanLoading, error: scanError } = useWorkspace(mode === "projects");
  const { data: peeks } = usePeeks(scan, mode === "projects");
  const { data: repo } = useRepoCurrent();
  const activeRepo = repo ? trimSlash(repo.workdir) : null;

  // The root folder has gone: the backend forgot it; say so and close.
  useEffect(() => {
    if (mode !== "projects" || !scanError) return;
    useUI.getState().emitToast("err", errText(scanError));
    useUI.getState().openPalette(null);
    void qc.invalidateQueries({ queryKey: QK.settings });
    qc.removeQueries({ queryKey: ["workspace"] });
  }, [mode, scanError, qc]);

  useEffect(() => {
    if (!mode) return;
    setQ("");
    setSel(-1);
    setTimeout(() => inputRef.current?.focus(), 0);
  }, [mode]);

  const items = useMemo<Item[]>(() => {
    const needle = q.trim().toLowerCase();
    if (mode === "commands") {
      // `terminal-app <name>` takes an argument too.
      const termArg = /^:?terminal-app(?:\s+(.*))?$/i.exec(q.trim());
      if (termArg) {
        const name = (termArg[1] ?? "").trim();
        return [
          {
            key: "terminal-app",
            label: ":terminal-app " + (name || "<name>"),
            detail: name ? `O opens the repo in ${name}` : "type an app name, e.g. iTerm or Ghostty",
            run: async () => {
              if (!name) return;
              close();
              qc.setQueryData(QK.settings, await api.settingsSet({ terminalApp: name }));
              useUI.getState().setOutput("terminal app · " + name);
            },
          },
        ];
      }
      // `claude-path <path>` takes an argument, case kept.
      const arg = /^:?claude-path(?:\s+(.*))?$/i.exec(q.trim());
      if (arg) {
        const path = (arg[1] ?? "").trim();
        return [
          {
            key: "claude-path",
            label: ":claude-path " + (path || "(auto-detect)"),
            detail: path ? "use this claude binary for resume mode" : "clear the setting and search PATH and the usual install dirs",
            run: async () => {
              close();
              qc.setQueryData(QK.settings, await api.settingsSet({ claudePath: path }));
              useUI.getState().setOutput(path ? "claude path · " + path : "claude path · auto-detect");
            },
          },
        ];
      }
      return COMMANDS.filter((c) => !needle || c.cmd.includes(needle) || c.desc.toLowerCase().includes(needle)).map(
        (c) => ({
          key: c.action,
          label: ":" + c.cmd,
          detail: c.key ? `${c.desc.toLowerCase()} · ${keyLabel(c.key)}` : c.desc.toLowerCase(),
          run: async () => {
            // Takes an argument: pre-fill it and keep the palette open.
            if (c.action === "settings.claudePath" || c.action === "settings.terminalApp") {
              setQ(c.action === "settings.claudePath" ? "claude-path " : "terminal-app ");
              inputRef.current?.focus();
              return;
            }
            // `:session` and `:target` swap the palette over instead of closing it.
            if (c.action !== "session.pick" && c.action !== "target.pick") close();
            await runAction(c.action, qc);
          },
        }),
      );
    }
    const now = Date.now();
    if (mode === "projects") {
      const peekBy = new Map((peeks ?? []).map((p) => [p.path, p]));
      const repos = (scan?.repos ?? [])
        .filter((r) => !needle || r.name.toLowerCase().includes(needle) || r.rel.toLowerCase().includes(needle))
        .sort((a, b) => (b.lastSession ?? -1) - (a.lastSession ?? -1) || a.name.localeCompare(b.name));
      return repos.map((r): Item => {
        const p = peekBy.get(r.path);
        const changed = !p ? "…" : p.changed === null ? "status unreadable" : p.changed === 0 ? "clean" : `${p.changed} changed`;
        const session = r.lastSession ? `claude ${agoLabel(r.lastSession, now).toLowerCase()}` : "no claude session";
        // A session newer than the last commit probably left changes to review.
        const fresh = !!p && !!r.lastSession && r.lastSession > (p.lastCommit ?? 0);
        return {
          key: r.path,
          label: r.name,
          detail: [r.rel, p ? p.branch ?? "no HEAD" : "…", changed, session].join(" · "),
          mark: trimSlash(r.path) === activeRepo ? "ACTIVE" : undefined,
          tags: [
            ...(fresh ? [{ label: "CLAUDE", tone: "agent" as const }] : []),
            ...(r.worktree ? [{ label: "WORKTREE", tone: "dim" as const }] : []),
          ],
          run: async () => {
            close();
            if (trimSlash(r.path) === activeRepo) return;
            await switchRepo(qc, r.path);
          },
        };
      });
    }
    if (mode === "targets") {
      const base = targets?.base ?? null;
      const active = (name: string | null) => (activeHead ?? null) === name;
      const items: Item[] = [
        {
          key: "worktree",
          label: "Working tree",
          detail: "uncommitted changes against HEAD",
          mark: active(null) ? "ACTIVE" : undefined,
          run: async () => {
            close();
            await switchTarget(qc, { kind: "worktree" });
          },
        },
        ...(targets?.branches ?? []).map((b) => ({
          key: "b:" + b.name,
          label: b.name + (b.checkedOut ? "  (checked out)" : ""),
          detail:
            (base ? `${b.ahead} ahead · ${b.behind} behind ${base}` : "no base branch found") +
            ` · ${agoLabel(b.updatedAt, now).toLowerCase()} · ${b.subject}`,
          mark: active(b.name) ? "ACTIVE" : undefined,
          run: async () => {
            close();
            await switchTarget(qc, { kind: "branch", head: b.name, base: null });
          },
        })),
      ];
      return items.filter((i) => !needle || i.label.toLowerCase().includes(needle) || i.detail.toLowerCase().includes(needle));
    }
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
  }, [mode, q, sessions, current, pinned, qc, targets, activeHead, scan, peeks, activeRepo]);

  if (!mode) return null;

  // Opening the picker puts the cursor on the first repo that isn't the
  // open one, so `P ⏎` jumps to the most recent other repo.
  const cur = sel >= 0 ? Math.min(sel, items.length - 1) : Math.max(0, mode === "projects" ? items.findIndex((i) => !i.mark) : 0);

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
            {mode === "sessions" ? "⌁" : mode === "targets" ? "⎇" : mode === "projects" ? "▤" : ":"}
          </span>
          <input
            ref={inputRef}
            // A command line, not prose: no macOS completion, correction or
            // inline predictions (WebKit's `writingsuggestions`, not yet in
            // React's types).
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            {...{ writingsuggestions: "false" }}
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") { e.preventDefault(); close(); }
              if (e.key === "ArrowDown") { e.preventDefault(); setSel(Math.min(items.length - 1, cur + 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setSel(Math.max(0, cur - 1)); }
              if (e.key === "Enter" && items[cur]) { e.preventDefault(); void exec(items[cur]); }
            }}
            placeholder={
              mode === "sessions"
                ? "filter sessions by title…"
                : mode === "targets"
                ? "filter branches…"
                : mode === "projects"
                ? "filter repos by name or path…"
                : "project · target · session · tree · transmit-mode · theme · accent · claude-path … · help"
            }
            className="flex-1 bg-transparent border-none outline-none text-[15px] text-ink-bright placeholder:text-ink-dimmer"
          />
          {mode === "targets" && (
            <span className="text-[10px] tracking-[0.2em] text-hud whitespace-nowrap">REVIEW TARGET</span>
          )}
          {mode === "projects" && (
            <span className="text-[10px] tracking-[0.2em] text-hud whitespace-nowrap uppercase">
              WORKSPACE · {scan?.name ?? "—"}
            </span>
          )}
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
              {i === cur && <div className="absolute inset-0 pointer-events-none bg-hud/[.11] border-l-2 border-hud" />}
              <span className="relative z-[1] flex-1 min-w-0 flex flex-col gap-[2px]">
                <span className="text-[13px] text-ink-light whitespace-nowrap overflow-hidden text-ellipsis">{it.label}</span>
                <span className="text-[11px] text-ink-dimmer whitespace-nowrap overflow-hidden text-ellipsis">{it.detail}</span>
              </span>
              {it.tags?.map((t) => (
                <span
                  key={t.label}
                  className={[
                    "relative z-[1] px-[6px] py-px text-[10px] tracking-[0.16em] border",
                    t.tone === "agent" ? "border-sig-agent/50 text-sig-agent" : "border-ink-dim/40 text-ink-dim",
                  ].join(" ")}
                >
                  {t.label}
                </span>
              ))}
              {it.mark && (
                <span className="relative z-[1] px-[6px] py-px text-[10px] tracking-[0.16em] border border-sig-agent/50 text-sig-agent">
                  {it.mark}
                </span>
              )}
            </li>
          ))}
          {mode === "projects" && scanLoading && <li className="px-3 py-2 text-ink-dimmer text-xs">scanning workspace…</li>}
          {mode === "projects" && !scanLoading && scan && !scan.repos.length && (
            <li className="px-3 py-2 text-ink-dimmer text-xs">no repos under {scan.root} any more · R rescans</li>
          )}
          {mode === "projects" && scan?.truncated && (
            <li className="px-3 py-2 text-ink-dimmer text-xs">stopped at {scan.repos.length} repos · pick a narrower root</li>
          )}
          {mode === "targets" && targetsLoading && <li className="px-3 py-2 text-ink-dimmer text-xs">reading branches…</li>}
          {mode === "sessions" && isLoading && <li className="px-3 py-2 text-ink-dimmer text-xs">scanning transcripts…</li>}
          {mode === "sessions" && !isLoading && !sessions?.length && (
            <li className="px-3 py-2 text-ink-dimmer text-xs">no Claude Code sessions found for this repo</li>
          )}
          {items.length === 0 && (mode === "commands" || (mode === "projects" && !!scan?.repos.length)) && <li className="px-3 py-2 text-ink-dimmer italic text-xs">no matches</li>}
        </ul>
      </div>
    </div>
  );
}

/** git2 reports workdirs with a trailing slash; the scan doesn't. */
function trimSlash(p: string): string {
  return p.length > 1 ? p.replace(/\/+$/, "") : p;
}
