import { open } from "@tauri-apps/plugin-dialog";
import type { QueryClient } from "@tanstack/react-query";
import { api, errText } from "./invoke";
import { useUI } from "../store/ui";
import { QK } from "../hooks/useRepo";
import type { RepoInfo } from "./types";

/** ⌘O: a repo opens; a folder of repos becomes the workspace root and the picker opens (docs/PLAN.md M14). */
export async function openRepoDialog(qc: QueryClient) {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Open repository or workspace folder",
  });
  if (typeof selected !== "string") return;
  try {
    const res = await api.repoOpen(selected);
    if (res.kind === "repo") {
      await afterSwitch(qc, res.info);
      return;
    }
    const ui = useUI.getState();
    qc.setQueryData(QK.workspace, res.scan);
    await qc.invalidateQueries({ queryKey: QK.settings });
    await qc.invalidateQueries({ queryKey: ["workspace", "peeks"] });
    const n = res.scan.repos.length;
    ui.emitToast("ok", `workspace ${res.scan.name} · ${n}${res.scan.truncated ? "+" : ""} repo${n === 1 ? "" : "s"}`);
    ui.setOutput("workspace root · " + res.scan.root);
    ui.openPalette("projects");
  } catch (e) {
    useUI.getState().emitToast("err", errText(e));
  }
}

/** Put another repo into review: the picker's ⏎, and ⌘O on a repo. */
export async function switchRepo(qc: QueryClient, path: string) {
  const res = await api.repoOpen(path);
  if (res.kind !== "repo") throw new Error(`${path} is not a repository`);
  await afterSwitch(qc, res.info);
}

async function afterSwitch(qc: QueryClient, info: RepoInfo) {
  const ui = useUI.getState();
  ui.select(null);
  ui.setSession(null);
  ui.setTarget({ kind: "worktree" }); // repo_open starts on the working tree
  ui.unfoldAll();
  ui.setQuery("");
  ui.setLeftView("changeset"); // a shown commit belongs to the old repo
  await qc.invalidateQueries({ queryKey: ["repo"] });
  await qc.invalidateQueries({ queryKey: ["workspace", "peeks"] });
  ui.emitToast("ok", "linked " + info.name);
  ui.setOutput("repo linked · " + info.workdir);
}
