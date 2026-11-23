import { invoke } from "@tauri-apps/api/core";
import type { InstalledGame } from "$lib/install/installs";
import type { LibrarySort } from "$lib/settings/settings";
import { get, writable } from "svelte/store";
import { autoUpdate, running } from "$lib/install/installs";
import { loadSettings, settings } from "$lib/settings/settings";

/** Mirrors the Rust `Game` (fishy-core library). */
export type Game = {
  app_name: string;
  namespace: string;
  catalog_item_id: string;
  title: string;
  cover_url: string | null;
  /** False when Epic has no Windows download for it. */
  installable: boolean;
  /** From the catalog (spec §6d): set when another store's app installs and starts it. */
  launch?: { third_party_store?: string | null };
};

/** The other store that installs this game (spec §6l), or null. Same names as Play's message. */
export function otherStore(g: Game): string | null {
  const raw = g.launch?.third_party_store;
  if (g.installable || !raw) return null;
  const key = raw.toLowerCase();
  if (key === "origin" || key === "the ea app") return "the EA app";
  if (key === "ubisoftconnect" || key === "uplay") return "Ubisoft Connect";
  return raw;
}

/** `latest_versions`: Epic's newest build version per app name (spec §6e). */
export type CachedLibrary = { updated_at: string; games: Game[]; latest_versions?: Record<string, string> };

export type LibraryView = {
  /** loading: nothing to show yet; ready: games shown (maybe from cache); error: nothing to show and the fetch failed;
   *  offline: no saved library and Epic can't be reached (the shell's offline banner has "Try again") */
  status: "loading" | "ready" | "error" | "offline";
  games: Game[];
  /** True while a background refresh runs over cached games. */
  updating: boolean;
  /** User-facing error ("" when none). */
  error: string;
  /** Epic's newest version per app name, for update checks. */
  latestVersions: Record<string, string>;
};

const LIBRARY_ERROR = "Couldn't load your library from Epic. Check your internet, then press Try again.";
/** The Rust side's message when the sign-in expired (spec §6f.2): the app goes to Login. */
const EXPIRED = "Your sign-in has expired. Please sign in again.";
const initial: LibraryView = { status: "loading", games: [], updating: false, error: "", latestVersions: {} };
const versionsOf = (c: CachedLibrary | null) => (c?.latest_versions && typeof c.latest_versions === "object" ? c.latest_versions : {});

export const library = writable<LibraryView>({ ...initial });

/** Forget the in-memory library (used on Log out, and between tests). */
export function resetLibrary() {
  library.set({ ...initial });
  online = false;
}

/** The last Library refresh reached Epic (so an update can be fetched now). */
let online = false;

const byTitle = (a: Game, b: Game) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" });

/**
 * The Library's order (spec §6f.3). "installed" and "size" put installed games first (newest or
 * biggest at the top), then the rest A–Z. Returns a new list.
 */
export function sortGames(games: Game[], sort: LibrarySort, installed: InstalledGame[]): Game[] {
  const list = [...games];
  if (sort === "az") return list.sort(byTitle);
  if (sort === "za") return list.sort((a, b) => byTitle(b, a));
  const info = new Map(installed.map((i) => [i.app_name, i]));
  if (sort === "played") {
    // Played games newest first, then other installed games, then the rest; ties A-Z.
    const rank = (g: Game) => (info.get(g.app_name)?.last_played ? 0 : info.has(g.app_name) ? 1 : 2);
    const when = (g: Game) => Date.parse(info.get(g.app_name)?.last_played ?? "") || 0;
    return list.sort((a, b) => rank(a) - rank(b) || when(b) - when(a) || byTitle(a, b));
  }
  const key = (g: Game): number | null => {
    const i = info.get(g.app_name);
    if (!i) return null;
    return sort === "installed" ? Date.parse(i.installed_at) || 0 : i.install_size;
  };
  return list.sort((a, b) => {
    const ka = key(a);
    const kb = key(b);
    if (ka !== null && kb !== null && ka !== kb) return kb - ka;
    if (ka !== null && kb === null) return -1;
    if (ka === null && kb !== null) return 1;
    return byTitle(a, b);
  });
}

/** Case-insensitive title search; an empty query shows everything. */
export function filterGames(games: Game[], query: string): Game[] {
  const q = query.trim().toLowerCase();
  return q ? games.filter((g) => g.title.toLowerCase().includes(q)) : games;
}

let current: LibraryView = { ...initial };
library.subscribe((v) => (current = v));

// An update skipped while its game ran is queued once the game closes (spec §6f.4).
let lastRunning: string[] = [];
running.subscribe((now) => {
  const closed = lastRunning.some((a) => !now.includes(a));
  lastRunning = now;
  if (closed && online) void autoUpdate(current.latestVersions, get(settings).auto_update);
});

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
    online = false;
    // Epic is unreachable: show what's saved and don't add a second error next to the offline banner.
    library.set(
      hasCache
        ? { status: "ready", games: cached!.games, updating: false, error: "", latestVersions: versionsOf(cached) }
        : { ...initial, status: "offline" },
    );
    return;
  }
  if (hasCache) {
    library.set({ status: "ready", games: cached!.games, updating: true, error: "", latestVersions: versionsOf(cached) });
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
    const latestVersions = versionsOf(fresh);
    library.set({ status: "ready", games: fresh.games, updating: false, error: "", latestVersions });
    online = true;
    await autoUpdate(latestVersions, (await loadSettings()).auto_update);
  } catch (e) {
    online = false;
    // Expired: the old account's games are gone and Login shows; leave a clean library behind.
    if (e === EXPIRED) return resetLibrary();
    library.update((v) =>
      hadGames ? { ...v, updating: false, error: LIBRARY_ERROR } : { ...initial, status: "error", error: LIBRARY_ERROR },
    );
  }
}
