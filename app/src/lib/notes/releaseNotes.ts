/**
 * The "What's new" notes, newest first, from release-notes.json (also read by
 * scripts/make-latest-json.mjs for the update file). Every release adds its entry at the top; a
 * test checks the top entry matches package.json's version, so a release can't forget it.
 */
import notes from "./release-notes.json";

export type ReleaseNote = { version: string; date: string; items: string[] };

export const RELEASE_NOTES: ReleaseNote[] = notes;

export const APP_VERSION = RELEASE_NOTES[0].version;

/** The notes open by themselves once per version. */
export function shouldAutoOpen(seen: string | null | undefined): boolean {
  return seen !== APP_VERSION;
}
