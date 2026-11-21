import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import SettingsPage from "./SettingsPage.svelte";
import { DEFAULT_SETTINGS, loadSettings, resetSettings, type Settings } from "./settings";
import { parseSpeed } from "./data";

afterEach(() => {
  cleanup();
  clearMocks();
  resetSettings();
});

const DEFAULT_FOLDER = "C:\\Users\\you\\Games";
const SPEED_ERROR = "Pick a number from 1 to 1000.";

type Handlers = Record<string, (args: Record<string, unknown>) => unknown>;

/** Fake backend: saved settings, the default folder and Start with Windows. */
function backend(over: Partial<Settings> = {}, extra: Handlers = {}) {
  const saved: Settings[] = [];
  let current: Settings = { ...DEFAULT_SETTINGS, ...over };
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    if (extra[cmd]) return extra[cmd](a);
    if (cmd === "settings_get") return current;
    if (cmd === "settings_set") {
      current = a.value as Settings;
      saved.push(current);
      return current;
    }
    if (cmd === "default_install_folder") return DEFAULT_FOLDER;
    if (cmd === "autostart_get") return false;
    return null;
  });
  return saved;
}

async function show(over: Partial<Settings> = {}, extra: Handlers = {}) {
  const saved = backend(over, extra);
  await loadSettings();
  render(SettingsPage, { account: { display_name: "FishFan" } });
  return saved;
}

describe("Settings sections", () => {
  it("sections_in_order", async () => {
    await show();
    const titles = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(titles).toEqual(["Downloads", "Appearance and startup", "Account"]);
  });
});

describe("Default install folder", () => {
  it("shows_the_default_without_use_default", async () => {
    await show();
    expect(await screen.findByText(DEFAULT_FOLDER)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Use default" })).toBeNull();
  });

  it("change_saves_the_picked_folder_and_offers_use_default", async () => {
    const saved = await show({}, { pick_folder: () => "D:\\Games" });
    await fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    await waitFor(() => expect(saved.at(-1)?.install_folder).toBe("D:\\Games"));
    expect(await screen.findByText("D:\\Games")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Use default" }));
    await waitFor(() => expect(saved.at(-1)?.install_folder).toBeNull());
  });

  it("cancelling_the_picker_saves_nothing", async () => {
    const saved = await show({}, { pick_folder: () => null });
    await fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    await new Promise((r) => setTimeout(r, 10));
    expect(saved).toEqual([]);
  });

  it("picker_failure_says_so", async () => {
    await show({}, {
      pick_folder: () => {
        throw "boom";
      },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    expect(await screen.findByText("Couldn't open the folder picker.")).toBeTruthy();
  });
});

describe("Limit download speed", () => {
  it("speed_switch_saves_10_by_default_and_off_saves_null", async () => {
    const saved = await show();
    const sw = screen.getByRole("switch", { name: "Limit download speed" }) as HTMLInputElement;
    expect(sw.checked).toBe(false);
    expect(screen.queryByRole("textbox", { name: "MB/s" })).toBeNull();
    await fireEvent.click(sw);
    await waitFor(() => expect(saved.at(-1)?.speed_limit_mbps).toBe(10));
    expect(((await screen.findByRole("textbox", { name: "MB/s" })) as HTMLInputElement).value).toBe("10");
    await fireEvent.click(sw);
    await waitFor(() => expect(saved.at(-1)?.speed_limit_mbps).toBeNull());
  });

  it("speed_box_refuses_bad_numbers", async () => {
    const saved = await show({ speed_limit_mbps: 10 });
    const box = screen.getByRole("textbox", { name: "MB/s" }) as HTMLInputElement;
    for (const bad of ["", "2.5", "abc", "0", "1001"]) {
      await fireEvent.change(box, { target: { value: bad } });
      expect(await screen.findByText(SPEED_ERROR)).toBeTruthy();
    }
    expect(saved).toEqual([]);
    await fireEvent.change(box, { target: { value: "25" } });
    await waitFor(() => expect(saved.at(-1)?.speed_limit_mbps).toBe(25));
    expect(screen.queryByText(SPEED_ERROR)).toBeNull();
  });

  it("parse_speed_accepts_only_whole_numbers_from_1_to_1000", () => {
    expect(["", "2.5", "abc", "0", "1001", "-3", "1e2"].map(parseSpeed)).toEqual([null, null, null, null, null, null, null]);
    expect(["1", "25", " 40 ", "1000"].map(parseSpeed)).toEqual([1, 25, 40, 1000]);
  });
});

describe("Appearance and startup", () => {
  it("reduce_motion_switch_saves", async () => {
    const saved = await show();
    await fireEvent.click(screen.getByRole("switch", { name: "Reduce motion" }));
    await waitFor(() => expect(saved.at(-1)?.reduce_motion).toBe(true));
  });

  it("start_with_windows_reads_the_real_state_and_changes_it", async () => {
    const asked: unknown[] = [];
    await show({}, {
      autostart_get: () => true,
      autostart_set: (a) => {
        asked.push(a.on);
        return a.on;
      },
    });
    const sw = screen.getByRole("switch", { name: "Start with Windows" }) as HTMLInputElement;
    await waitFor(() => expect(sw.checked).toBe(true));
    await fireEvent.click(sw);
    await waitFor(() => expect(asked).toEqual([false]));
    expect(sw.checked).toBe(false);
  });

  it("start_with_windows_error_says_so_and_goes_back", async () => {
    await show({}, {
      autostart_set: () => {
        throw "Couldn't change Start with Windows.";
      },
    });
    const sw = screen.getByRole("switch", { name: "Start with Windows" }) as HTMLInputElement;
    await fireEvent.click(sw);
    expect(await screen.findByText("Couldn't change Start with Windows.")).toBeTruthy();
    expect(sw.checked).toBe(false);
  });
});
