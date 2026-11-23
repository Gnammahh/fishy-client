import { describe, it, expect } from "vitest";

// Every box that dims the app (What's new, Install, Uninstall, Manage…) must start under
// Fishy's own title bar (spec §6s), or a tall box slides under it and its top is hidden.
const sources = import.meta.glob("../**/*.svelte", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

/** The CSS rule `.overlay { … }` of a component, if it has one. */
const overlayRule = (src: string) => /\.overlay\s*\{([^}]*)\}/.exec(src)?.[1] ?? null;

describe("dialog overlays", () => {
  const overlays = Object.entries(sources).filter(([, src]) => overlayRule(src)?.includes("position: fixed"));

  it("there are dialogs to check", () => {
    expect(overlays.length).toBeGreaterThanOrEqual(7);
  });

  it.each(overlays.map(([file]) => file))("%s starts under the title bar and fits below it", (file) => {
    const src = sources[file];
    expect(overlayRule(src)).toMatch(/inset:\s*var\(--titlebar\)\s+0\s+0\s+0;/);
    // The box itself never grows taller than the space under the title bar, padding and
    // border included (without border-box they're added on top and it sticks out again).
    const box = /\{([^}]*max-height:\s*calc\(100vh - var\(--titlebar\) - 32px\)[^}]*)\}/.exec(src)?.[1];
    expect(box).toBeTruthy();
    expect(box).toMatch(/box-sizing:\s*border-box;/);
  });
});
