import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { get, writable } from "svelte/store";
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
  /** An update (spec §6e): can be paused, not cancelled. */
  update?: boolean;
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
  /** When Play last started it (spec §6f.4); missing for games not played since. */
  last_played?: string | null;
  /** Seconds played through Fishy Client (spec §6p). */
  playtime_secs?: number;
  /** The optional parts installed (spec §6n); null or missing: every file. */
  parts?: string[] | null;
  /** For an add-on piece (Fortnite's Save the World Content): its game. Hidden from the Library. */
  parent?: string | null;
  /** The game's Manage box choices (spec §6t); missing in old records. */
  options?: GameOptions;
};
/** Mirrors `GameOptions` (fishy-core install::store). */
export type GameOptions = {
  /** null: follow the Auto update switch in Settings. */
  auto_update: boolean | null;
  priority: boolean;
  launch_args: string;
  launch_args_on: boolean;
  /** The shortcut files Fishy Client made. */
  shortcuts: string[];
};
export type VerifyResult = { kind: "good" } | { kind: "bad"; files: number; bytes: number } | { kind: "stopped" } | { kind: "error"; message: string };
/** The running (or last) Verify (spec §6t). */
export type VerifyState = { app: string; done: number; total: number } | { app: string; result: VerifyResult };
export type InstallOptions = {
  install_path: string;
  download_size: number;
  install_size: number;
  free_space: number;
  enough_space: boolean;
  /** False when Windows won't let Fishy Client create the folder (spec §6g.3). */
  folder_ok?: boolean;
  /** Windows will ask for permission once to create the folder (spec §6k). */
  needs_permission?: boolean;
  error: string | null;
  /** Set when the saved default folder couldn't be used (spec §6f.1). */
  note?: string | null;
  /** Optional parts: installed with Epic's launcher, then imported (spec §6n). */
  has_parts?: boolean;
};
/** Mirrors `PlayError` (app/src-tauri play.rs). */
export type PlayError = { message: string; can_offline: boolean; signed_out: boolean };
export type GameStatus = "installed" | "downloading" | "queued" | "paused" | "error" | "none";

export const downloads = writable<DownloadItem[]>([]);
export const installed = writable<InstalledGame[]>([]);
/** Code names of games started by Play that are still open (spec §6f.4). */
export const running = writable<string[]>([]);
/** The running or last finished Verify (spec §6t). */
export const verifying = writable<VerifyState | null>(null);
/** A game a desktop shortcut asked to play (spec §6t); the Library plays it and clears this. */
export const playRequest = writable<string | null>(null);

/** Forget everything in memory (between tests). */
export function resetInstalls() {
  downloads.set([]);
  installed.set([]);
  running.set([]);
  verifying.set(null);
  playRequest.set(null);
}

const asNames = (v: unknown) => (Array.isArray(v) ? v.filter((n): n is string => typeof n === "string") : []);
export const asItems = (v: DownloadsView | null | undefined) => (v && Array.isArray(v.items) ? v.items : []);

/** Re-reads the installed games from disk. */
export async function refreshInstalled() {
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
  const stopRunning = await safeListen("running-changed", (p) => running.set(asNames(p)));
  const stopProgress = await safeListen("verify-progress", (p) => {
    const v = p as { app_name: string; done: number; total: number };
    verifying.set({ app: v.app_name, done: v.done, total: v.total });
  });
  const stopDone = await safeListen("verify-done", (p) => {
    const v = p as { app_name: string; result: VerifyResult };
    verifying.set({ app: v.app_name, result: v.result });
  });
  const stopPlay = await safeListen("play-request", (p) => typeof p === "string" && playRequest.set(p));
  try {
    const waiting = await invoke<string | null>("play_request_take");
    if (typeof waiting === "string" && waiting) playRequest.set(waiting);
  } catch {
    /* none */
  }
  try {
    running.set(asNames(await invoke<string[]>("running_list")));
  } catch {
    /* nothing running */
  }
  try {
    downloads.set(asItems(await invoke<DownloadsView>("downloads_state")));
  } catch {
    /* nothing yet */
  }
  await refreshInstalled();
  return () => {
    stopDownloads();
    stopInstalled();
    stopRunning();
    stopProgress();
    stopDone();
    stopPlay();
  };
}

export function statusFor(appName: string, dl: DownloadItem[], ins: InstalledGame[]): GameStatus {
  // A download of one of the game's add-on pieces counts as the game's (spec §6n).
  const job = dl.find((d) => d.app_name === appName || ins.some((i) => i.app_name === d.app_name && i.parent === appName));
  if (job) return job.state;
  return ins.some((g) => g.app_name === appName) ? "installed" : "none";
}

export const message = (e: unknown) => (typeof e === "string" ? e : "Something went wrong. Please try again.");

export function getInstallOptions(game: Game, parent?: string | null): Promise<InstallOptions> {
  return invoke<InstallOptions>("install_options", { game, parent: parent ?? null });
}

/** The Install box closed without installing: the backend drops the file list it kept. */
export function forgetInstall(appName: string) {
  invoke("install_forget", { appName }).catch(() => {});
}

/** Epic's own games folder, if it exists (spec §6k). */
export async function epicFolder(): Promise<string | null> {
  try {
    const f = await invoke<string | null>("epic_folder");
    return typeof f === "string" && f ? f : null;
  } catch {
    return null;
  }
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

/** "Played: 12 h 5 min" for the ⋯ menu (spec §6p); null when never played through Fishy Client. */
export function formatPlaytime(secs: number | undefined): string | null {
  if (!secs || secs <= 0) return null;
  if (secs < 60) return "Played: under a minute";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h === 0) return `Played: ${m} min`;
  return m === 0 ? `Played: ${h} h` : `Played: ${h} h ${m} min`;
}

/** Opens the game's folder in Windows Explorer. Returns an error message, or null. */
export async function openGameFolder(appName: string): Promise<string | null> {
  try {
    await invoke("open_game_folder", { appName });
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

/** True when Epic's newest version differs from the installed one (spec §6e), for the game or
 * any of its add-on pieces (spec §6n). */
export function hasUpdate(appName: string, ins: InstalledGame[], latest: Record<string, string>): boolean {
  const outdated = (g: InstalledGame) => !!latest[g.app_name] && latest[g.app_name] !== g.version;
  if (!ins.some((g) => g.app_name === appName)) return false;
  return ins.some((g) => (g.app_name === appName || g.parent === appName) && outdated(g));
}

/** Queues an update. Returns an error message, or null when it worked. */
export async function startUpdate(appName: string): Promise<string | null> {
  try {
    downloads.set(asItems(await invoke<DownloadsView>("update_start", { appName })));
    return null;
  } catch (e) {
    return message(e);
  }
}

/** A game's own Auto update switch, or the Settings one until it has its own (spec §6t). */
export function wantsAutoUpdate(g: InstalledGame, settingsDefault: boolean): boolean {
  return g.options?.auto_update ?? settingsDefault;
}

/** Puts a changed record in the installed list. */
function replaceInstalled(g: InstalledGame) {
  installed.update((list) => (list.some((x) => x.app_name === g.app_name) ? list.map((x) => (x.app_name === g.app_name ? g : x)) : list));
}

/** Runs a command that returns the game's record. Returns an error message, or null. */
async function withRecord(cmd: string, args: Record<string, unknown>): Promise<string | null> {
  try {
    const g = await invoke<InstalledGame>(cmd, args);
    if (g && typeof g === "object" && typeof g.app_name === "string") replaceInstalled(g);
    return null;
  } catch (e) {
    return message(e);
  }
}

/** Saves the Manage box's switches and launch options (spec §6t). */
export function setGameOptions(appName: string, o: Omit<GameOptions, "shortcuts">): Promise<string | null> {
  return withRecord("game_options_set", {
    appName,
    autoUpdate: o.auto_update,
    priority: o.priority,
    launchArgs: o.launch_args,
    launchArgsOn: o.launch_args_on,
  });
}

export const createShortcut = (appName: string) => withRecord("shortcut_create", { appName });
export const addStartMenu = (appName: string) => withRecord("shortcut_add_start_menu", { appName });
export const removeShortcut = (appName: string) => withRecord("shortcut_remove", { appName });

/** Starts checking the game's files; progress arrives in `verifying`. Returns an error, or null. */
export async function verifyStart(appName: string): Promise<string | null> {
  try {
    verifying.set({ app: appName, done: 0, total: 0 });
    await invoke("verify_start", { appName });
    return null;
  } catch (e) {
    verifying.set(null);
    return message(e);
  }
}

export const verifyStop = (appName: string) => invoke("verify_stop", { appName }).catch(() => {});

/** Download them: queues the missing or damaged files. Returns an error message, or null. */
export async function verifyRepair(appName: string): Promise<string | null> {
  try {
    downloads.set(asItems(await invoke<DownloadsView>("verify_repair", { appName })));
    verifying.set(null);
    return null;
  } catch (e) {
    return message(e);
  }
}

/** Start now on a waiting game (spec §6t). Returns an error message, or null. */
export async function startNow(appName: string, mode: "pause" | "both"): Promise<string | null> {
  try {
    downloads.set(asItems(await invoke<DownloadsView>("download_start_now", { appName, mode })));
    return null;
  } catch (e) {
    return message(e);
  }
}

/** Auto-update: queues every installed game that wants it (its own switch, else Settings') and
 * has a newer version and no download yet. A running game waits; it's queued when it closes
 * (spec §6f.4, §6t). */
export async function autoUpdate(latest: Record<string, string>, settingsDefault: boolean) {
  let list: InstalledGame[] = [];
  try {
    const fresh = await invoke<InstalledGame[]>("installed_list");
    list = Array.isArray(fresh) ? fresh : [];
  } catch {
    return;
  }
  const busy = new Set([...get(downloads).map((d) => d.app_name), ...get(running)]);
  for (const g of list) {
    // A piece is updated with its game.
    if (!g.parent && wantsAutoUpdate(g, settingsDefault) && !busy.has(g.app_name) && hasUpdate(g.app_name, list, latest)) await startUpdate(g.app_name);
  }
}
