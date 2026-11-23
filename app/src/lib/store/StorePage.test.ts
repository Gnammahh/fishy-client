import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import StorePage from "./StorePage.svelte";
import type { FreeGame } from "./store";

const game = (title: string, date: string, over: Partial<FreeGame> = {}): FreeGame => ({
  title,
  url: "https://store.epicgames.com/p/" + title.toLowerCase().replace(/\W+/g, "-"),
  image: null,
  date,
  owned: false,
  ...over,
});

function mock(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: [string, Record<string, unknown>][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args as Record<string, unknown>]);
    return handlers[cmd]?.(args as Record<string, unknown>);
  });
  return calls;
}

const lists = { now: [game("Mark of the Deep", "2026-11-26T16:00:00Z")], next: [game("Bread and Fred", "2026-11-26T16:00:00Z")] };

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("StorePage", () => {
  it("shows_free_now_and_next_with_dates", async () => {
    mock({ store_free_games: () => lists });
    render(StorePage);
    expect(await screen.findByText("Mark of the Deep")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Free now" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Free next" })).toBeTruthy();
    expect(screen.getByText(/^Free until Nov 26/)).toBeTruthy();
    expect(screen.getByText(/^Free from Nov 26/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Get Mark of the Deep on Epic" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "View Bread and Fred on Epic" })).toBeTruthy();
    expect(screen.getByText("Fishy Client opens Epic's page; you get the game there yourself.")).toBeTruthy();
  });

  it("owned_game_says_in_your_library", async () => {
    mock({ store_free_games: () => ({ now: [game("Mark of the Deep", "2026-11-26T16:00:00Z", { owned: true })], next: [] }) });
    render(StorePage);
    expect(await screen.findByText("In your Library")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Get Mark of the Deep on Epic" })).toBeNull();
  });

  it("get_opens_the_store_window_at_the_page", async () => {
    const calls = mock({ store_free_games: () => lists, store_open: () => null });
    render(StorePage);
    await fireEvent.click(await screen.findByRole("button", { name: "Get Mark of the Deep on Epic" }));
    await waitFor(() => expect(calls).toContainEqual(["store_open", { url: "https://store.epicgames.com/p/mark-of-the-deep" }]));
  });

  it("open_epics_store_button", async () => {
    const calls = mock({ store_free_games: () => lists, store_open: () => null });
    render(StorePage);
    await fireEvent.click(screen.getByRole("button", { name: "Open Epic's store" }));
    await waitFor(() => expect(calls).toContainEqual(["store_open", { url: null }]));
  });

  it("empty_sections_say_nothing_right_now", async () => {
    mock({ store_free_games: () => ({ now: [], next: [] }) });
    render(StorePage);
    await waitFor(() => expect(screen.getAllByText("Nothing right now.").length).toBe(2));
  });

  it("error_offers_try_again_with_force", async () => {
    let n = 0;
    const calls = mock({
      store_free_games: () => {
        n++;
        if (n === 1) throw "Couldn't load the free games. Check your internet, then press Try again.";
        return lists;
      },
    });
    render(StorePage);
    expect(await screen.findByText("Couldn't load the free games. Check your internet, then press Try again.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Mark of the Deep")).toBeTruthy();
    expect(calls.filter(([c]) => c === "store_free_games").map(([, a]) => a)).toEqual([{ force: false }, { force: true }]);
  });
});
