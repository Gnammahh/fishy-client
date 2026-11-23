import { invoke } from "@tauri-apps/api/core";

/** One achievement, as the `game_achievements` command sends it (spec §6q). */
export type Achievement = {
  name: string;
  title: string;
  description: string;
  icon_url: string | null;
  unlocked: boolean;
  unlocked_at: string | null;
  hidden: boolean;
  /** Percent of players who have it. */
  rarity: number | null;
};

/** Already sorted: unlocked newest first, then locked most common first. */
export type Achievements = { total: number; unlocked: number; items: Achievement[] };

export type AchievementFilter = "all" | "unlocked" | "locked";

export function loadAchievements(appName: string): Promise<Achievements> {
  return invoke<Achievements>("game_achievements", { appName });
}

export function filterAchievements(items: Achievement[], f: AchievementFilter): Achievement[] {
  if (f === "all") return items;
  return items.filter((i) => i.unlocked === (f === "unlocked"));
}

/** "Unlocked 12 Mar 2025", or just "Unlocked" when Epic gives no usable date. */
export function formatUnlocked(iso: string | null): string {
  const d = iso ? new Date(iso) : null;
  if (!d || Number.isNaN(d.getTime())) return "Unlocked";
  const date = d.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
  return `Unlocked ${date}`;
}

/** "4.2% of players have it", or null when Epic gives no rarity. */
export function formatRarity(p: number | null): string | null {
  if (p === null || !Number.isFinite(p)) return null;
  return `${p.toFixed(1)}% of players have it`;
}
