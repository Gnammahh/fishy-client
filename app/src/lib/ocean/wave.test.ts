import { describe, it, expect } from "vitest";
import { steppedWavePath } from "./wave";

const opts = { width: 640, height: 200, baseY: 100, amplitude: 20, wavelength: 320, step: 4 };

function vertices(d: string): [number, number][] {
  return [...d.matchAll(/(-?\d+(?:\.\d+)?),(-?\d+(?:\.\d+)?)/g)].map((m) => [Number(m[1]), Number(m[2])]);
}

/** y of the wave edge at x = the last edge vertex at or before x (bottom corners excluded). */
function edgeY(d: string, x: number): number {
  const edge = vertices(d).filter(([, y]) => y !== opts.height);
  return edge.filter(([vx]) => vx <= x).at(-1)![1];
}

describe("steppedWavePath", () => {
  it("path starts bottom-left and closes bottom-right", () => {
    const d = steppedWavePath(opts);
    expect(d.startsWith("M0,200")).toBe(true);
    expect(d.endsWith("L640,200 Z")).toBe(true);
  });

  it("all vertices snap to the step grid", () => {
    for (const [x, y] of vertices(steppedWavePath(opts))) {
      expect(x % 4).toBe(0);
      expect(y % 4).toBe(0);
    }
  });

  it("edge stays within amplitude", () => {
    const edge = vertices(steppedWavePath(opts)).filter(([, y]) => y !== 200);
    expect(edge.length).toBeGreaterThan(4);
    for (const [, y] of edge) {
      expect(y).toBeGreaterThanOrEqual(80);
      expect(y).toBeLessThanOrEqual(120);
    }
  });

  it("edge is periodic", () => {
    const d = steppedWavePath(opts);
    expect(edgeY(d, 320)).toBe(edgeY(d, 0));
    expect(edgeY(d, 640)).toBe(edgeY(d, 0));
  });

  it("rejects width that is not a whole number of wavelengths", () => {
    expect(() => steppedWavePath({ ...opts, width: 500 })).toThrow();
  });
});
