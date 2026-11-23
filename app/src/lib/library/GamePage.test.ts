import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { createRawSnippet } from "svelte";
import GamePage from "./GamePage.svelte";
import type { Game } from "./library";
import type { Achievement, Achievements } from "./achievements";

const game = (title: string): Game => ({
  app_name: title.replace(/\W/g, ""),
  namespace: "ns",
  catalog_item_id: title,
  title,
  cover_url: null,
  installable: true,
});
const ach = (name: string, unlocked: boolean, extra: Partial<Achievement> = {}): Achievement => ({
  name,
  title: name,
  description: `${name} text`,
  icon_url: null,
  unlocked,
  unlocked_at: null,
  hidden: false,
  rarity: null,
  ...extra,
});
const two: Achievements = {
  total: 2,
  unlocked: 1,
  items: [ach("Got it", true, { unlocked_at: "2025-03-12T18:00:00.000Z" }), ach("Not yet", false, { rarity: 4.2 })],
};
const actions = createRawSnippet((g: () => Game) => ({ render: () => `<button>Play ${g().title}</button>` }));

function props(title = "Kelp Runner", onback = () => {}) {
  return { game: game(title), actions, lines: ["Size: 1 GB"], onback };
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("GamePage", () => {
  it("shows_count_and_rows", async () => {
    mockIPC((cmd) => (cmd === "game_achievements" ? two : undefined));
    render(GamePage, props());
    expect(await screen.findByText("1 / 2 unlocked")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Kelp Runner" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Play Kelp Runner" })).toBeTruthy();
    expect(screen.getByText("Size: 1 GB")).toBeTruthy();
    expect(screen.getByText("Unlocked 12 Mar 2025")).toBeTruthy();
    expect(screen.getByText("4.2% of players have it")).toBeTruthy();
    expect(screen.getByText("Not yet").closest("li")?.classList.contains("locked")).toBe(true);
    expect(screen.getByText("Got it").closest("li")?.classList.contains("locked")).toBe(false);
  });

  it("filter_switches_rows", async () => {
    mockIPC((cmd) => (cmd === "game_achievements" ? two : undefined));
    render(GamePage, props());
    await screen.findByText("1 / 2 unlocked");
    await fireEvent.click(screen.getByRole("tab", { name: "Unlocked" }));
    expect(screen.queryByText("Not yet")).toBeNull();
    expect(screen.getByText("Got it")).toBeTruthy();
    await fireEvent.click(screen.getByRole("tab", { name: "Locked" }));
    expect(screen.queryByText("Got it")).toBeNull();
    expect(screen.getByText("Not yet")).toBeTruthy();
  });

  it("empty_filters_say_so", async () => {
    mockIPC((cmd) => (cmd === "game_achievements" ? { total: 1, unlocked: 0, items: [ach("Not yet", false)] } : undefined));
    render(GamePage, props());
    await screen.findByText("0 / 1 unlocked");
    await fireEvent.click(screen.getByRole("tab", { name: "Unlocked" }));
    expect(screen.getByText("None unlocked yet.")).toBeTruthy();
    cleanup();
    mockIPC((cmd) => (cmd === "game_achievements" ? { total: 1, unlocked: 1, items: [ach("Got it", true)] } : undefined));
    render(GamePage, props());
    await screen.findByText("1 / 1 unlocked");
    expect(screen.getByText("Got it").closest("li")?.querySelector(".when")?.textContent).toBe("Unlocked"); // no date from Epic
    await fireEvent.click(screen.getByRole("tab", { name: "Locked" }));
    expect(screen.getByText("You have them all!")).toBeTruthy();
  });

  it("no_achievements_text", async () => {
    mockIPC((cmd) => (cmd === "game_achievements" ? { total: 0, unlocked: 0, items: [] } : undefined));
    render(GamePage, props());
    expect(await screen.findByText("This game has no Epic achievements.")).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Locked" })).toBeNull();
  });

  it("error_then_try_again", async () => {
    let calls = 0;
    mockIPC((cmd) => {
      if (cmd !== "game_achievements") return undefined;
      calls += 1;
      if (calls === 1) throw "Couldn't load achievements from Epic.";
      return two;
    });
    render(GamePage, props());
    expect(await screen.findByText("Couldn't load achievements from Epic.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("1 / 2 unlocked")).toBeTruthy();
  });

  it("a_late_reply_for_another_game_is_ignored", async () => {
    let releaseA: (v: Achievements) => void = () => {};
    mockIPC((cmd, args) => {
      if (cmd !== "game_achievements") return undefined;
      const app = (args as { appName: string }).appName;
      if (app === "GameA") return new Promise<Achievements>((r) => (releaseA = r));
      return { total: 1, unlocked: 1, items: [ach("B win", true)] };
    });
    const view = render(GamePage, props("Game A"));
    await view.rerender(props("Game B"));
    expect(await screen.findByText("B win")).toBeTruthy();
    releaseA({ total: 1, unlocked: 0, items: [ach("A thing", false)] });
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByText("A thing")).toBeNull();
    expect(screen.getByText("1 / 1 unlocked")).toBeTruthy();
  });

  it("back_calls_onback", async () => {
    mockIPC((cmd) => (cmd === "game_achievements" ? two : undefined));
    const onback = vi.fn();
    render(GamePage, props("Kelp Runner", onback));
    await fireEvent.click(screen.getByRole("button", { name: "← Library" }));
    await waitFor(() => expect(onback).toHaveBeenCalled());
  });
});
