import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { get, writable } from "svelte/store";

/** Mirrors `AppUpdate` (app/src-tauri app_update.rs, spec §6j). */
export type AppUpdate = { version: string; notes: string };

export type UpdateState = {
  offer: AppUpdate | null;
  /** Later was pressed: hidden until the next start. */
  hidden: boolean;
  /** Update now is running; `percent` is the download progress. */
  installing: boolean;
  percent: number;
  error: string | null;
};

const fresh = (): UpdateState => ({ offer: null, hidden: false, installing: false, percent: 0, error: null });

export const appUpdate = writable<UpdateState>(fresh());

export function resetAppUpdate() {
  appUpdate.set(fresh());
}

/** Asks once at start-up. A failed check shows nothing (spec §6j). */
export async function checkForUpdate() {
  try {
    const offer = await invoke<AppUpdate | null>("app_update_check");
    if (offer && typeof offer.version === "string") appUpdate.update((s) => ({ ...s, offer }));
  } catch {
    /* silent: nobody asked for it */
  }
}

export function later() {
  appUpdate.update((s) => ({ ...s, hidden: true }));
}

const message = (e: unknown) => (typeof e === "string" ? e : "Couldn't download the update. Try again later.");

/** Update now. On success the app restarts, so this never returns normally. */
export async function installUpdate() {
  if (get(appUpdate).installing) return;
  appUpdate.update((s) => ({ ...s, installing: true, percent: 0, error: null }));
  const stop = await listen<{ percent: number }>("app-update-progress", (e) =>
    appUpdate.update((s) => ({ ...s, percent: Math.max(0, Math.min(100, Math.round(e.payload?.percent ?? 0))) })),
  );
  try {
    await invoke("app_update_install");
  } catch (e) {
    appUpdate.update((s) => ({ ...s, installing: false, error: message(e) }));
  } finally {
    stop();
  }
}
