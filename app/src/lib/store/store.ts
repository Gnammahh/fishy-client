import { invoke } from "@tauri-apps/api/core";

/** Mirrors fishy-core `FreeGame` (spec §6o). `date`: free until (now) or free from (next). */
export type FreeGame = { title: string; url: string; image: string | null; date: string; owned: boolean };
export type FreeGames = { now: FreeGame[]; next: FreeGame[] };

/** This week's free games. `force` skips the 10-minute memory (Try again). */
export async function loadFreeGames(force = false): Promise<FreeGames> {
  const g = await invoke<FreeGames>("store_free_games", { force });
  return { now: Array.isArray(g?.now) ? g.now : [], next: Array.isArray(g?.next) ? g.next : [] };
}

/** Opens Epic's store window, at `url` (an Epic page) or the store's front page. */
export function openStore(url?: string): Promise<void> {
  return invoke("store_open", { url: url ?? null });
}

/** "Nov 26" in the user's time zone. */
export function shortDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}
