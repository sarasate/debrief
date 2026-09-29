import { useQueryClient } from "@tanstack/react-query";
import { HudFrame } from "../components/HudFrame";
import { StatusChip } from "../components/StatusChip";
import { useChangeset, useReview, useStatus } from "../hooks/useRepo";
import { runAction } from "../hooks/useKeybindings";
import { filterCounts, splitPath, type Row } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { useUI, type FileFilter, type Grouping } from "../store/ui";

const pill = (active: boolean) =>
  active
    ? "border-hud bg-hud/[.14] text-hud"
    : "border-hud/[.18] bg-transparent text-ink-dim";

const FILTERS: { id: FileFilter; label: string }[] = [
  { id: "all", label: "ALL" },
  { id: "open", label: "OPEN" },
  { id: "flagged", label: "FLAGGED" },
];

export function ChangesetPanel() {
  const { data, isLoading, error } = useStatus();
  const view = useChangeset();
  const query = useUI((s) => s.query);
  const filter = useUI((s) => s.filter);
  const grouping = useUI((s) => s.grouping);
  const { data: model } = useReview();
  const maskNoise = useUI((s) => s.maskNoise);
  const counts = filterCounts(model, view.base);
  const total = data?.files.length ?? 0;
  const configError = model?.noise.error;
  const qc = useQueryClient();
  const setGrouping = (g: Grouping) => grouping !== g && void runAction("grouping.toggle", qc);

  return (
    <HudFrame
      id="changeset"
      title="Changeset"
      className="w-[360px] flex-none"
      badge={
        <span className="px-[6px] py-px border border-hud/[.45] text-hud">
          {view.visible.length}/{total}
        </span>
      }
    >
      <div className="flex-none flex flex-col gap-[9px] px-3 py-[10px] border-b border-hud/10">
        <label className="flex items-center gap-2 h-8 px-[10px] bg-bg-deep border border-hud/[.18]">
          <span className="text-hud">/</span>
          <span className="sr-only">Filter files by path</span>
          <input
            data-filter-input
            type="text"
            placeholder="filter path…"
            value={query}
            onChange={(e) => useUI.getState().setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.currentTarget.blur();
                useUI.getState().setFocus("changeset");
              }
            }}
            className="flex-1 min-w-0 border-0 outline-none bg-transparent text-ink-light text-[12px] placeholder:text-ink-dimmer"
          />
        </label>
        <div className="grid grid-cols-2 gap-[6px]">
          <button
            type="button"
            onClick={() => setGrouping("intent")}
            className={`dc-hov h-[30px] text-[10.5px] tracking-[0.16em] border ${pill(grouping === "intent")}`}
          >
            BY INTENT
          </button>
          <button
            type="button"
            onClick={() => setGrouping("tree")}
            className={`dc-hov h-[30px] text-[10.5px] tracking-[0.16em] border ${pill(grouping === "tree")}`}
          >
            FILE TREE
          </button>
        </div>
        <div className="flex gap-[6px]">
          {FILTERS.map((f) => (
            <button
              key={f.id}
              type="button"
              onClick={() => useUI.getState().setFilter(f.id)}
              className={`dc-hov h-[26px] px-[9px] text-[10px] tracking-[0.14em] border ${pill(filter === f.id)}`}
            >
              {f.label} {counts[f.id]}
            </button>
          ))}
        </div>
        <label
          title={model?.noise.globs.join("  ")}
          className="flex items-center gap-2 text-[10.5px] tracking-[0.1em] text-ink-dim cursor-pointer"
        >
          <input
            type="checkbox"
            checked={maskNoise}
            onChange={() => void runAction("filter.mask", qc)}
            className="m-0 accent-[var(--ac)]"
          />
          MASK GENERATED &amp; LOCKFILES ({view.noiseCount})
          {model?.noise.source === "file" && <span className="text-ink-faint">· .debrief.toml</span>}
        </label>
        {configError && (
          <div className="px-2 py-[6px] border border-sig-warn/45 bg-sig-warn/[.08] text-[10.5px] leading-[1.5] text-sig-warnInk break-words">
            {configError} · using default globs
          </div>
        )}
      </div>

      <nav
        aria-label="Changed files"
        data-panel-scroll="changeset"
        className="flex-1 min-h-0 overflow-y-auto px-[6px] pt-1 pb-[10px]"
      >
        {error && <Empty text={errText(error)} danger />}
        {isLoading && <Empty text="SCANNING WORKTREE…" />}
        {data && total === 0 && <Empty text="NO SIGNAL · WORKTREE CLEAN" />}
        {data && total > 0 && view.rows.length === 0 && (
          <Empty
            text={
              view.base.length === 0 && maskNoise && view.noiseCount === total
                ? "ONLY GENERATED FILES CHANGED · M TO UNMASK"
                : "NO SIGNAL · NOTHING MATCHES"
            }
          />
        )}
        {view.rows.map((r) =>
          r.kind === "group" ? (
            <GroupRow key={"g:" + r.id} row={r} />
          ) : r.kind === "folder" ? (
            <FolderRow key={"d:" + r.key} row={r} />
          ) : (
            <FileRow key={r.file.path} row={r} />
          ),
        )}
      </nav>
    </HudFrame>
  );
}

function Empty({ text, danger = false }: { text: string; danger?: boolean }) {
  return (
    <div
      className={[
        "px-3 py-7 text-center text-[11px] tracking-[0.14em] break-words",
        danger ? "text-sig-danger" : "text-ink-faint",
      ].join(" ")}
    >
      {text}
    </div>
  );
}

function GroupRow({ row }: { row: Extract<Row, { kind: "group" }> }) {
  return (
    <div className="flex items-baseline gap-2 px-2 pt-3 pb-[5px]">
      <span title={row.title} className="min-w-0 text-[10px] tracking-[0.18em] text-hud whitespace-nowrap overflow-hidden text-ellipsis">
        {row.title} · {row.count}
      </span>
      <span className="flex-1" />
      <span className="text-[10.5px] text-sig-add">+{row.adds}</span>
      <span className="text-[10.5px] text-sig-delete">−{row.dels}</span>
    </div>
  );
}

function FolderRow({ row }: { row: Extract<Row, { kind: "folder" }> }) {
  return (
    <button
      type="button"
      onClick={() => useUI.getState().toggleFolder(row.key)}
      style={{ paddingLeft: row.pad }}
      className="w-full h-[26px] flex items-center gap-[6px] pr-2 border-0 bg-transparent text-ink-dim text-[11.5px] text-left"
    >
      <span className="w-[10px] text-hud">{row.open ? "▾" : "▸"}</span>
      {row.name}/
    </button>
  );
}

function FileRow({ row }: { row: Extract<Row, { kind: "file" }> }) {
  const f = row.file;
  const active = useUI((s) => s.selectedPath === f.path);
  const { data: flags } = useReview((m) => m.files[f.path]?.flags);
  const { data: viewed } = useReview((m) => !!m.files[f.path]?.viewed);
  const { name, dirs } = splitPath(f.path);

  return (
    <button
      type="button"
      data-path={f.path}
      title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}
      onClick={() => useUI.getState().select(f.path)}
      style={{ paddingLeft: row.pad }}
      className={[
        "relative w-full min-h-[34px] flex items-center gap-[9px] text-left py-[5px] pr-[10px] border",
        active ? "border-hud/30 bg-hud/[.09]" : "border-transparent bg-transparent",
      ].join(" ")}
    >
      {active && (
        <>
          <span className="dc-corner-inner tl" />
          <span className="dc-corner-inner tr" />
          <span className="dc-corner-inner bl" />
          <span className="dc-corner-inner br" />
        </>
      )}
      <StatusChip status={f.status} />
      <span className="flex-1 min-w-0 flex flex-col">
        <span
          className={[
            "text-[12px] whitespace-nowrap overflow-hidden text-ellipsis",
            active ? "text-ink-bright" : viewed ? "text-ink-faint" : "text-ink-light",
          ].join(" ")}
        >
          {name}
        </span>
        {row.showDir && dirs.length > 0 && (
          <span className="text-[10px] text-ink-faint whitespace-nowrap overflow-hidden text-ellipsis">{dirs.join("/")}</span>
        )}
      </span>
      {flags && flags.length > 0 && (
        <span
          aria-label="Flagged"
          title={flags.map((x) => x.reason).join("\n")}
          className="flex-none px-1 text-[10px] font-bold text-ink-void bg-sig-warn"
        >
          !
        </span>
      )}
      {f.isBinary ? (
        <span className="text-[10px] tracking-[0.12em] text-ink-faint">BIN</span>
      ) : (
        <>
          <span className="text-[10.5px] text-sig-add">+{f.adds}</span>
          <span className="text-[10.5px] text-sig-delete min-w-[24px]">−{f.dels}</span>
        </>
      )}
      <span aria-label={viewed ? "Viewed" : undefined} className="w-3 text-[11px] text-sig-add">
        {viewed ? "✓" : ""}
      </span>
    </button>
  );
}
