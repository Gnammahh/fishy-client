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

  it("setting_on_forces_reduced_motion", async () => {
    window.matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      addEventListener: (_: string, fn: Listener) => (listener = fn),
      removeEventListener: () => (listener = null),
    })) as unknown as typeof window.matchMedia;
    const { reducedMotion } = await import("./motion");
    const { settings, DEFAULT_SETTINGS } = await import("./settings/settings");
    const values: boolean[] = [];
    const unsub = reducedMotion.subscribe((v) => values.push(v));
    expect(values.at(-1)).toBe(false);
    settings.set({ ...DEFAULT_SETTINGS, reduce_motion: true });
    expect(values.at(-1)).toBe(true);
    unsub();
  });

  it("setting_off_follows_windows", async () => {
    const { reducedMotion } = await import("./motion");
    const { settings, DEFAULT_SETTINGS } = await import("./settings/settings");
    settings.set({ ...DEFAULT_SETTINGS, reduce_motion: false });
    const values: boolean[] = [];
    const unsub = reducedMotion.subscribe((v) => values.push(v));
    expect(values.at(-1)).toBe(true); // Windows asks for it
    listener!({ matches: false });
    expect(values.at(-1)).toBe(false);
    unsub();
  });

  it("the_html_attribute_follows_reduced_motion_so_css_animations_stop_too", async () => {
    window.matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      addEventListener: (_: string, fn: Listener) => (listener = fn),
      removeEventListener: () => (listener = null),
    })) as unknown as typeof window.matchMedia;
    const { syncMotionAttribute } = await import("./motion");
    const { settings, DEFAULT_SETTINGS } = await import("./settings/settings");
    const html = document.documentElement;
    const stop = syncMotionAttribute(html);
    expect(html.hasAttribute("data-reduce-motion")).toBe(false);
    settings.set({ ...DEFAULT_SETTINGS, reduce_motion: true });
    expect(html.hasAttribute("data-reduce-motion")).toBe(true);
    settings.set({ ...DEFAULT_SETTINGS, reduce_motion: false });
    expect(html.hasAttribute("data-reduce-motion")).toBe(false);
    stop();
  });
});

describe("paused animations (spec §6r)", () => {
  it("data_paused_follows_minimized_and_running_games", async () => {
    const { syncPausedAttribute, minimized } = await import("./motion");
    const { running } = await import("./install/installs");
    const html = document.createElement("html");
    const stop = syncPausedAttribute(html);
    expect(html.hasAttribute("data-paused")).toBe(false);
    minimized.set(true);
    expect(html.hasAttribute("data-paused")).toBe(true);
    minimized.set(false);
    expect(html.hasAttribute("data-paused")).toBe(false);
    running.set(["KelpRunner"]);
    expect(html.hasAttribute("data-paused")).toBe(true);
    running.set([]);
    expect(html.hasAttribute("data-paused")).toBe(false);
    stop();
  });
});

describe("watchMinimized (spec §6r)", () => {
  it("does_not_crash_without_a_tauri_backend_like_the_dev_preview", async () => {
    const { clearMocks } = await import("@tauri-apps/api/mocks");
    clearMocks();
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    const rejections: unknown[] = [];
    const onRejection = (reason: unknown) => rejections.push(reason);
    // Vitest runs on Node, where a promise nobody catches ends up here.
    type Proc = { on(e: string, f: (r: unknown) => void): void; off(e: string, f: (r: unknown) => void): void };
    const proc = (globalThis as unknown as { process: Proc }).process;
    proc.on("unhandledRejection", onRejection);
    try {
      const { watchMinimized } = await import("./motion");
      let stop: (() => void) | undefined;
      expect(() => (stop = watchMinimized())).not.toThrow();
      await new Promise((r) => setTimeout(r, 20));
      stop?.();
      expect(rejections).toEqual([]);
    } finally {
      proc.off("unhandledRejection", onRejection);
    }
  });

  it("the_window_minimized_event_updates_the_minimized_store", async () => {
    const { mockIPC, clearMocks } = await import("@tauri-apps/api/mocks");
    mockIPC(() => null, { shouldMockEvents: true });
    try {
      const { emit } = await import("@tauri-apps/api/event");
      const { watchMinimized, minimized } = await import("./motion");
      const stop = watchMinimized();
      await new Promise((r) => setTimeout(r, 0));
      await emit("window-minimized", true);
      expect(get(minimized)).toBe(true);
      await emit("window-minimized", false);
      expect(get(minimized)).toBe(false);
      stop();
    } finally {
      clearMocks();
    }
  });
});
