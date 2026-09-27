import { HudFrame } from "../components/HudFrame";
import { StatusChip } from "../components/StatusChip";
import { useChangeset, useStatus } from "../hooks/useRepo";
import { filterCounts, splitPath, type Row } from "../lib/changeset";
import { errText } from "../lib/invoke";
import { useUI, type FileFilter } from "../store/ui";

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
  const counts = filterCounts(view.base);
  const total = data?.files.length ?? 0;

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
            disabled
            title="Intent groups need a Claude transcript"
            className={`h-[30px] text-[10.5px] tracking-[0.16em] border disabled:opacity-40 ${pill(grouping === "intent")}`}
          >
            BY INTENT
          </button>
          <button
            type="button"
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
        <label className="flex items-center gap-2 text-[10.5px] tracking-[0.1em] text-ink-dim opacity-40">
          <input type="checkbox" checked disabled readOnly className="m-0 accent-[var(--ac)]" />
          MASK GENERATED &amp; LOCKFILES (0)
        </label>
      </div>

      <nav
        aria-label="Changed files"
        data-panel-scroll="changeset"
        className="flex-1 min-h-0 overflow-y-auto px-[6px] pt-1 pb-[10px]"
      >
        {error && <Empty text={errText(error)} danger />}
        {isLoading && <Empty text="SCANNING WORKTREE…" />}
        {data && total === 0 && <Empty text="NO SIGNAL · WORKTREE CLEAN" />}
        {data && total > 0 && view.rows.length === 0 && <Empty text="NO SIGNAL · NOTHING MATCHES" />}
        {view.rows.map((r) => (r.kind === "folder" ? <FolderRow key={"d:" + r.key} row={r} /> : <FileRow key={r.file.path} row={r} />))}
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
  const { name } = splitPath(f.path);

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
            active ? "text-ink-bright" : "text-ink-light",
          ].join(" ")}
        >
          {name}
        </span>
      </span>
      {f.isBinary ? (
        <span className="text-[10px] tracking-[0.12em] text-ink-faint">BIN</span>
      ) : (
        <>
          <span className="text-[10.5px] text-sig-add">+{f.adds}</span>
          <span className="text-[10.5px] text-sig-delete min-w-[24px]">−{f.dels}</span>
        </>
      )}
      <span className="w-3 text-[11px] text-sig-add" />
    </button>
  );
}
