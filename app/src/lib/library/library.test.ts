import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import LibraryPage from "./LibraryPage.svelte";
import { filterGames, resetLibrary, type Game } from "./library";

const game = (title: string, cover: string | null = null): Game => ({
  app_name: title.replace(/\W/g, ""),
  namespace: "ns",
  catalog_item_id: title.toLowerCase().replace(/\W/g, "-"),
  title,
  cover_url: cover,
});

function mockCommands(handlers: Record<string, () => unknown>) {
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    return handlers[cmd]?.();
  });
  return calls;
}

afterEach(() => {
  cleanup();
  clearMocks();
  resetLibrary();
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
});
