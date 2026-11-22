import { invoke } from "@tauri-apps/api/core";
import { get, writable } from "svelte/store";

/** Mirrors fishy-core `Settings` (spec §6e). */
export type Settings = {
  auto_update: boolean;
  warn_outdated_play: boolean;
  /** Where the Install box starts; null means %USERPROFILE%\Games. */
  install_folder: string | null;
  /** MB/s, 1 to 1000; null means no limit. */
  speed_limit_mbps: number | null;
  reduce_motion: boolean;
  /** The Library's order (spec §6f.3). */
  library_sort: LibrarySort;
  /** What happens to the window after Play (spec §6f.4). */
  when_game_starts: WhenGameStarts;
  /** The version whose "What's new" notes were last shown (null: never). */
  notes_seen: string | null;
};

export type LibrarySort = "az" | "za" | "installed" | "size" | "played";
export type WhenGameStarts = "keep" | "minimize";

export const DEFAULT_SETTINGS: Settings = {
  auto_update: false,
  warn_outdated_play: true,
  install_folder: null,
  speed_limit_mbps: null,
  reduce_motion: false,
  library_sort: "az",
  when_game_starts: "keep",
  notes_seen: null,
};
const SAVE_ERROR = "Couldn't save your settings.";

export const settings = writable<Settings>({ ...DEFAULT_SETTINGS });

/** Back to the defaults (between tests). */
export function resetSettings() {
  settings.set({ ...DEFAULT_SETTINGS });
}

const withDefaults = (s: unknown): Settings => ({ ...DEFAULT_SETTINGS, ...(s && typeof s === "object" ? (s as Partial<Settings>) : {}) });

/** Reads the saved settings. Keeps what's in memory if that fails. */
export async function loadSettings(): Promise<Settings> {
  try {
    const s = withDefaults(await invoke<Settings | null>("settings_get"));
    settings.set(s);
    return s;
  } catch {
    return get(settings);
  }
}

/** Saves a change. Returns an error message, or null when it worked. */
export async function saveSettings(patch: Partial<Settings>): Promise<string | null> {
  const next = { ...get(settings), ...patch };
  try {
    const saved = await invoke<Settings | null>("settings_set", { value: next });
    settings.set(withDefaults(saved ?? next));
    return null;
  } catch {
    return SAVE_ERROR;
  }
}
