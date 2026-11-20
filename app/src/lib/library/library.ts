import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";

/** Mirrors the Rust `Game` (fishy-core library). */
export type Game = {
  app_name: string;
  namespace: string;
  catalog_item_id: string;
  title: string;
  cover_url: string | null;
};

export type CachedLibrary = { updated_at: string; games: Game[] };

export type LibraryView = {
  /** loading: nothing to show yet; ready: games shown (maybe from cache); error: nothing to show and the fetch failed;
   *  offline: no saved library and Epic can't be reached (the shell's offline banner has "Try again") */
  status: "loading" | "ready" | "error" | "offline";
  games: Game[];
  /** True while a background refresh runs over cached games. */
  updating: boolean;
  /** User-facing error ("" when none). */
  error: string;
};

const LIBRARY_ERROR = "Couldn't load your library from Epic.";
const initial: LibraryView = { status: "loading", games: [], updating: false, error: "" };

export const library = writable<LibraryView>({ ...initial });

/** Forget the in-memory library (used on Log out, and between tests). */
export function resetLibrary() {
  library.set({ ...initial });
}

/** Case-insensitive title search; an empty query shows everything. */
export function filterGames(games: Game[], query: string): Game[] {
  const q = query.trim().toLowerCase();
  return q ? games.filter((g) => g.title.toLowerCase().includes(q)) : games;
}

let current: LibraryView = { ...initial };
library.subscribe((v) => (current = v));

/** Shows the saved library at once (if any), then refreshes it from Epic, unless offline. */
export async function loadLibrary(offline = false) {
  let cached: CachedLibrary | null = null;
  try {
    cached = await invoke<CachedLibrary | null>("library_cached");
  } catch {
    /* no cache available */
  }
  const hasCache = !!cached && Array.isArray(cached.games);
  if (offline) {
    // Epic is unreachable: show what's saved and don't add a second error next to the offline banner.
    library.set(hasCache ? { status: "ready", games: cached!.games, updating: false, error: "" } : { ...initial, status: "offline" });
    return;
  }
  if (hasCache) {
    library.set({ status: "ready", games: cached!.games, updating: true, error: "" });
  } else if (current.status !== "ready") {
    library.set({ ...initial });
  }
  await refreshLibrary();
}

/** Fetches from Epic. Keeps games already on screen if it fails. */
export async function refreshLibrary() {
  const hadGames = current.status === "ready";
  library.update((v) => ({ ...v, updating: hadGames, error: "", status: hadGames ? "ready" : "loading" }));
  try {
    const fresh = await invoke<CachedLibrary>("library_refresh");
    if (!fresh || !Array.isArray(fresh.games)) throw LIBRARY_ERROR;
    library.set({ status: "ready", games: fresh.games, updating: false, error: "" });
  } catch {
    library.update((v) =>
      hadGames ? { ...v, updating: false, error: LIBRARY_ERROR } : { status: "error", games: [], updating: false, error: LIBRARY_ERROR },
    );
  }
}
