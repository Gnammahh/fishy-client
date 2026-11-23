import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import ManageDialog from "./ManageDialog.svelte";
import { installed, resetInstalls, verifying, type GameOptions, type InstalledGame } from "$lib/install/installs";
import { settings, resetSettings, DEFAULT_SETTINGS } from "$lib/settings/settings";
import type { Game } from "./library";

const game: Game = {
  app_name: "SolCesto",
  namespace: "ns",
  catalog_item_id: "cat",
  title: "Sol Cesto",
  cover_url: null,
  installable: true,
  launch: {},
} as Game;
const options = (o: Partial<GameOptions> = {}): GameOptions => ({ auto_update: null, priority: false, launch_args: "", launch_args_on: false, shortcuts: [], ...o });
const record = (o: Partial<GameOptions> = {}, extra: Partial<InstalledGame> = {}): InstalledGame => ({
  app_name: "SolCesto",
  title: "Sol Cesto",
  version: "1",
  install_path: "C:\\Games\\Sol Cesto",
  executable: "Sol.exe",
  launch_parameters: "",
  install_size: 242_960_000,
  installed_at: "2026-11-21T00:00:00Z",
  playtime_secs: 12 * 3600 + 5 * 60,
  options: options(o),
  ...extra,
});

/** Records every command; game_options_set / shortcut_* answer with the record they'd save. */
function mock(handlers: Record<string, (args: any) => unknown> = {}) {
  const calls: [string, any][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    if (handlers[cmd]) return handlers[cmd](args);
    if (cmd === "game_options_set") {
      const a = args as any;
      return record({ auto_update: a.autoUpdate, priority: a.priority, launch_args: a.launchArgs, launch_args_on: a.launchArgsOn });
    }
    return null;
  });
  return calls;
}

function show(props: Partial<{ installed: InstalledGame; running: boolean; hasDownload: boolean; sharedWithEpic: boolean }> = {}) {
  const onclose = vi.fn();
  const onuninstall = vi.fn();
  const rec = props.installed ?? record();
  installed.set([rec]);
  const { unmount } = render(ManageDialog, { game, installed: rec, running: false, hasDownload: false, sharedWithEpic: false, onclose, onuninstall, ...props });
  return { onclose, onuninstall, unmount };
}

afterEach(() => {
  cleanup();
  clearMocks();
  resetInstalls();
  resetSettings();
  vi.useRealTimers();
});

describe("ManageDialog", () => {
  it("is titled with the game and has the five rows", () => {
    mock();
    show({ sharedWithEpic: true });
    expect(screen.getByRole("dialog", { name: "Sol Cesto" })).toBeTruthy();
    for (const name of ["Auto update", "Priority download", "Launch options"]) expect(screen.getByRole("switch", { name })).toBeTruthy();
    expect(screen.getByText("Shortcut")).toBeTruthy();
    expect(screen.getByText("Installation")).toBeTruthy();
    expect(screen.getByText("Updates this game by itself when a new version is out.")).toBeTruthy();
    expect(screen.getByText("When downloads wait in line, this game goes first.")).toBeTruthy();
    expect(screen.getByText("Size: 231.7 MB")).toBeTruthy();
    expect(screen.getByText("C:\\Games\\Sol Cesto")).toBeTruthy();
    expect(screen.getByText("Played: 12 h 5 min")).toBeTruthy();
    expect(screen.getByText("Also in Epic's launcher. Updating in one may make the other update again.")).toBeTruthy();
  });

  it("auto update shows the Settings value until the game has its own, and saves it", async () => {
    const calls = mock();
    settings.set({ ...DEFAULT_SETTINGS, auto_update: true });
    show();
    const auto = screen.getByRole("switch", { name: "Auto update" }) as HTMLInputElement;
    expect(auto.checked).toBe(true);
    await fireEvent.click(auto);
    await waitFor(() => expect(calls).toContainEqual(["game_options_set", { appName: "SolCesto", autoUpdate: false, priority: false, launchArgs: "", launchArgsOn: false }]));
    cleanup();
    show({ installed: record({ auto_update: false }) });
    expect((screen.getByRole("switch", { name: "Auto update" }) as HTMLInputElement).checked).toBe(false);
  });

  it("priority and launch options save", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const calls = mock();
    show();
    await fireEvent.click(screen.getByRole("switch", { name: "Priority download" }));
    await waitFor(() => expect(calls.at(-1)?.[1]).toMatchObject({ priority: true }));
    await fireEvent.click(screen.getByRole("switch", { name: "Launch options" }));
    await waitFor(() => expect(calls.at(-1)?.[1]).toMatchObject({ launchArgsOn: true }));
    const box = screen.getByRole("textbox", { name: "Launch options text" });
    await fireEvent.input(box, { target: { value: "-windowed" } });
    vi.advanceTimersByTime(600);
    await waitFor(() => expect(calls.at(-1)?.[1]).toMatchObject({ launchArgs: "-windowed", launchArgsOn: true }));
  });

  it("launch options typed just before closing are still saved", async () => {
    const calls = mock();
    const { unmount } = show();
    await fireEvent.input(screen.getByRole("textbox", { name: "Launch options text" }), { target: { value: "-windowed" } });
    unmount();
    await waitFor(() => expect(calls.at(-1)).toEqual(["game_options_set", { appName: "SolCesto", autoUpdate: null, priority: false, launchArgs: "-windowed", launchArgsOn: false }]));
  });

  it("a switch still saves while the launch text is too long", async () => {
    const calls = mock();
    show();
    await fireEvent.input(screen.getByRole("textbox", { name: "Launch options text" }), { target: { value: "a".repeat(1001) } });
    await fireEvent.click(screen.getByRole("switch", { name: "Priority download" }));
    await waitFor(() => expect(calls.at(-1)?.[1]).toMatchObject({ priority: true, launchArgs: "" }));
    expect(screen.getByText("Keep it under 1000 characters.")).toBeTruthy();
  });

  it("a double click on Create makes one shortcut", async () => {
    let release: (v: unknown) => void = () => {};
    const calls = mock({ shortcut_create: () => new Promise((r) => (release = () => r(record({ shortcuts: ["C:\D\Sol Cesto.lnk"] })))) });
    show();
    const create = screen.getByRole("button", { name: "Create" });
    await fireEvent.click(create);
    await fireEvent.click(create);
    release(null);
    expect(await screen.findByText("Also add it to the Start menu?")).toBeTruthy();
    expect(calls.filter(([c]) => c === "shortcut_create")).toHaveLength(1);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("refuses launch options over 1000 characters", async () => {
    const calls = mock();
    show();
    const box = screen.getByRole("textbox", { name: "Launch options text" });
    await fireEvent.input(box, { target: { value: "a".repeat(1001) } });
    await fireEvent.blur(box);
    expect(await screen.findByText("Keep it under 1000 characters.")).toBeTruthy();
    expect(calls.filter(([c]) => c === "game_options_set")).toEqual([]);
  });

  it("creates a shortcut, asks about the Start menu, and removes them", async () => {
    const desk = "C:\\Users\\you\\Desktop\\Sol Cesto.lnk";
    const start = "C:\\Users\\you\\Start Menu\\Fishy Client\\Sol Cesto.lnk";
    const calls = mock({
      shortcut_create: () => record({ shortcuts: [desk] }),
      shortcut_add_start_menu: () => record({ shortcuts: [desk, start] }),
      shortcut_remove: () => record(),
    });
    show();
    expect(screen.getByText("Start this game from your desktop.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("Also add it to the Start menu?")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Yes" }));
    await waitFor(() => expect(calls.map(([c]) => c)).toContain("shortcut_add_start_menu"));
    expect(await screen.findByText("On your desktop and in the Start menu")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(calls.map(([c]) => c)).toContain("shortcut_remove"));
    expect(await screen.findByRole("button", { name: "Create" })).toBeTruthy();
  });

  it("No keeps only the desktop shortcut", async () => {
    mock({ shortcut_create: () => record({ shortcuts: ["C:\\D\\Sol Cesto.lnk"] }) });
    show();
    await fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await fireEvent.click(await screen.findByRole("button", { name: "No" }));
    expect(await screen.findByText("On your desktop")).toBeTruthy();
    expect(screen.queryByText("Also add it to the Start menu?")).toBeNull();
  });

  it("verify is greyed out with the reason", () => {
    mock();
    show({ running: true });
    const v = screen.getByRole("button", { name: "Verify" }) as HTMLButtonElement;
    expect(v.disabled).toBe(true);
    expect(v.title).toBe("Close the game first.");
    expect((screen.getByRole("button", { name: "Uninstall" }) as HTMLButtonElement).disabled).toBe(true);
    cleanup();
    show({ hasDownload: true });
    expect((screen.getByRole("button", { name: "Verify" }) as HTMLButtonElement).title).toBe("Finish or cancel its download first.");
    cleanup();
    verifying.set({ app: "Other", done: 1, total: 9 });
    show();
    expect((screen.getByRole("button", { name: "Verify" }) as HTMLButtonElement).title).toBe("Another game is being checked.");
  });

  it("verify shows progress, then the result and Download them", async () => {
    const calls = mock({ verify_repair: () => ({ items: [] }) });
    show();
    await fireEvent.click(screen.getByRole("button", { name: "Verify" }));
    expect(calls.map(([c]) => c)).toContain("verify_start");
    verifying.set({ app: "SolCesto", done: 1234, total: 21000 });
    expect(await screen.findByText("Checking files… 1,234 / 21,000")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(calls.map(([c]) => c)).toContain("verify_stop");
    verifying.set({ app: "SolCesto", result: { kind: "bad", files: 12, bytes: 340 * 1024 * 1024 } });
    expect(await screen.findByText("12 files are missing or damaged (340.0 MB).")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Download them" }));
    await waitFor(() => expect(calls.map(([c]) => c)).toContain("verify_repair"));
    verifying.set({ app: "SolCesto", result: { kind: "good" } });
    expect(await screen.findByText("All files are fine.")).toBeTruthy();
  });

  it("uninstall and close", async () => {
    mock();
    const { onclose, onuninstall } = show();
    await fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    expect(onuninstall).toHaveBeenCalled();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onclose).toHaveBeenCalledTimes(2);
  });
});
