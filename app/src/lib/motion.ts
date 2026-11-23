import { derived, readable, writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { running } from "./install/installs";
import { settings } from "./settings/settings";

const QUERY = "(prefers-reduced-motion: reduce)";

/** True when Windows "Animation effects: Off" asks for reduced motion. Updates live. */
const windowsAsks = readable<boolean>(
  typeof window !== "undefined" && window.matchMedia ? window.matchMedia(QUERY).matches : false,
  (set) => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia(QUERY);
    set(mq.matches);
    const onChange = (e: { matches: boolean }) => set(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  },
);

/** Calm animations when Windows asks for it or the Reduce motion setting is on (spec §6f.1). */
export const reducedMotion = derived([windowsAsks, settings], ([$windows, $settings]) => $windows || $settings.reduce_motion);

/**
 * Mirrors `reducedMotion` onto `data-reduce-motion` on `target` (the <html> element), so animations
 * that live only in CSS calm down for the setting too, not just for Windows. Returns the unsubscribe.
 */
export function syncMotionAttribute(target: HTMLElement = document.documentElement): () => void {
  return reducedMotion.subscribe((on) => target.toggleAttribute("data-reduce-motion", on));
}

/** Fishy Client's window is minimized (the `window-minimized` event from Rust, spec §6r). */
export const minimized = writable(false);

/** The waves and sidebar fish pause while minimized or while a game from Fishy Client runs. */
export const pausedMotion = derived([minimized, running], ([$minimized, $running]) => $minimized || $running.length > 0);

/** Mirrors `pausedMotion` onto `data-paused` on `target` (the <html> element). Returns the unsubscribe. */
export function syncPausedAttribute(target: HTMLElement = document.documentElement): () => void {
  return pausedMotion.subscribe((on) => target.toggleAttribute("data-paused", on));
}

/** Follows the window's minimized state sent by Rust. Returns the unsubscribe. */
export function watchMinimized(): () => void {
  let stop: (() => void) | null = null;
  let gone = false;
  listen<boolean>("window-minimized", (e) => minimized.set(e.payload)).then((un) => {
    if (gone) un();
    else stop = un;
  });
  return () => {
    gone = true;
    stop?.();
  };
}
