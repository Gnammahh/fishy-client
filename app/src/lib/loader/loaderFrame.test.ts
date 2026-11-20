import { describe, it, expect } from "vitest";
import { fishLoaderFrame, CYCLE_MS } from "./loaderFrame";

describe("fishLoaderFrame", () => {
  it("cycle is 1800 ms", () => {
    expect(CYCLE_MS).toBe(1800);
  });

  it("starts with pellet at top and fish centred", () => {
    expect(fishLoaderFrame(0, 0.8)).toEqual({
      phase: "fall",
      fishX: 0.5,
      facing: "right",
      mouthOpen: false,
      pelletY: 0,
      pelletVisible: true,
    });
  });

  it("fish faces left for a pellet on the left", () => {
    expect(fishLoaderFrame(700, 0.2).facing).toBe("left");
  });

  it("fish reaches pellet and eats", () => {
    const f = fishLoaderFrame(1000, 0.3);
    expect(f.phase).toBe("eat");
    expect(f.fishX).toBe(0.3);
    expect(f.mouthOpen).toBe(true);
  });

  it("pellet gone after bite", () => {
    expect(fishLoaderFrame(1150, 0.3).pelletVisible).toBe(false);
  });

  it("returns to centre and faces it", () => {
    const f = fishLoaderFrame(1799, 0.3);
    expect(f.phase).toBe("return");
    expect(Math.abs(f.fishX - 0.5)).toBeLessThanOrEqual(0.01);
    expect(f.facing).toBe("right");
  });

  it("cycle repeats", () => {
    expect(fishLoaderFrame(1800 + 100, 0.6)).toEqual(fishLoaderFrame(100, 0.6));
  });

  it("a pellet dead centre keeps the fish facing right", () => {
    expect(fishLoaderFrame(1500, 0.5).facing).toBe("right");
  });
});
