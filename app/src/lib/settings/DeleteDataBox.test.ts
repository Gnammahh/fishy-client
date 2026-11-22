import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { get } from "svelte/store";
import DeleteDataBox from "./DeleteDataBox.svelte";
import { authState, loginError } from "$lib/auth/auth";
import type { InstalledGame } from "$lib/install/installs";

const GB = 1024 ** 3;
const game = (name: string, gb: number): InstalledGame => ({
  app_name: name,
  title: name,
  version: "1",
  install_path: `C:\\Games\\${name}`,
  executable: "a.exe",
  launch_parameters: "",
  install_size: gb * GB,
  installed_at: "2026-11-21T00:00:00Z",
});

const NOTE = "Your game folders stay on disk, but Fishy Client will forget them.";

afterEach(() => {
  cleanup();
  clearMocks();
  authState.set({ kind: "Loading" });
  loginError.set("");
});

function calls(handler: (cmd: string, args: unknown) => unknown = () => ({ failed: 0, message: null })) {
  const seen: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    seen.push({ cmd, args });
    return handler(cmd, args);
  });
  return seen;
}

describe("DeleteDataBox", () => {
  it("explains_what_goes", () => {
    calls();
    render(DeleteDataBox, { games: [], onclose: () => {} });
    expect(
      screen.getByText("This logs you out and removes everything Fishy Client saved on this PC: settings, your game list, download progress and logs."),
    ).toBeTruthy();
  });

  it("tick_box_hidden_with_no_games", () => {
    calls();
    render(DeleteDataBox, { games: [], onclose: () => {} });
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByText(NOTE)).toBeNull();
  });

  it("tick_box_shows_count_and_size", async () => {
    calls();
    render(DeleteDataBox, { games: [game("A", 20), game("B", 12), game("C", 10)], onclose: () => {} });
    const box = screen.getByRole("checkbox", { name: "Also delete my installed games (3 games, 42.0 GB)" }) as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(screen.getByText(NOTE)).toBeTruthy();
    await fireEvent.click(box);
    expect(screen.queryByText(NOTE)).toBeNull();
  });

  it("one_game_is_singular", () => {
    calls();
    render(DeleteDataBox, { games: [game("A", 1)], onclose: () => {} });
    expect(screen.getByRole("checkbox", { name: "Also delete my installed games (1 game, 1.0 GB)" })).toBeTruthy();
  });

  it("a_first_download_alone_still_offers_the_tick", async () => {
    const seen = calls();
    const dl = { app_name: "B", title: "B", state: "paused" as const, downloaded: GB, total: 3 * GB, bytes_per_sec: 0, error: null };
    render(DeleteDataBox, { games: [], downloads: [dl], onclose: () => {} });
    const box = screen.getByRole("checkbox", { name: "Also delete my unfinished downloads (1 download)" });
    expect(screen.getByText(NOTE)).toBeTruthy();
    await fireEvent.click(box);
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(seen.find((c) => c.cmd === "delete_all_data")?.args).toEqual({ alsoGames: true }));
  });

  it("games_and_downloads_are_both_named", () => {
    calls();
    const dl = { app_name: "B", title: "B", state: "downloading" as const, downloaded: GB, total: 3 * GB, bytes_per_sec: 1, error: null };
    render(DeleteDataBox, { games: [game("A", 2)], downloads: [dl, { ...dl, app_name: "C" }], onclose: () => {} });
    expect(
      screen.getByRole("checkbox", { name: "Also delete my installed games and unfinished downloads (1 game, 2.0 GB; 2 downloads)" }),
    ).toBeTruthy();
  });

  it("cancel_does_nothing", async () => {
    const seen = calls();
    let closed = false;
    render(DeleteDataBox, { games: [game("A", 1)], onclose: () => (closed = true) });
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(closed).toBe(true);
    expect(seen.filter((c) => c.cmd === "delete_all_data")).toEqual([]);
  });

  it("delete_sends_the_tick_and_disables_buttons", async () => {
    let finish: (v: unknown) => void = () => {};
    const seen = calls((cmd) => (cmd === "delete_all_data" ? new Promise((r) => (finish = r)) : null));
    authState.set({ kind: "SignedIn", display_name: "FishFan", account_id: "acc" });
    render(DeleteDataBox, { games: [game("A", 1)], onclose: () => {} });
    const del = screen.getByRole("button", { name: "Delete" }) as HTMLButtonElement;
    const cancel = screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement;
    await fireEvent.click(del);
    await waitFor(() => expect(del.disabled).toBe(true));
    expect(cancel.disabled).toBe(true);
    await fireEvent.click(del);
    finish({ failed: 0, message: null });
    await waitFor(() => expect(get(authState)).toEqual({ kind: "SignedOut" }));
    const deletes = seen.filter((c) => c.cmd === "delete_all_data");
    expect(deletes.length).toBe(1);
    expect(deletes[0].args).toEqual({ alsoGames: false });
  });

  it("ticked_sends_also_games", async () => {
    const seen = calls();
    render(DeleteDataBox, { games: [game("A", 1)], onclose: () => {} });
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(seen.find((c) => c.cmd === "delete_all_data")?.args).toEqual({ alsoGames: true }));
  });

  it("partial_failure_is_shown_on_the_login_screen", async () => {
    const msg = "Some files couldn't be deleted (2). Close any running games and try again.";
    calls(() => ({ failed: 2, message: msg }));
    render(DeleteDataBox, { games: [], onclose: () => {} });
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(get(loginError)).toBe(msg));
    expect(get(authState)).toEqual({ kind: "SignedOut" });
  });

  it("a_download_that_wont_stop_is_explained_in_the_box", async () => {
    const msg = "A download wouldn't stop, so nothing was deleted. Try again in a minute.";
    calls(() => {
      throw msg;
    });
    authState.set({ kind: "SignedIn", display_name: "FishFan", account_id: "acc" });
    render(DeleteDataBox, { games: [], onclose: () => {} });
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect((await screen.findByRole("alert")).textContent).toBe(msg);
    expect(get(authState)).toEqual({ kind: "SignedIn", display_name: "FishFan", account_id: "acc" });
    expect((screen.getByRole("button", { name: "Delete" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("a_failed_call_says_so_in_the_box", async () => {
    calls(() => {
      throw new Error("ipc down");
    });
    render(DeleteDataBox, { games: [], onclose: () => {} });
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Couldn't delete your data. Please try again.");
  });
});
