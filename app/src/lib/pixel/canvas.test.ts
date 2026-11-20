import { describe, it, expect } from "vitest";
import { drawGrid } from "./canvas";

function fakeCtx() {
  const calls: { fill: string; x: number; y: number; w: number; h: number }[] = [];
  const ctx = {
    fillStyle: "",
    fillRect(x: number, y: number, w: number, h: number) {
      calls.push({ fill: String(this.fillStyle), x, y, w, h });
    },
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, calls };
}

describe("drawGrid", () => {
  it("draws one scaled block per palette pixel", () => {
    const { ctx, calls } = fakeCtx();
    drawGrid(ctx, [".#", "o?"], { "#": "#aaa", o: "#bbb" }, 10, 20, 3);
    expect(calls).toEqual([
      { fill: "#aaa", x: 13, y: 20, w: 3, h: 3 },
      { fill: "#bbb", x: 10, y: 23, w: 3, h: 3 },
    ]);
  });

  it("flip mirrors horizontally and vflip mirrors vertically", () => {
    const { ctx, calls } = fakeCtx();
    drawGrid(ctx, ["#.", ".."], { "#": "#aaa" }, 0, 0, 2, { flip: true, vflip: true });
    expect(calls).toEqual([{ fill: "#aaa", x: 2, y: 2, w: 2, h: 2 }]);
  });
});
