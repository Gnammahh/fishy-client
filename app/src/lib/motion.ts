import { derived, readable } from "svelte/store";
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
