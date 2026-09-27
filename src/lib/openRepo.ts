import { open } from "@tauri-apps/plugin-dialog";
import type { QueryClient } from "@tanstack/react-query";
import { api, errText } from "./invoke";
import { useUI } from "../store/ui";

export async function openRepoDialog(qc: QueryClient) {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Open repository",
  });
  if (typeof selected !== "string") return;
  try {
    const info = await api.repoOpen(selected);
    const ui = useUI.getState();
    ui.select(null);
    ui.unfoldAll();
    ui.setQuery("");
    await qc.invalidateQueries({ queryKey: ["repo"] });
    ui.emitToast("ok", "linked " + info.name);
    ui.setOutput("repo linked · " + info.workdir);
  } catch (e) {
    useUI.getState().emitToast("err", errText(e));
  }
}
