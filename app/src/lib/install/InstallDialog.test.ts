import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import InstallDialog from "./InstallDialog.svelte";
import { resetInstalls } from "./installs";

const GB = 1024 ** 3;
const game = { app_name: "Tide", namespace: "ns", catalog_item_id: "cat", title: "Tidepool Odyssey", cover_url: null, installable: true };
const options = (over: Record<string, unknown> = {}) => ({
  install_path: "C:\\Users\\you\\Games\\Tidepool Odyssey",
  download_size: 3.1 * GB,
  install_size: 7.4 * GB,
  free_space: 695 * GB,
  enough_space: true,
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
