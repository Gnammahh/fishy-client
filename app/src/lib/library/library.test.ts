import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import LibraryPage from "./LibraryPage.svelte";
import { filterGames, resetLibrary, type Game } from "./library";
import { downloads, installed, resetInstalls, type DownloadItem, type InstalledGame } from "$lib/install/installs";
import { settings, resetSettings } from "$lib/settings/settings";

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
    expect(await screen.findByText("Couldn't load your library from Epic.")).toBeTruthy();
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
    expect(await screen.findByText("Couldn't load your library from Epic.")).toBeTruthy();
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
    settings.set({ auto_update: false, warn_outdated_play: false });
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
});
