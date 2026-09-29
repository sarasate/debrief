// Field notes and transmit (SPEC §6), shared by the keys and the panel.

import type { QueryClient } from "@tanstack/react-query";
import { api } from "./invoke";
import { currentModel } from "./review";
import { splitPath } from "./changeset";
import { QK, diffKey, reviewKey } from "../hooks/useRepo";
import { useUI } from "../store/ui";
import type { FileDiff, TransmitMode } from "./types";

const FORCE_WINDOW_MS = 8_000;

/** What a note written now would be attached to. */
export function noteTarget(qc: QueryClient) {
  const ui = useUI.getState();
  const path = ui.selectedPath;
  if (!path) return null;
  const diff = qc.getQueryData<FileDiff>(diffKey(path));
  const hunks = diff?.hunks ?? [];
  const i = ui.hunkId ? hunks.findIndex((h) => h.id === ui.hunkId) : -1;
  const scoped = ui.noteScope === "auto" && ui.hunkPinned && i !== -1;
  return {
    path,
    name: splitPath(path).name,
    hunk: scoped ? { id: hunks[i].id, header: hunks[i].header, index: i + 1, of: hunks.length } : null,
  };
}

export function focusComposer(scope: "auto" | "file") {
  useUI.getState().setNoteScope(scope);
  requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>("[data-note-input]")?.focus());
}

export async function queueNote(qc: QueryClient) {
  const ui = useUI.getState();
  const model = currentModel(qc);
  const target = noteTarget(qc);
  const text = ui.noteDraft.trim();
  if (!model || !target || !text) return;
  await api.notesAdd(model.stateKey, target.path, target.hunk, text);
  ui.setNoteDraft("");
  ui.setNoteScope("auto");
  await qc.invalidateQueries({ queryKey: reviewKey() });
  ui.setOutput(`note queued @ ${target.name}` + (target.hunk ? ` · hunk ${target.hunk.index}/${target.hunk.of}` : ""));
}

export async function removeNote(qc: QueryClient, id: string) {
  const model = currentModel(qc);
  if (!model) return;
  await api.notesRemove(model.stateKey, id);
  await qc.invalidateQueries({ queryKey: reviewKey() });
  useUI.getState().setOutput("note removed");
}

export async function transmit(qc: QueryClient) {
  const ui = useUI.getState();
  const model = currentModel(qc);
  if (!model) return;
  if (!model.notes.length && !model.unreportedDiscards && !model.pendingRequests) {
    ui.setOutput("nothing to transmit · n to write a note");
    return;
  }
  const force = Date.now() < ui.forceUntil;
  ui.setForceUntil(0);
  const r = await api.notesTransmit(model.stateKey, model.session?.id ?? null, force);
  const n = "notes" in r ? r.notes : 0;
  const sent = `transmitted ${n} note${n === 1 ? "" : "s"} to claude · awaiting next turn`;
  switch (r.status) {
    case "copied":
      ui.emitToast("ok", "copied · paste into your Claude session");
      ui.setOutput(sent);
      break;
    case "written":
      ui.emitToast("ok", "wrote feedback.md · pointer copied, paste it into Claude");
      ui.setOutput(sent);
      break;
    case "started":
      ui.setTransmit({ running: true, lines: [], code: null, sent: false });
      ui.setOutput(`resuming session ${model.stateKey.slice(0, 8)} · streaming · ^c to stop`);
      break;
    case "confirm":
      ui.setForceUntil(Date.now() + FORCE_WINDOW_MS);
      ui.emitToast("err", r.message);
      ui.setOutput(`${r.message} · f again within 8s to send anyway`);
      return;
  }
  await qc.invalidateQueries({ queryKey: reviewKey() });
}

export async function setTransmitMode(qc: QueryClient, mode: TransmitMode) {
  const s = await api.settingsSet({ transmitMode: mode });
  qc.setQueryData(QK.settings, s);
  useUI.getState().setOutput(
    mode === "resume"
      ? "transmit mode: resume · f runs claude --resume in the background"
      : `transmit mode: ${mode}`,
  );
}
