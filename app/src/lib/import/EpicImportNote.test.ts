import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import EpicImportNote from "./EpicImportNote.svelte";
import { resetInstalls } from "$lib/install/installs";
import { resetEpicImport } from "./epicImport";

const MB = 1024 * 1024;
const games = (n: number) => Array.from({ length: n }, (_, i) => ({ app_name: `A${i}`, title: `Game ${i}` }));

/** Records every call; `answers` gives each command's reply (a function gets the args). */
function mock(answers: Record<string, unknown>) {
  const calls: [string, unknown][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    const a = answers[cmd];
    if (a instanceof Error) throw a.message;
    return typeof a === "function" ? a(args) : (a ?? null);
  });
  return calls;
}

afterEach(() => {
  cleanup();
  clearMocks();
  resetInstalls();
  resetEpicImport();
});

describe("EpicImportNote", () => {
  it("shows_how_many_games_can_be_added", async () => {
    mock({ epic_import_scan: games(3) });
    render(EpicImportNote);
    expect(await screen.findByText("3 games installed by Epic's launcher can be added.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add them" })).toBeTruthy();
  });

  it("says_game_for_one", async () => {
    mock({ epic_import_scan: games(1) });
    render(EpicImportNote);
    expect(await screen.findByText("1 game installed by Epic's launcher can be added.")).toBeTruthy();
  });

  it("nothing_when_none_or_the_scan_fails", async () => {
    const calls = mock({ epic_import_scan: [] });
    render(EpicImportNote);
    await waitFor(() => expect(calls.map((c) => c[0])).toContain("epic_import_scan"));
    expect(screen.queryByRole("button", { name: "Add them" })).toBeNull();
    cleanup();
    mock({ epic_import_scan: new Error("boom") });
    render(EpicImportNote);
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByRole("button", { name: "Add them" })).toBeNull();
  });

  it("the_x_hides_it", async () => {
    mock({ epic_import_scan: games(2) });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Hide" }));
    expect(screen.queryByText(/can be added/)).toBeNull();
  });

  it("add_them_adds_each_game_in_order_then_refreshes_installed", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    const calls = mock({
      epic_import_scan: games(2),
      epic_import_add: async () => {
        await gate;
        return { kind: "added" };
      },
      installed_list: [],
    });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    expect(await screen.findByText("Checking Game 0…")).toBeTruthy();
    release();
    await waitFor(() => expect(calls.map((c) => c[0])).toContain("installed_list"));
    const adds = calls.filter((c) => c[0] === "epic_import_add").map((c) => c[1]);
    expect(adds).toEqual([{ appName: "A0" }, { appName: "A1" }]);
    await waitFor(() => expect(screen.queryByText(/can be added/)).toBeNull());
  });

  it("missing_files_offer_download_them", async () => {
    const calls = mock({
      epic_import_scan: games(1),
      epic_import_add: { kind: "missing", files: 12, bytes: 340 * MB },
      epic_import_repair: { items: [] },
      installed_list: [],
    });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    expect(await screen.findByText("Game 0: 12 files are missing or damaged (340.0 MB).")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Download them" }));
    expect(calls).toContainEqual(["epic_import_repair", { appName: "A0" }]);
    await waitFor(() => expect(screen.queryByText(/missing or damaged/)).toBeNull());
  });

  it("one_missing_file_is_singular", async () => {
    mock({ epic_import_scan: games(1), epic_import_add: { kind: "missing", files: 1, bytes: 396 }, installed_list: [] });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    expect(await screen.findByText("Game 0: 1 file is missing or damaged (396.0 B).")).toBeTruthy();
  });

  it("an_error_shows_its_message", async () => {
    const message = "Couldn't read Game 0's files from Epic's launcher. Press Install to download it.";
    mock({ epic_import_scan: games(1), epic_import_add: { kind: "error", message, signed_out: false }, installed_list: [] });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    expect(await screen.findByText(message)).toBeTruthy();
  });

  it("a_failed_repair_shows_why", async () => {
    mock({
      epic_import_scan: games(1),
      epic_import_add: { kind: "missing", files: 2, bytes: 10 },
      epic_import_repair: new Error("Couldn't reach Epic to check Game 0. Check your internet."),
      installed_list: [],
    });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Download them" }));
    expect(await screen.findByText("Couldn't reach Epic to check Game 0. Check your internet.")).toBeTruthy();
  });

  it("hidden_stays_hidden_after_leaving_the_library", async () => {
    const calls = mock({ epic_import_scan: games(2) });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Hide" }));
    cleanup();
    render(EpicImportNote);
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByText(/can be added/)).toBeNull();
    expect(calls.filter((c) => c[0] === "epic_import_scan").length).toBe(1);
  });

  it("lines_and_a_running_check_survive_leaving_the_library", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    const calls = mock({
      epic_import_scan: games(2),
      epic_import_add: async (args: { appName: string }) => {
        if (args.appName === "A1") await gate;
        return args.appName === "A0" ? { kind: "missing", files: 3, bytes: 10 } : { kind: "added" };
      },
      installed_list: [],
    });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    await screen.findByText("Checking Game 1…");
    cleanup();
    render(EpicImportNote);
    expect(screen.getByText("Checking Game 1…")).toBeTruthy();
    expect(screen.getByText("Game 0: 3 files are missing or damaged (10.0 B).")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add them" })).toBeNull();
    release();
    await waitFor(() => expect(screen.queryByText("Checking Game 1…")).toBeNull());
    expect(calls.filter((c) => c[0] === "epic_import_add").length).toBe(2);
  });

  it("download_them_works_once_while_busy", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    const calls = mock({
      epic_import_scan: games(1),
      epic_import_add: { kind: "missing", files: 2, bytes: 10 },
      epic_import_repair: async () => {
        await gate;
        return { items: [] };
      },
      installed_list: [],
    });
    render(EpicImportNote);
    await fireEvent.click(await screen.findByRole("button", { name: "Add them" }));
    const button = await screen.findByRole("button", { name: "Download them" });
    await fireEvent.click(button);
    await fireEvent.click(button);
    expect((button as HTMLButtonElement).disabled).toBe(true);
    release();
    await waitFor(() => expect(screen.queryByText(/missing or damaged/)).toBeNull());
    expect(calls.filter((c) => c[0] === "epic_import_repair").length).toBe(1);
  });
});
