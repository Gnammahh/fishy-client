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
    await waitFor(() => expect(saved).toEqual([{ auto_update: true, warn_outdated_play: true }]));
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
});
