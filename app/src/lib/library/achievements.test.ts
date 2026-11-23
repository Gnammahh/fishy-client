import { describe, it, expect } from "vitest";
import { filterAchievements, formatRarity, formatUnlocked, type Achievement } from "./achievements";

const item = (name: string, unlocked: boolean): Achievement => ({
  name,
  title: name,
  description: "",
  icon_url: null,
  unlocked,
  unlocked_at: null,
  hidden: false,
  rarity: null,
});

describe("achievements helpers", () => {
  it("formats_the_unlock_date", () => {
    expect(formatUnlocked("2025-03-12T18:00:00.000Z")).toBe("Unlocked 12 Mar 2025");
    expect(formatUnlocked(null)).toBe("Unlocked");
    expect(formatUnlocked("garbage")).toBe("Unlocked");
  });

  it("formats_the_rarity", () => {
    expect(formatRarity(4.2)).toBe("4.2% of players have it");
    expect(formatRarity(60)).toBe("60.0% of players have it");
    expect(formatRarity(null)).toBeNull();
    expect(formatRarity(NaN)).toBeNull();
  });

  it("filters_and_keeps_the_order", () => {
    const items = [item("a", true), item("b", false), item("c", true)];
    expect(filterAchievements(items, "all").map((i) => i.name)).toEqual(["a", "b", "c"]);
    expect(filterAchievements(items, "unlocked").map((i) => i.name)).toEqual(["a", "c"]);
    expect(filterAchievements(items, "locked").map((i) => i.name)).toEqual(["b"]);
  });
});
