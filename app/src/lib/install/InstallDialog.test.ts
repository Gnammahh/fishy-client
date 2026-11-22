import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import InstallDialog from "./InstallDialog.svelte";
import { resetInstalls } from "./installs";

const GB = 1024 ** 3;
const EPIC = "C:\\Program Files\\Epic Games";
const game = { app_name: "Tide", namespace: "ns", catalog_item_id: "cat", title: "Tidepool Odyssey", cover_url: null, installable: true };
const options = (over: Record<string, unknown> = {}) => ({
  install_path: "C:\\Users\\you\\Games\\Tidepool Odyssey",
  download_size: 3.1 * GB,
  install_size: 7.4 * GB,
  free_space: 695 * GB,
  enough_space: true,
  folder_ok: true,
  error: null,
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

afterEach(() => {
  cleanup();
  clearMocks();
  resetInstalls();
});

describe("InstallDialog", () => {
  it("shows_the_default_folder_and_sizes", async () => {
    mock({ install_options: () => options() });
    render(InstallDialog, { game, onclose: () => {} });
    expect(screen.getByRole("dialog", { name: "Install Tidepool Odyssey" })).toBeTruthy();
    expect(await screen.findByText("C:\\Users\\you\\Games\\Tidepool Odyssey")).toBeTruthy();
    expect(screen.getByText("3.1 GB")).toBeTruthy();
    expect(screen.getByText("7.4 GB")).toBeTruthy();
    expect(screen.getByText("695.0 GB")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Install" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("says_only_fishy_client_will_know_the_game", async () => {
    mock({ install_options: () => options() });
    render(InstallDialog, { game, onclose: () => {} });
    expect(
      await screen.findByText("Only Fishy Client will know about this game. Epic's launcher can't see it and would download it again."),
    ).toBeTruthy();
  });

  it("says_when_the_default_folder_isnt_available", async () => {
    const note = "Your default folder isn't available, so this uses C:\\Users\\you\\Games.";
    mock({ install_options: () => options({ note }) });
    render(InstallDialog, { game, onclose: () => {} });
    expect(await screen.findByText(note)).toBeTruthy();
  });

  it("not_enough_space_disables_install_and_says_why", async () => {
    mock({ install_options: () => options({ enough_space: false, error: "Not enough space on C:. Needs 900.0 GB, 695.0 GB free." }) });
    render(InstallDialog, { game, onclose: () => {} });
    expect(await screen.findByText("Not enough space on C:. Needs 900.0 GB, 695.0 GB free.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Install" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("folder_not_ok_disables_install_and_shows_message", async () => {
    const error = "Fishy Client can't create folders in C:\\Program Files\\Epic Games without administrator rights. Pick a different folder.";
    mock({ install_options: () => options({ folder_ok: false, error }) });
    render(InstallDialog, { game, onclose: () => {} });
    expect(await screen.findByText(error)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Install" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("needs_permission_shows_note_and_install_enabled", async () => {
    mock({ install_options: () => options({ folder_ok: true, needs_permission: true }) });
    render(InstallDialog, { game, onclose: () => {} });
    expect(await screen.findByText("Windows will ask for permission once to create this folder.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Install" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("use_epics_folder_asks_again_with_it", async () => {
    const calls = mock({ install_options: () => options(), epic_folder: () => EPIC });
    render(InstallDialog, { game, onclose: () => {} });
    await fireEvent.click(await screen.findByRole("button", { name: "Use Epic's folder" }));
    await waitFor(() => expect(calls.filter(([c]) => c === "install_options").at(-1)![1].parent).toBe(EPIC));
  });

  it("use_epics_folder_hidden_without_it", async () => {
    mock({ install_options: () => options(), epic_folder: () => null });
    render(InstallDialog, { game, onclose: () => {} });
    await screen.findByText("C:\\Users\\you\\Games\\Tidepool Odyssey");
    expect(screen.queryByRole("button", { name: "Use Epic's folder" })).toBeNull();
  });

  it("declined_message_shown", async () => {
    const msg = "Fishy Client needs your permission to use that folder. Pick a different folder.";
    mock({
      install_options: () => options({ needs_permission: true }),
      install_start: () => {
        throw msg;
      },
    });
    render(InstallDialog, { game, onclose: () => {} });
    await fireEvent.click(await screen.findByRole("button", { name: "Install" }));
    expect(await screen.findByText(msg)).toBeTruthy();
  });

  it("change_folder_asks_again_with_the_picked_parent", async () => {
    const calls = mock({
      install_options: (a) => options({ install_path: `${a.parent ?? "C:\\Users\\you\\Games"}\\Tidepool Odyssey` }),
      pick_folder: () => "D:\\Games",
    });
    render(InstallDialog, { game, onclose: () => {} });
    await screen.findByText("C:\\Users\\you\\Games\\Tidepool Odyssey");
    await fireEvent.click(screen.getByRole("button", { name: "Change folder" }));
    expect(await screen.findByText("D:\\Games\\Tidepool Odyssey")).toBeTruthy();
    expect(calls.filter(([c]) => c === "install_options").at(-1)![1].parent).toBe("D:\\Games");
  });

  it("install_starts_the_download_with_the_chosen_parent_and_closes", async () => {
    const onclose = vi.fn();
    const calls = mock({ install_options: () => options(), pick_folder: () => "D:\\Games", install_start: () => ({ items: [] }) });
    render(InstallDialog, { game, onclose });
    await screen.findByText("C:\\Users\\you\\Games\\Tidepool Odyssey");
    await fireEvent.click(screen.getByRole("button", { name: "Change folder" }));
    await waitFor(() => expect(calls.filter(([c]) => c === "install_options")).toHaveLength(2));
    await fireEvent.click(screen.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    const start = calls.find(([c]) => c === "install_start")!;
    expect(start[1].parent).toBe("D:\\Games");
    expect((start[1].game as { app_name: string }).app_name).toBe("Tide");
  });

  it("errors_from_epic_are_shown", async () => {
    mock({
      install_options: () => {
        throw "This game uses a download format Fishy Client can't install yet.";
      },
    });
    render(InstallDialog, { game, onclose: () => {} });
    expect(await screen.findByText("This game uses a download format Fishy Client can't install yet.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Install" })).toBeNull();
  });

  it("cancel_and_escape_close_it", async () => {
    const onclose = vi.fn();
    const calls = mock({ install_options: () => options() });
    render(InstallDialog, { game, onclose });
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(2);
    // Closing frees the file list the box fetched.
    expect(calls.filter(([c]) => c === "install_forget").map(([, a]) => a.appName)).toEqual([game.app_name, game.app_name]);
  });
});
