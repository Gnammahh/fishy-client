/**
 * The "What's new" notes, newest first. Every release adds its entry at the top; a test checks the
 * top entry matches package.json's version, so a release can't forget it.
 */
export type ReleaseNote = { version: string; date: string; items: string[] };

export const RELEASE_NOTES: ReleaseNote[] = [
  {
    version: "1.1.0",
    date: "2026-11-22",
    items: [
      "Games you already installed with Epic's launcher can be added without downloading them again: press Add them at the top of the Library.",
      "If some of their files are missing or damaged, Download them fetches only those.",
      "The ⋯ menu of an installed game shows how much space it takes.",
      "The Install box warns about folders that need administrator rights, such as C:\\Program Files\\Epic Games.",
      "Games you haven't installed look a bit grey in the Library.",
      "The Install box says that Epic's launcher can't see games installed with Fishy Client.",
      "This What's new box.",
    ],
  },
  {
    version: "1.0.0",
    date: "2026-11-22",
    items: [
      "The first version: sign in with Epic, see your Library, install, play and update your games.",
      "Pause and resume downloads, with a speed limit if you want one.",
      "Library filters and sorting, and a Running badge while a game is open.",
      "Settings, logs without secrets, and Delete all my data.",
      "A Windows installer (.msi).",
    ],
  },
];

export const APP_VERSION = RELEASE_NOTES[0].version;

/** The notes open by themselves once per version. */
export function shouldAutoOpen(seen: string | null | undefined): boolean {
  return seen !== APP_VERSION;
}
