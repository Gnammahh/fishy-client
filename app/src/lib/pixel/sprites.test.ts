import { describe, it, expect } from "vitest";
import {
  FISH_SWIM,
  FISH_MOUTH_OPEN,
  FISH_MOUTH_CLOSED,
  FISH_DEAD,
  PELLET,
  BUBBLE,
  PUFF,
  ICON_FISH,
} from "./sprites";
import type { Grid } from "./grid";

const dims = (g: Grid) => ({ w: g[0].length, h: g.length, uniform: g.every((r) => r.length === g[0].length) });

describe("sprites", () => {
  it("all fish frames are 16x16", () => {
    for (const g of [FISH_SWIM[0], FISH_SWIM[1], FISH_MOUTH_OPEN, FISH_MOUTH_CLOSED, FISH_DEAD]) {
      expect(dims(g)).toEqual({ w: 16, h: 16, uniform: true });
    }
  });

  it("fish frames face right and have an eye", () => {
    for (const g of [FISH_SWIM[0], FISH_SWIM[1], FISH_MOUTH_OPEN, FISH_MOUTH_CLOSED]) {
      const eyes: number[] = [];
      g.forEach((row) => [...row].forEach((ch, x) => ch === "e" && eyes.push(x)));
      expect(eyes).toHaveLength(1);
      expect(eyes[0]).toBeGreaterThanOrEqual(8);
    }
  });

  it("pellet is 4x4, bubble is 4x4, puff is 6x6, icon fish is 14x10", () => {
    expect(dims(PELLET)).toEqual({ w: 4, h: 4, uniform: true });
    expect(dims(BUBBLE)).toEqual({ w: 4, h: 4, uniform: true });
    expect(dims(PUFF)).toEqual({ w: 6, h: 6, uniform: true });
    expect(dims(ICON_FISH)).toEqual({ w: 14, h: 10, uniform: true });
  });
});
