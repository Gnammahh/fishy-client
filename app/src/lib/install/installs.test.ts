import { describe, it, expect, afterEach } from "vitest";
import { get } from "svelte/store";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { downloads, installed, running, initInstalls, statusFor, uninstall, formatPlaytime, resetInstalls, type DownloadItem, type InstalledGame } from "./installs";

const item = (app_name: string, state: DownloadItem["state"]): DownloadItem => ({
  app_name,
  title: app_name,
  state,
  downloaded: 10,
  total: 100,
  bytes_per_sec: 0,
  error: null,
});
const inst = (app_name: string): InstalledGame => ({
  app_name,
  title: app_name,
  version: "1",
  install_path: "C:\\Games\\" + app_name,
  executable: "x.exe",
  launch_parameters: "",
  install_size: 1,
  installed_at: "2026-11-21T00:00:00Z",
});

afterEach(() => {
  clearMocks();
  resetInstalls();
});

describe("statusFor", () => {
  it("covers every state", () => {
    const dl = [item("D", "downloading"), item("Q", "queued"), item("P", "paused"), item("E", "error")];
    const ins = [inst("I")];
    expect(statusFor("D", dl, ins)).toBe("downloading");
    expect(statusFor("Q", dl, ins)).toBe("queued");
    expect(statusFor("P", dl, ins)).toBe("paused");
    expect(statusFor("E", dl, ins)).toBe("error");
    expect(statusFor("I", dl, ins)).toBe("installed");
    expect(statusFor("X", dl, ins)).toBe("none");
  });
});

describe("initInstalls", () => {
  it("fills the stores and follows events", async () => {
    let installedNow = [inst("A")];
    mockIPC(
      (cmd) => {
        if (cmd === "downloads_state") return { items: [item("B", "downloading")] };
        if (cmd === "installed_list") return installedNow;
        if (cmd === "running_list") return ["A"];
      },
      { shouldMockEvents: true },
    );
    const stop = await initInstalls();
    expect(get(downloads).map((d) => d.app_name)).toEqual(["B"]);
    expect(get(installed).map((g) => g.app_name)).toEqual(["A"]);

    await emit("downloads-changed", { items: [item("B", "paused"), item("C", "queued")] });
    expect(get(downloads).map((d) => d.state)).toEqual(["paused", "queued"]);

    installedNow = [inst("A"), inst("B")];
    await emit("installed-changed", null);
    await new Promise((r) => setTimeout(r, 0));
    expect(get(installed).map((g) => g.app_name)).toEqual(["A", "B"]);

    expect(get(running)).toEqual(["A"]);
    await emit("running-changed", ["B"]);
    expect(get(running)).toEqual(["B"]);
    await emit("running-changed", "nonsense");
    expect(get(running)).toEqual([]);
    stop();
  });
});

describe("uninstall", () => {
  it("returns the error message when it fails, null when it works", async () => {
    mockIPC((cmd) => {
      if (cmd === "uninstall_game") throw "Couldn't delete some files. Close the game and try again.";
    });
    expect(await uninstall("A")).toBe("Couldn't delete some files. Close the game and try again.");
    clearMocks();
    mockIPC((cmd) => (cmd === "uninstall_game" ? [] : null));
    installed.set([inst("A")]);
    expect(await uninstall("A")).toBeNull();
    expect(get(installed)).toEqual([]);
  });
});

describe("formatPlaytime", () => {
  it("formats_the_time_played", () => {
    expect(formatPlaytime(0)).toBeNull();
    expect(formatPlaytime(30)).toBe("Played: under a minute");
    expect(formatPlaytime(25 * 60 + 40)).toBe("Played: 25 min");
    expect(formatPlaytime(12 * 3600 + 5 * 60)).toBe("Played: 12 h 5 min");
    expect(formatPlaytime(2 * 3600)).toBe("Played: 2 h");
  });
});
