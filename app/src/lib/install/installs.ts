import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writable } from "svelte/store";
import type { Game } from "$lib/library/library";


/** Mirrors the Rust types (app/src-tauri downloads.rs, fishy-core install::store). */
export type DownloadItem = {
  app_name: string;
  title: string;
  state: "downloading" | "queued" | "paused" | "error";
  downloaded: number;
  total: number;
  bytes_per_sec: number;
  error: string | null;
};
export type DownloadsView = { items: DownloadItem[] };
export type InstalledGame = {
  app_name: string;
  title: string;
  version: string;
  install_path: string;
  executable: string;
  launch_parameters: string;
  install_size: number;
  installed_at: string;
};
export type InstallOptions = {
  install_path: string;
  download_size: number;
  install_size: number;
  free_space: number;
  enough_space: boolean;
  error: string | null;
};
/** Mirrors `PlayError` (app/src-tauri play.rs). */
export type PlayError = { message: string; can_offline: boolean; signed_out: boolean };
export type GameStatus = "installed" | "downloading" | "queued" | "paused" | "error" | "none";

export const downloads = writable<DownloadItem[]>([]);
export const installed = writable<InstalledGame[]>([]);

/** Forget everything in memory (between tests). */
export function resetInstalls() {
  downloads.set([]);
  installed.set([]);
}

const asItems = (v: DownloadsView | null | undefined) => (v && Array.isArray(v.items) ? v.items : []);

async function refreshInstalled() {
  try {
    const list = await invoke<InstalledGame[]>("installed_list");
    installed.set(Array.isArray(list) ? list : []);
  } catch {
    /* keep what we have */
  }
}

/** Loads the current state and follows the backend's change events. Returns a stop function. */
export async function initInstalls(): Promise<() => void> {
  const safeListen = async (event: string, handler: (payload: unknown) => void) => {
    try {
      const stop = await listen(event, (e) => handler(e.payload));
      // Tauri's unlisten is async; if the backend is already gone (app closing) ignore it.
      return () => void Promise.resolve(stop() as unknown).catch(() => {});
    } catch {
      return () => {};
    }
  };
  const stopDownloads = await safeListen("downloads-changed", (p) => downloads.set(asItems(p as DownloadsView)));
  const stopInstalled = await safeListen("installed-changed", () => void refreshInstalled());
  try {
    downloads.set(asItems(await invoke<DownloadsView>("downloads_state")));
  } catch {
    /* nothing yet */
  }
  await refreshInstalled();
  return () => {
    stopDownloads();
    stopInstalled();
  };
}

export function statusFor(appName: string, dl: DownloadItem[], ins: InstalledGame[]): GameStatus {
  const job = dl.find((d) => d.app_name === appName);
  if (job) return job.state;
  return ins.some((g) => g.app_name === appName) ? "installed" : "none";
}

const message = (e: unknown) => (typeof e === "string" ? e : "Something went wrong. Please try again.");

export function getInstallOptions(game: Game, parent?: string | null): Promise<InstallOptions> {
  return invoke<InstallOptions>("install_options", { game, parent: parent ?? null });
}

export function pickFolder(): Promise<string | null> {
  return invoke<string | null>("pick_folder");
}

/** Starts (or queues) an install. Returns an error message, or null when it worked. */
export async function startInstall(game: Game, parent?: string | null): Promise<string | null> {
  try {
    downloads.set(asItems(await invoke<DownloadsView>("install_start", { game, parent: parent ?? null })));
    return null;
  } catch (e) {
    return message(e);
  }
}

export const pause = (appName: string) => invoke("download_pause", { appName });
export const resume = (appName: string) => invoke("download_resume", { appName });
/** Stops and deletes the files. Returns an error message, or null when it worked. */
export async function cancel(appName: string): Promise<string | null> {
  try {
    await invoke("download_cancel", { appName });
    return null;
  } catch (e) {
    return message(e);
  }
}

/** Returns an error message, or null when it worked. */
export async function uninstall(appName: string): Promise<string | null> {
  try {
    const list = await invoke<InstalledGame[]>("uninstall_game", { appName });
    installed.set(Array.isArray(list) ? list : []);
    return null;
  } catch (e) {
    return message(e);
  }
}

/** Starts an installed game (spec §6d). Returns null when it started, else what went wrong. */
export async function play(appName: string, offline = false): Promise<PlayError | null> {
  try {
    await invoke("play_game", { appName, offline });
    return null;
  } catch (e) {
    if (e && typeof e === "object" && typeof (e as PlayError).message === "string") {
      const err = e as PlayError;
      return { message: err.message, can_offline: !!err.can_offline, signed_out: !!err.signed_out };
    }
    return { message: message(e), can_offline: false, signed_out: false };
  }
}
