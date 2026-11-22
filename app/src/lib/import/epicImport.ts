import { invoke } from "@tauri-apps/api/core";
import { get, writable } from "svelte/store";
import { asItems, downloads, message, refreshInstalled, type DownloadsView } from "$lib/install/installs";
import { formatBytes } from "$lib/progress/progress";
import { authState } from "$lib/auth/auth";

/** Mirrors app/src-tauri epic_import.rs (spec §6g). */
export type Candidate = { app_name: string; title: string };
export type AddResult =
  | { kind: "added" }
  | { kind: "missing"; files: number; bytes: number }
  | { kind: "error"; message: string; signed_out: boolean };

/** Games from Epic's launcher that can be added. A failed scan counts as none. */
export async function scanEpic(): Promise<Candidate[]> {
  try {
    const list = await invoke<Candidate[]>("epic_import_scan");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

export async function addFromEpic(appName: string): Promise<AddResult> {
  try {
    return await invoke<AddResult>("epic_import_add", { appName });
  } catch (e) {
    return { kind: "error", message: message(e), signed_out: false };
  }
}

/** Queues the missing files. Returns an error message, or null when it worked. */
export async function repairFromEpic(appName: string): Promise<string | null> {
  try {
    downloads.set(asItems(await invoke<DownloadsView>("epic_import_repair", { appName })));
    return null;
  } catch (e) {
    return message(e);
  }
}

/** One line per game that couldn't be added straight away (spec §6g). */
export type Line = { game: Candidate; text: string; repair: boolean; busy: boolean; error: string | null };
export type NoteState = { scanned: boolean; hidden: boolean; waiting: Candidate[]; checking: string | null; lines: Line[] };

const fresh = (): NoteState => ({ scanned: false, hidden: false, waiting: [], checking: null, lines: [] });

/** Kept outside the component, so leaving the Library and coming back keeps it (until restart). */
export const epicNote = writable<NoteState>(fresh());

export function resetEpicImport() {
  epicNote.set(fresh());
}

// Another account may sign in next: start over.
authState.subscribe((s) => {
  if (s.kind === "SignedOut") resetEpicImport();
});

/** Scans Epic's list once per app start. */
export async function scanOnce() {
  if (get(epicNote).scanned) return;
  epicNote.update((n) => ({ ...n, scanned: true }));
  const found = await scanEpic();
  epicNote.update((n) => ({ ...n, waiting: found }));
}

const patch = (f: (n: NoteState) => Partial<NoteState>) => epicNote.update((n) => ({ ...n, ...f(n) }));

/** Add them: checks each waiting game in turn. A second call while one runs does nothing. */
export async function addAll() {
  const todo = get(epicNote).waiting;
  if (get(epicNote).checking || todo.length === 0) return;
  patch(() => ({ waiting: [] }));
  for (const game of todo) {
    patch(() => ({ checking: game.title }));
    const r = await addFromEpic(game.app_name);
    if (r.kind === "missing") {
      const what = r.files === 1 ? "1 file is" : `${r.files} files are`;
      const text = `${game.title}: ${what} missing or damaged (${formatBytes(r.bytes)}).`;
      patch((n) => ({ lines: [...n.lines, { game, text, repair: true, busy: false, error: null }] }));
    } else if (r.kind === "error") {
      patch((n) => ({ lines: [...n.lines, { game, text: r.message, repair: false, busy: false, error: null }] }));
      if (r.signed_out) break;
    }
  }
  patch(() => ({ checking: null }));
  await refreshInstalled();
}

const setLine = (app: string, f: (l: Line) => Line | null) =>
  patch((n) => ({ lines: n.lines.flatMap((l) => (l.game.app_name === app ? [f(l)].filter((x): x is Line => x !== null) : [l])) }));

/** Download them, once at a time per game. */
export async function repair(appName: string) {
  const line = get(epicNote).lines.find((l) => l.game.app_name === appName);
  if (!line || line.busy) return;
  setLine(appName, (l) => ({ ...l, busy: true, error: null }));
  const err = await repairFromEpic(appName);
  setLine(appName, (l) => (err ? { ...l, busy: false, error: err } : null));
}
