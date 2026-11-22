import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import SettingsPage from "./SettingsPage.svelte";
import { loadSettings, resetSettings } from "./settings";

afterEach(() => {
  cleanup();
  clearMocks();
  resetSettings();
});

describe("SettingsPage", () => {
  it("shows_the_saved_settings_and_saves_a_change", async () => {
    const saved: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "settings_get") return { auto_update: false, warn_outdated_play: true };
      if (cmd === "settings_set") {
        saved.push((args as { value: unknown }).value);
        return (args as { value: unknown }).value;
      }
      return null;
    });
    await loadSettings();
    render(SettingsPage);
    const auto = screen.getByRole("switch", { name: "Update games automatically" }) as HTMLInputElement;
    const warn = screen.getByRole("switch", { name: "Warn before playing a game with an update waiting" }) as HTMLInputElement;
    expect(auto.checked).toBe(false);
    expect(warn.checked).toBe(true);
    await fireEvent.click(auto);
    await waitFor(() =>
      expect(saved).toEqual([
        { auto_update: true, warn_outdated_play: true, install_folder: null, speed_limit_mbps: null, reduce_motion: false, library_sort: "az", when_game_starts: "keep", show_in_epic: true },
      ]),
    );
  });

  it("show_in_epic_switch_is_on_and_saves", async () => {
    const saved: { show_in_epic?: boolean }[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "settings_get") return { auto_update: false, warn_outdated_play: true };
      if (cmd === "settings_set") {
        saved.push((args as { value: { show_in_epic?: boolean } }).value);
        return (args as { value: unknown }).value;
      }
      return null;
    });
    await loadSettings();
    render(SettingsPage);
    const sw = screen.getByRole("switch", { name: "Show my games in Epic's launcher" }) as HTMLInputElement;
    expect(sw.checked).toBe(true);
    expect(screen.getByText("Epic's launcher sees games installed with Fishy Client. Changes show the next time Epic's launcher starts.")).toBeTruthy();
    await fireEvent.click(sw);
    await waitFor(() => expect(saved[0]?.show_in_epic).toBe(false));
  });

  it("a_failed_save_says_so_and_keeps_the_old_value", async () => {
    mockIPC((cmd) => {
      if (cmd === "settings_set") throw "disk full";
      return null;
    });
    render(SettingsPage);
    const auto = screen.getByRole("switch", { name: "Update games automatically" }) as HTMLInputElement;
    await fireEvent.click(auto);
    expect(await screen.findByText("Couldn't save your settings.")).toBeTruthy();
    expect(auto.checked).toBe(false);
  });

  it("account_section_has_log_out_logs_and_delete", async () => {
    const seen: string[] = [];
    mockIPC((cmd) => {
      seen.push(cmd);
      if (cmd === "logout") return { kind: "SignedOut" };
      return null;
    });
    render(SettingsPage, { account: { display_name: "FishFan", account_id: "acc123" } });
    expect(screen.getByRole("heading", { name: "Account" })).toBeTruthy();
    expect(screen.getByText("FishFan")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open logs folder" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete all my data…" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Log out" }));
    await waitFor(() => expect(seen).toContain("logout"));
  });

  it("open_logs_failure_shows_the_message", async () => {
    mockIPC((cmd) => {
      if (cmd === "open_logs_folder") throw "Couldn't open the logs folder. It's at %APPDATA%\\FishyClient\\logs.";
      return null;
    });
    render(SettingsPage, { account: { display_name: "FishFan" } });
    await fireEvent.click(screen.getByRole("button", { name: "Open logs folder" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Couldn't open the logs folder. It's at %APPDATA%\\FishyClient\\logs.");
  });

  it("delete_all_my_data_opens_the_box", async () => {
    mockIPC(() => null);
    render(SettingsPage, { account: { display_name: "FishFan" } });
    await fireEvent.click(screen.getByRole("button", { name: "Delete all my data…" }));
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
