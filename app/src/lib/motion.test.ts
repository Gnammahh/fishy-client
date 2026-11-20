import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";

type Listener = (e: { matches: boolean }) => void;
let listener: Listener | null = null;

beforeEach(() => {
  listener = null;
  vi.resetModules();
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: query === "(prefers-reduced-motion: reduce)",
    media: query,
    addEventListener: (_: string, fn: Listener) => {
      listener = fn;
    },
    removeEventListener: () => {
      listener = null;
    },
  })) as unknown as typeof window.matchMedia;
});

describe("reducedMotion", () => {
  it("reducedMotion follows the media query", async () => {
    const { reducedMotion } = await import("./motion");
    expect(get(reducedMotion)).toBe(true);

    const values: boolean[] = [];
    const unsub = reducedMotion.subscribe((v) => values.push(v));
    expect(listener).not.toBeNull();
    listener!({ matches: false });
    expect(values.at(-1)).toBe(false);
    unsub();
  });
});
