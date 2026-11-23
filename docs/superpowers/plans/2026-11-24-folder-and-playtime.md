# Game folder and playtime: plan (spec §6p)

Native, test-first. User left choices to Claude (2026-11-24).

1. **Running list durations** (`crates/fishy-core/src/running.rs`): `check` returns `Vec<(String, Duration)>` (time from `add` to `taken_at`). Test `closed_games_report_how_long_they_ran`. Update the app's `watch` loop.
2. **Store** (`crates/fishy-core/src/install/store.rs`): `InstalledGame.playtime_secs: u64` (`#[serde(default)]`); `add_playtime(app, secs)`; `finish_install` keeps it like `last_played`. Tests `playtime_adds_up_and_survives_an_update`, old file reads 0. `watch` calls `add_playtime` for each closed game.
3. **Open game folder** (`app/src-tauri/src/lib.rs`): `open_game_folder(app_name)`; pure `game_folder(installed, app) -> Result<PathBuf, String>` tested (`missing_folder_is_refused`).
4. **UI** (`installs.ts`, `LibraryPage.svelte`): `InstalledGame.playtime_secs?`, `formatPlaytime(secs)`, menu item + line. Tests in `library.test.ts` / `installs.test.ts`.
5. README line, review, release 1.6.0, push, hand check.
