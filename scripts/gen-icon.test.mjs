// @vitest-environment node
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { renderIcon } from "./gen-icon.mjs";

const text = readFileSync(fileURLToPath(new URL("../assets/sprites/fish-icon.txt", import.meta.url)), "utf8");

function px(png, x, y) {
  const i = (png.width * y + x) * 4;
  return [png.data[i], png.data[i + 1], png.data[i + 2], png.data[i + 3]];
}

describe("renderIcon", () => {
  it("renderIcon is square at requested size", () => {
    const png = renderIcon(text, 1024);
    expect(png.width).toBe(1024);
    expect(png.height).toBe(1024);
  });

  it("corners are transparent", () => {
    const png = renderIcon(text, 1024);
    expect(px(png, 0, 0)[3]).toBe(0);
    expect(px(png, 3 * 64 + 32, 32)).toEqual([0, 0, 0, 255]);
  });

  it("tile is black and fish is aqua", () => {
    const png = renderIcon(text, 1024);
    expect(px(png, 32, 8 * 64 + 32)).toEqual([0, 0, 0, 255]);
    expect(px(png, 7 * 64 + 32, 3 * 64 + 32)).toEqual([79, 227, 209, 255]);
  });

  it("fish is centered", () => {
    const png = renderIcon(text, 16);
    let minX = 99, maxX = -1, minY = 99, maxY = -1;
    for (let y = 0; y < 16; y++)
      for (let x = 0; x < 16; x++) {
        const [r, g, b] = px(png, x, y);
        if (r === 79 && g === 227 && b === 209) {
          minX = Math.min(minX, x); maxX = Math.max(maxX, x);
          minY = Math.min(minY, y); maxY = Math.max(maxY, y);
        }
      }
    expect([minX, maxX, minY, maxY]).toEqual([1, 14, 3, 12]);
  });

  it("rejects sizes that are not a multiple of 16", () => {
    expect(() => renderIcon(text, 100)).toThrow("size must be a multiple of 16");
  });
});
