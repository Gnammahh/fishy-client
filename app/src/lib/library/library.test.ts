import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import LibraryPage from "./LibraryPage.svelte";
import { filterGames, sortGames, resetLibrary, refreshLibrary, loadLibrary, library, type Game } from "./library";
import { get } from "svelte/store";
import { downloads, installed, running, resetInstalls, type DownloadItem, type InstalledGame } from "$lib/install/installs";
import { settings, resetSettings, DEFAULT_SETTINGS } from "$lib/settings/settings";

const installedGame = (app_name: string): InstalledGame => ({
  app_name,
  title: app_name,
  version: "1",
  install_path: "C:\\Games\\" + app_name,
  executable: "x.exe",
  launch_parameters: "",
  install_size: 1,
  installed_at: "2026-11-21T00:00:00Z",
});
const downloadItem = (app_name: string, state: DownloadItem["state"]): DownloadItem => ({
  app_name,
  title: app_name,
  state,
  downloaded: 1,
  total: 2,
  bytes_per_sec: 0,
  error: null,
});

const game = (title: string, cover: string | null = null): Game => ({
  app_name: title.replace(/\W/g, ""),
  namespace: "ns",
  catalog_item_id: title.toLowerCase().replace(/\W/g, "-"),
  title,
  cover_url: cover,
  installable: true,
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function mockCommands(handlers: Record<string, (args?: any) => unknown>) {
  const calls: string[] = [];
  mockIPC((cmd, args) => {
    calls.push(cmd);
    return handlers[cmd]?.(args);
  });
  return calls;
}

afterEach(() => {
  vi.useRealTimers();
  cleanup();
  clearMocks();
  resetLibrary();
  resetInstalls();
  resetSettings();
});

describe("sortGames", () => {
  const ins = (app_name: string, installed_at: string, install_size: number): InstalledGame => ({
    ...installedGame(app_name),
    installed_at,
    install_size,
  });
  const games = [game("Coral Kart"), game("abyssal Lanterns"), game("Kelp Runner"), game("Brine & Bones")];
  const installedList = [ins("KelpRunner", "2026-11-01T00:00:00Z", 5), ins("BrineBones", "2026-11-20T00:00:00Z", 2)];
  const titles = (gs: Game[]) => gs.map((g) => g.title);

  it("a_to_z_ignores_letter_case", () => {
    expect(titles(sortGames(games, "az", installedList))).toEqual(["abyssal Lanterns", "Brine & Bones", "Coral Kart", "Kelp Runner"]);
  });

  it("z_to_a", () => {
    expect(titles(sortGames(games, "za", installedList))).toEqual(["Kelp Runner", "Coral Kart", "Brine & Bones", "abyssal Lanterns"]);
  });

  it("recently_installed_puts_installed_games_first_newest_on_top_then_a_to_z", () => {
    expect(titles(sortGames(games, "installed", installedList))).toEqual(["Brine & Bones", "Kelp Runner", "abyssal Lanterns", "Coral Kart"]);
  });

  it("size_on_disk_puts_the_biggest_installed_game_first_then_a_to_z", () => {
    expect(titles(sortGames(games, "size", installedList))).toEqual(["Kelp Runner", "Brine & Bones", "abyssal Lanterns", "Coral Kart"]);
  });

  it("sorts_recently_played_then_other_installed_then_the_rest", () => {
    const list = [game("Abalone"), game("Barnacle"), game("coral"), game("Dory")];
    const ins2 = [
      { ...installedGame("Abalone"), last_played: "2026-11-01T00:00:00Z" },
      installedGame("coral"),
      { ...installedGame("Dory"), last_played: "2026-11-20T00:00:00Z" },
    ];
    const copy = [...list];
    expect(titles(sortGames(list, "played", ins2))).toEqual(["Dory", "Abalone", "coral", "Barnacle"]);
    expect(list).toEqual(copy);
  });

  it("does_not_change_the_list_it_was_given", () => {
    const copy = [...games];
    sortGames(games, "za", installedList);
    expect(games).toEqual(copy);
  });
});

describe("refreshLibrary", () => {
  it("no_update_is_tried_after_a_game_closes_while_offline", async () => {
    const started: unknown[] = [];
    mockCommands({
      library_cached: () => ({ updated_at: "x", games: [game("Kelp Runner")], latest_versions: { KelpRunner: "2" } }),
      installed_list: () => [installedGame("KelpRunner")],
      update_start: (a) => {
        started.push(a);
        return { items: [] };
      },
    });
    settings.set({ ...DEFAULT_SETTINGS, auto_update: true });
    await loadLibrary(true);
    running.set(["KelpRunner"]);
    running.set([]);
    await new Promise((r) => setTimeout(r, 20));
    expect(started).toEqual([]);
  });

  it("an_expired_sign_in_leaves_a_clean_library_not_an_error", async () => {
    mockCommands({
      library_refresh: () => {
        throw "Your sign-in has expired. Please sign in again.";
      },
    });
    await refreshLibrary();
    expect(get(library)).toEqual({ status: "loading", games: [], updating: false, error: "", latestVersions: {} });
  });
});

describe("filterGames", () => {
  it("matches titles case-insensitively and ignores surrounding spaces", () => {
    const games = [game("Tidepool Odyssey"), game("Kelp Runner")];
    expect(filterGames(games, "  KELP ").map((g) => g.title)).toEqual(["Kelp Runner"]);
    expect(filterGames(games, "")).toHaveLength(2);
  });
});

describe("LibraryPage", () => {
  it("shows_the_cached_library_then_the_refreshed_one", async () => {
    mockCommands({
      library_cached: () => ({ updated_at: "2026-11-20T20:00:00Z", games: [game("Old Game")] }),
      library_refresh: () => ({ updated_at: "2026-11-20T21:00:00Z", games: [game("Old Game"), game("New Game")] }),
    });
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("New Game")).toBeTruthy();
    expect(screen.getByText("2 games")).toBeTruthy();
  });

  it("first_load_shows_the_loader_message", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => new Promise(() => {}) });
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Fetching your library…")).toBeTruthy();
  });

  it("empty_library_shows_the_empty_state", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => ({ updated_at: "x", games: [] }) });
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("No fish in this pond yet. Games you own on Epic show up here.")).toBeTruthy();
  });

  it("error_without_cache_offers_try_again", async () => {
    let fail = true;
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => {
        if (fail) throw "Couldn't load your library from Epic.";
        return { updated_at: "x", games: [game("Back Again")] };
      },
    });
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Couldn't load your library from Epic. Check your internet, then press Try again.")).toBeTruthy();
    fail = false;
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Back Again")).toBeTruthy();
    expect(calls.filter((c) => c === "library_refresh")).toHaveLength(2);
  });

  it("error_with_cache_keeps_the_games_and_shows_a_banner", async () => {
    mockCommands({
      library_cached: () => ({ updated_at: "x", games: [game("Still Here")] }),
      library_refresh: () => {
        throw "Couldn't load your library from Epic.";
      },
    });
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Couldn't load your library from Epic. Check your internet, then press Try again.")).toBeTruthy();
    expect(screen.getByText("Still Here")).toBeTruthy();
  });

  it("offline_shows_cached_games_without_asking_epic", async () => {
    const calls = mockCommands({ library_cached: () => ({ updated_at: "x", games: [game("Saved Game")] }) });
    render(LibraryPage, { name: "FishFan", offline: true });
    expect(await screen.findByText("Saved Game")).toBeTruthy();
    expect(calls).not.toContain("library_refresh");
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("offline_without_cache_explains_instead_of_erroring", async () => {
    const calls = mockCommands({ library_cached: () => null });
    render(LibraryPage, { name: "FishFan", offline: true });
    expect(await screen.findByText("Your library shows up here once Fishy Client can reach Epic.")).toBeTruthy();
    expect(calls).not.toContain("library_refresh");
  });

  it("search_filters_the_grid", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => ({ updated_at: "x", games: [game("Tidepool Odyssey"), game("Kelp Runner")] }) });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    await fireEvent.input(screen.getByLabelText("Search your library"), { target: { value: "tide" } });
    await waitFor(() => expect(screen.queryByText("Kelp Runner")).toBeNull());
    expect(screen.getByText("Tidepool Odyssey")).toBeTruthy();
  });

  it("installed_games_get_a_badge_and_their_own_tab", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => ({ updated_at: "x", games: [game("Tidepool Odyssey"), game("Kelp Runner")] }) });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    expect(screen.getByText("Installed", { selector: ".badge" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Install Tidepool Odyssey" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Install Kelp Runner" })).toBeNull();
    await fireEvent.click(screen.getByRole("tab", { name: "Installed (1)" }));
    await waitFor(() => expect(screen.queryByText("Tidepool Odyssey")).toBeNull());
    expect(screen.getByText("Kelp Runner")).toBeTruthy();
    installed.set([]);
    expect(await screen.findByText("No games installed yet.")).toBeTruthy();
  });

  it("games_without_a_windows_build_get_their_own_tab_with_one_explanation", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({
        updated_at: "x",
        games: [game("Tidepool Odyssey"), { ...game("Mac Minnow"), installable: false }, { ...game("Phone Plaice"), installable: false }],
      }),
    });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Tidepool Odyssey");
    // "All" only has games you can install, and the count matches it.
    expect(screen.queryByText("Mac Minnow")).toBeNull();
    expect(screen.getByText("1 game")).toBeTruthy();
    await fireEvent.click(screen.getByRole("tab", { name: "Not on Windows (2)" }));
    await waitFor(() => expect(screen.queryByText("Tidepool Odyssey")).toBeNull());
    expect(screen.getByText("Mac Minnow")).toBeTruthy();
    expect(screen.getByText("Phone Plaice")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Install / })).toBeNull();
    // One explanation for the whole tab, not one per game.
    expect(screen.getAllByText(/Epic has no Windows download for these games/)).toHaveLength(1);
  });

  it("the_not_on_windows_tab_is_hidden_when_every_game_runs_on_windows", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => ({ updated_at: "x", games: [game("Tidepool Odyssey")] }) });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Tidepool Odyssey");
    expect(screen.queryByRole("tab", { name: /Not on Windows/ })).toBeNull();
  });

  it("install_button_opens_the_install_box", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Tidepool Odyssey")] }),
      install_options: () => new Promise(() => {}),
    });
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Install Tidepool Odyssey" }));
    expect(screen.getByRole("dialog", { name: "Install Tidepool Odyssey" })).toBeTruthy();
    expect(screen.getByText("Getting download info…")).toBeTruthy();
  });

  it("downloading_and_queued_games_show_their_state_instead_of_install", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => ({ updated_at: "x", games: [game("Tidepool Odyssey"), game("Kelp Runner")] }) });
    downloads.set([downloadItem("TidepoolOdyssey", "downloading"), downloadItem("KelpRunner", "queued")]);
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    expect(screen.getByText("Downloading…")).toBeTruthy();
    expect(screen.getByText("Queued")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Install / })).toBeNull();
  });

  it("menu_shows_installed_size", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner")] }),
    });
    installed.set([{ ...installedGame("KelpRunner"), install_size: 13207024435 }]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "More options for Kelp Runner" }));
    const menu = screen.getByRole("menu");
    expect(menu.textContent).toMatch(/^\s*Size: 12\.3 GB\s*Uninstall/);
  });

  it("uninstall_asks_first_then_calls_the_backend", async () => {
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner")] }),
      uninstall_game: () => [],
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "More options for Kelp Runner" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Uninstall" }));
    expect(screen.getByText("Uninstall Kelp Runner? Its game files will be deleted.")).toBeTruthy();
    expect(calls).not.toContain("uninstall_game");
    await fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    await waitFor(() => expect(calls).toContain("uninstall_game"));
    expect(await screen.findByRole("button", { name: "Install Kelp Runner" })).toBeTruthy();
  });

  it("a_failed_uninstall_shows_the_reason", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner")] }),
      uninstall_game: () => {
        throw "Couldn't delete some files. Close the game and try again.";
      },
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "More options for Kelp Runner" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Uninstall" }));
    await fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    expect(await screen.findByText("Couldn't delete some files. Close the game and try again.")).toBeTruthy();
  });

  it("covers_use_the_image_url_or_a_fish_placeholder", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("With Cover", "https://cdn/a.jpg"), game("No Cover")] }),
    });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("With Cover");
    expect((screen.getByAltText("With Cover cover art") as HTMLImageElement).src).toBe("https://cdn/a.jpg");
    expect(screen.getByLabelText("No cover art for No Cover")).toBeTruthy();
  });
  it("play_shows_only_on_installed_games_and_says_starting_then_play_again", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const played: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner"), game("Reef Run")] }),
      play_game: (args) => {
        played.push(args);
        return null;
      },
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    const play = await screen.findByRole("button", { name: "Play Kelp Runner" });
    expect(screen.queryByRole("button", { name: "Play Reef Run" })).toBeNull();
    await fireEvent.click(play);
    await waitFor(() => expect(played).toEqual([{ appName: "KelpRunner", offline: false }]));
    const starting = await screen.findByRole("button", { name: "Starting Kelp Runner" });
    expect(starting.textContent?.trim()).toBe("Starting…");
    expect((starting as HTMLButtonElement).disabled).toBe(true);
    await vi.advanceTimersByTimeAsync(5000);
    expect(await screen.findByRole("button", { name: "Play Kelp Runner" })).toBeTruthy();
  });

  it("a_failed_play_explains_and_offers_play_offline_when_allowed", async () => {
    const played: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner")] }),
      play_game: (args) => {
        played.push(args);
        if (!args.offline) throw { message: "Couldn't reach Epic to sign the game in. Check your internet.", can_offline: true, signed_out: false };
        return null;
      },
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Play Kelp Runner" }));
    expect(await screen.findByText("Couldn't reach Epic to sign the game in. Check your internet.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Play offline" }));
    await waitFor(() => expect(played).toEqual([
      { appName: "KelpRunner", offline: false },
      { appName: "KelpRunner", offline: true },
    ]));
    await waitFor(() => expect(screen.queryByText("Couldn't reach Epic to sign the game in. Check your internet.")).toBeNull());
  });

  it("play_offline_is_hidden_when_the_game_needs_epic", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner")] }),
      play_game: () => {
        throw { message: "Couldn't find Kelp Runner's program file. Uninstall and install it again.", can_offline: false, signed_out: false };
      },
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Play Kelp Runner" }));
    expect(await screen.findByText("Couldn't find Kelp Runner's program file. Uninstall and install it again.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Play offline" })).toBeNull();
  });

  it("an_expired_sign_in_goes_back_to_login", async () => {
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => ({ updated_at: "x", games: [game("Kelp Runner")] }),
      play_game: () => {
        throw { message: "Your sign-in has expired. Please sign in again.", can_offline: false, signed_out: true };
      },
      retry_auth: () => ({ kind: "SignedOut" }),
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Play Kelp Runner" }));
    await waitFor(() => expect(calls).toContain("retry_auth"));
  });
  // ---- Updates (spec §6e) ----

  const withUpdate = (games: Game[], latest: Record<string, string>) => ({ updated_at: "x", games, latest_versions: latest });

  it("an_installed_game_with_a_newer_version_shows_update_available_and_update", async () => {
    const started: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }),
      update_start: (args) => {
        started.push(args);
        return { items: [] };
      },
    });
    installed.set([installedGame("KelpRunner")]); // version "1"
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Update available")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Play Kelp Runner" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Update Kelp Runner" }));
    await waitFor(() => expect(started).toEqual([{ appName: "KelpRunner" }]));
  });

  it("an_up_to_date_game_shows_no_update", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "1" }) });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Installed")).toBeTruthy();
    expect(screen.queryByText("Update available")).toBeNull();
    expect(screen.queryByRole("button", { name: "Update Kelp Runner" })).toBeNull();
  });

  it("playing_an_outdated_game_warns_first_and_the_tick_box_is_saved", async () => {
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }),
      play_game: () => null,
      settings_set: (args) => args.value,
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Play Kelp Runner" }));
    const text = "Kelp Runner has an update waiting. You'll play the older version, and some online games won't start until they're updated.";
    expect(screen.getByText(text)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByText(text)).toBeNull();
    expect(calls).not.toContain("play_game");

    await fireEvent.click(screen.getByRole("button", { name: "Play Kelp Runner" }));
    await fireEvent.click(screen.getByRole("checkbox", { name: "Don't show this again" }));
    await fireEvent.click(screen.getByRole("button", { name: "Play anyway" }));
    await waitFor(() => expect(calls).toContain("play_game"));
    await waitFor(() => expect(calls).toContain("settings_set"));
    let warn = true;
    settings.subscribe((s) => (warn = s.warn_outdated_play))();
    expect(warn).toBe(false);
  });

  it("update_now_in_the_warning_starts_the_update_instead", async () => {
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }),
      update_start: () => ({ items: [] }),
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Play Kelp Runner" }));
    await fireEvent.click(screen.getByRole("button", { name: "Update now" }));
    await waitFor(() => expect(calls).toContain("update_start"));
    expect(calls).not.toContain("play_game");
  });

  it("with_the_warning_off_play_starts_at_once", async () => {
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }),
      play_game: () => null,
    });
    settings.set({ ...DEFAULT_SETTINGS, warn_outdated_play: false });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("button", { name: "Play Kelp Runner" }));
    await waitFor(() => expect(calls).toContain("play_game"));
  });

  it("an_updating_game_says_updating_and_hides_play", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }) });
    installed.set([installedGame("KelpRunner")]);
    downloads.set([{ ...downloadItem("KelpRunner", "downloading"), update: true }]);
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Updating…")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Play Kelp Runner" })).toBeNull();
  });

  it("auto_update_queues_outdated_games_without_a_job_after_a_refresh", async () => {
    const started: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner"), game("Reef Run"), game("Tide")], { KelpRunner: "2", ReefRun: "9", Tide: "1" }),
      settings_get: () => ({ auto_update: true, warn_outdated_play: true }),
      installed_list: () => [installedGame("KelpRunner"), installedGame("ReefRun"), installedGame("Tide")],
      update_start: (args) => {
        started.push(args);
        return { items: [] };
      },
    });
    downloads.set([{ ...downloadItem("ReefRun", "paused"), update: true }]); // already has a job
    render(LibraryPage, { name: "FishFan" });
    await waitFor(() => expect(started).toEqual([{ appName: "KelpRunner" }]));
  });
  it("auto_update_skips_running_games_and_queues_them_when_closed", async () => {
    const started: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }),
      settings_get: () => ({ auto_update: true, warn_outdated_play: true }),
      installed_list: () => [installedGame("KelpRunner")],
      update_start: (args) => {
        started.push(args);
        return { items: [] };
      },
    });
    running.set(["KelpRunner"]);
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Kelp Runner")).toBeTruthy();
    await new Promise((r) => setTimeout(r, 20));
    expect(started).toEqual([]);
    running.set([]);
    await waitFor(() => expect(started).toEqual([{ appName: "KelpRunner" }]));
  });

  // ---- Refresh button ----

  it("refresh_fetches_the_library_and_installed_list_again", async () => {
    let round = 0;
    const calls = mockCommands({
      library_cached: () => null,
      library_refresh: () => {
        round += 1;
        return withUpdate([game("Kelp Runner")], { KelpRunner: round === 1 ? "1" : "2" });
      },
      installed_list: () => [installedGame("KelpRunner")],
    });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    expect(await screen.findByText("Installed")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Refresh library" }));
    expect(await screen.findByText("Update available")).toBeTruthy();
    expect(calls.filter((c) => c === "library_refresh")).toHaveLength(2);
    expect(calls).toContain("installed_list");
  });

  it("refresh_is_disabled_while_refreshing_and_says_refreshing", async () => {
    let finish: (v: unknown) => void = () => {};
    let round = 0;
    mockCommands({
      library_cached: () => null,
      library_refresh: () => {
        round += 1;
        return round === 1 ? withUpdate([game("Kelp Runner")], {}) : new Promise((r) => (finish = r));
      },
      installed_list: () => [],
    });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    const button = screen.getByRole("button", { name: "Refresh library" }) as HTMLButtonElement;
    await fireEvent.click(button);
    expect(await screen.findByText("Refreshing…")).toBeTruthy();
    expect(screen.getByText("Fishy Client checks every game with Epic, so this takes a few seconds.")).toBeTruthy();
    expect(button.title).toBe("Check Epic for new games and updates. It checks every game, so it takes a few seconds.");
    expect(button.disabled).toBe(true);
    finish(withUpdate([game("Kelp Runner")], {}));
    await waitFor(() => expect(button.disabled).toBe(false));
  });

  it("refresh_is_hidden_offline", async () => {
    mockCommands({ library_cached: () => ({ updated_at: "x", games: [game("Kelp Runner")] }) });
    render(LibraryPage, { name: "FishFan", offline: true });
    await screen.findByText("Kelp Runner");
    expect(screen.queryByRole("button", { name: "Refresh library" })).toBeNull();
  });
});

describe("Updates tab and Sort menu", () => {
  const withUpdate = (games: Game[], latest: Record<string, string>) => ({ updated_at: "x", games, latest_versions: latest });

  it("the_updates_tab_shows_only_games_with_an_update_waiting", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner"), game("Coral Kart"), game("Reef Raiders")], { KelpRunner: "2", CoralKart: "1" }),
    });
    installed.set([installedGame("KelpRunner"), installedGame("CoralKart")]);
    render(LibraryPage, { name: "FishFan" });
    const tab = await screen.findByRole("tab", { name: "Updates (1)" });
    await fireEvent.click(tab);
    await waitFor(() => expect(screen.queryByText("Coral Kart")).toBeNull());
    expect(screen.getByText("Kelp Runner")).toBeTruthy();
    expect(screen.queryByText("Reef Raiders")).toBeNull();
  });

  it("there_is_no_updates_tab_without_updates", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "1" }) });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    expect(screen.queryByRole("tab", { name: /Updates/ })).toBeNull();
  });

  it("when_the_last_update_is_done_the_updates_tab_goes_back_to_all", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => withUpdate([game("Kelp Runner"), game("Coral Kart")], { KelpRunner: "2" }) });
    installed.set([installedGame("KelpRunner")]);
    render(LibraryPage, { name: "FishFan" });
    await fireEvent.click(await screen.findByRole("tab", { name: "Updates (1)" }));
    installed.set([{ ...installedGame("KelpRunner"), version: "2" }]);
    await waitFor(() => expect(screen.getByRole("tab", { name: "All" }).getAttribute("aria-selected")).toBe("true"));
    expect(screen.getByText("Coral Kart")).toBeTruthy();
  });

  it("the_sort_menu_orders_the_cards_and_saves_the_choice", async () => {
    const saved: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Coral Kart"), game("Abyssal Lanterns"), game("Kelp Runner")], {}),
      settings_set: (a: { value: unknown }) => {
        saved.push(a.value);
        return a.value;
      },
    });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    const cardTitles = () => Array.from(document.querySelectorAll(".card .title")).map((e) => e.textContent?.trim());
    expect(cardTitles()).toEqual(["Abyssal Lanterns", "Coral Kart", "Kelp Runner"]);
    await fireEvent.change(screen.getByRole("combobox", { name: "Sort" }), { target: { value: "za" } });
    await waitFor(() => expect(cardTitles()).toEqual(["Kelp Runner", "Coral Kart", "Abyssal Lanterns"]));
    await waitFor(() => expect(saved.at(-1)).toMatchObject({ library_sort: "za" }));
  });

  it("sort_menu_offers_recently_played", async () => {
    const saved: unknown[] = [];
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner")], {}),
      settings_set: (a: { value: unknown }) => {
        saved.push(a.value);
        return a.value;
      },
    });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Kelp Runner");
    const sort = screen.getByRole("combobox", { name: "Sort" }) as HTMLSelectElement;
    expect(Array.from(sort.options).map((o) => o.textContent)).toContain("Recently played");
    await fireEvent.change(sort, { target: { value: "played" } });
    await waitFor(() => expect(saved.at(-1)).toMatchObject({ library_sort: "played" }));
  });

  it("a_running_game_shows_running_and_greys_out_its_buttons", async () => {
    mockCommands({ library_cached: () => null, library_refresh: () => withUpdate([game("Kelp Runner")], { KelpRunner: "2" }) });
    installed.set([installedGame("KelpRunner")]);
    running.set(["KelpRunner"]);
    render(LibraryPage, { name: "FishFan" });
    const btn = (await screen.findByRole("button", { name: "Kelp Runner is running" })) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.textContent?.trim()).toBe("Running");
    expect((screen.getByRole("button", { name: "Update Kelp Runner" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "More options for Kelp Runner" }));
    expect((screen.getByRole("menuitem", { name: "Uninstall" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("Close Kelp Runner first.")).toBeTruthy();
    running.set([]);
    await waitFor(() => expect((screen.getByRole("button", { name: "Play Kelp Runner" }) as HTMLButtonElement).disabled).toBe(false));
    expect(screen.queryByText("Close Kelp Runner first.")).toBeNull();
    expect((screen.getByRole("button", { name: "Update Kelp Runner" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("the_saved_sort_is_used_and_works_with_search", async () => {
    mockCommands({
      library_cached: () => null,
      library_refresh: () => withUpdate([game("Kelp Runner"), game("Kelp Kart"), game("Coral Kart")], {}),
      settings_get: () => ({ ...DEFAULT_SETTINGS, library_sort: "za" }),
    });
    render(LibraryPage, { name: "FishFan" });
    await screen.findByText("Coral Kart");
    await waitFor(() => expect((screen.getByRole("combobox", { name: "Sort" }) as HTMLSelectElement).value).toBe("za"));
    await fireEvent.input(screen.getByRole("searchbox"), { target: { value: "kelp" } });
    const cardTitles = () => Array.from(document.querySelectorAll(".card .title")).map((e) => e.textContent?.trim());
    await waitFor(() => expect(cardTitles()).toEqual(["Kelp Runner", "Kelp Kart"]));
  });
});
