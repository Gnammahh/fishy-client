import { readable } from "svelte/store";

const QUERY = "(prefers-reduced-motion: reduce)";

/** True when the user (or Windows "Animation effects: Off") asks for reduced motion. Updates live. */
export const reducedMotion = readable<boolean>(
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
