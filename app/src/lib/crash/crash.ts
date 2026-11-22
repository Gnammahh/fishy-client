import { invoke } from "@tauri-apps/api/core";

/** Longest error text sent to the log (spec §6f.2). */
const MAX = 500;

/** "<name>: <message>" for an Error, else its text, cut to 500 characters. */
function describe(e: unknown): string {
  const text = e instanceof Error ? `${e.name}: ${e.message}` : String(e);
  return Array.from(text).slice(0, MAX).join("");
}

/** Sends a screen error to the log (through the app's secret scrubber). Never throws. */
export function reportUiError(e: unknown): void {
  try {
    invoke("log_ui_error", { message: describe(e) }).catch(() => {});
  } catch {
    /* logging must never break anything */
  }
}

/** Logs errors that don't break a screen (a failed background promise). Returns the undo. */
export function installGlobalHandlers(target: Window = window): () => void {
  const onError = (e: Event) => reportUiError((e as ErrorEvent).error ?? (e as ErrorEvent).message);
  const onRejection = (e: Event) => reportUiError((e as PromiseRejectionEvent).reason);
  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onRejection);
  return () => {
    target.removeEventListener("error", onError);
    target.removeEventListener("unhandledrejection", onRejection);
  };
}
