import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import DownloadsPage from "./DownloadsPage.svelte";
import { downloads, installed, resetInstalls, type DownloadItem } from "./installs";
import { settings, resetSettings, DEFAULT_SETTINGS } from "$lib/settings/settings";

const MB = 1024 * 1024;
const item = (app_name: string, state: DownloadItem["state"], over: Partial<DownloadItem> = {}): DownloadItem => ({
  app_name,
  title: `Title ${app_name}`,
  state,
  downloaded: 42 * MB,
  total: 100 * MB,
  bytes_per_sec: 2 * MB,
  error: null,
  ...over,
});

function mock() {
  const calls: [string, unknown][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return null;
  });
  return calls;
}

afterEach(() => {
  cleanup();
  clearMocks();
  resetInstalls();
  resetSettings();
});

describe("DownloadsPage", () => {
  it("start_now_with_nothing_downloading_starts_it_without_asking", async () => {
    const calls = mock();
    downloads.set([item("A", "paused"), item("B", "queued")]);
    render(DownloadsPage);
    await fireEvent.click(screen.getByRole("button", { name: "Start Title B now" }));
    expect(calls).toContainEqual(["download_start_now", { appName: "B", mode: "pause" }]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("start_now_with_one_downloading_asks_pause_or_both", async () => {
    const calls = mock();
    downloads.set([item("A", "downloading"), item("B", "queued")]);
    render(DownloadsPage);
    await fireEvent.click(screen.getByRole("button", { name: "Start Title B now" }));
    expect(screen.getByRole("dialog", { name: "Start Title B now" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Pause Title A" }));
    await waitFor(() => expect(calls).toContainEqual(["download_start_now", { appName: "B", mode: "pause" }]));
    expect(screen.queryByRole("dialog")).toBeNull();
    downloads.set([item("A", "downloading"), item("B", "queued")]); // the mock answered with an empty list
    await fireEvent.click(await screen.findByRole("button", { name: "Start Title B now" }));
    await fireEvent.click(screen.getByRole("button", { name: "Download both" }));
    await waitFor(() => expect(calls).toContainEqual(["download_start_now", { appName: "B", mode: "both" }]));
  });

  it("start_now_is_greyed_with_two_downloading_and_both_show_as_downloading", async () => {
    mock();
    downloads.set([item("A", "downloading"), item("B", "downloading"), item("C", "queued")]);
    render(DownloadsPage);
    const btn = screen.getByRole("button", { name: "Start Title C now" }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.title).toBe("Two games are already downloading.");
    expect(screen.getAllByRole("progressbar")).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Pause" })).toHaveLength(2);
    expect(screen.queryByText("Paused")).toBeNull();
  });

  it("a_failed_start_now_shows_the_reason", async () => {
    mockIPC(() => {
      throw "That game isn't waiting.";
    });
    downloads.set([item("B", "queued")]);
    render(DownloadsPage);
    await fireEvent.click(screen.getByRole("button", { name: "Start Title B now" }));
    expect(await screen.findByText("That game isn't waiting.")).toBeTruthy();
  });

  it("empty_state", () => {
    render(DownloadsPage);
    expect(screen.getByText("Nothing downloading. Pick a game in your Library and press Install.")).toBeTruthy();
  });

  it("downloading_shows_the_fish_bar_and_pause_works", async () => {
    const calls = mock();
    downloads.set([item("A", "downloading")]);
    render(DownloadsPage);
    expect(screen.getByText("Title A")).toBeTruthy();
    expect(screen.getByRole("progressbar")).toBeTruthy();
    expect(screen.getByText(/^42% · 42\.0 MB \/ 100\.0 MB/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(calls).toContainEqual(["download_pause", { appName: "A" }]);
  });

  it("paused_offers_resume", async () => {
    const calls = mock();
    downloads.set([item("A", "paused")]);
    render(DownloadsPage);
    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    expect(calls).toContainEqual(["download_resume", { appName: "A" }]);
  });

  it("cancel_asks_first", async () => {
    const calls = mock();
    downloads.set([item("A", "downloading")]);
    render(DownloadsPage);
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByText("Stop and delete the downloaded files?")).toBeTruthy();
    expect(calls.map(([c]) => c)).not.toContain("download_cancel");
    await fireEvent.click(screen.getByRole("button", { name: "Delete files" }));
    await waitFor(() => expect(calls).toContainEqual(["download_cancel", { appName: "A" }]));
  });

  it("queue_lists_waiting_games_with_a_remove_button", async () => {
    const calls = mock();
    downloads.set([item("A", "downloading"), item("B", "queued"), item("C", "paused")]);
    render(DownloadsPage);
    expect(screen.getByText("Title B")).toBeTruthy();
    expect(screen.getByText("Title C")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Remove Title B from the queue" }));
    // A queued game may already have files on disk (a resumed download), so it asks too.
    expect(screen.getByText("Stop and delete the downloaded files?")).toBeTruthy();
    expect(calls.map(([c]) => c)).not.toContain("download_cancel");
    await fireEvent.click(screen.getByRole("button", { name: "Delete files" }));
    await waitFor(() => expect(calls).toContainEqual(["download_cancel", { appName: "B" }]));
  });

  it("a_failed_cancel_shows_the_reason", async () => {
    mockIPC((cmd) => {
      if (cmd === "download_cancel") throw "Couldn't delete some files. Close the game and try again.";
      return null;
    });
    downloads.set([item("A", "paused")]);
    render(DownloadsPage);
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await fireEvent.click(screen.getByRole("button", { name: "Delete files" }));
    expect(await screen.findByText("Couldn't delete some files. Close the game and try again.")).toBeTruthy();
  });

  it("error_shows_the_message_and_retry_resumes", async () => {
    const calls = mock();
    downloads.set([item("A", "error", { error: "A downloaded file was damaged. Press Retry to download it again." })]);
    render(DownloadsPage);
    expect(screen.getByText("A downloaded file was damaged. Press Retry to download it again.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(calls).toContainEqual(["download_resume", { appName: "A" }]);
  });

  it("a_finished_download_leaves_a_note", async () => {
    mock();
    downloads.set([item("A", "downloading")]);
    render(DownloadsPage);
    downloads.set([]);
    installed.set([
      { app_name: "A", title: "Title A", version: "1", install_path: "C:\\G\\A", executable: "", launch_parameters: "", install_size: 1, installed_at: "x" },
    ]);
    expect(await screen.findByText("Finished: Title A. It's in your Library.")).toBeTruthy();
  });
  it("an_update_can_be_cancelled_and_keeps_the_version_you_have", async () => {
    const calls = mock();
    downloads.set([item("A", "downloading", { update: true }), item("B", "queued", { update: true })]);
    render(DownloadsPage);
    expect(screen.getByText("Updating…")).toBeTruthy();
    expect(screen.queryByText("Updates can be paused but not cancelled.")).toBeNull();
    expect(screen.getByRole("button", { name: "Pause" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByText("Cancel the update? What it downloaded is deleted, and Title A stays at the version you have.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel update" }));
    await waitFor(() => expect(calls).toContainEqual(["download_cancel", { appName: "A" }]));
    await fireEvent.click(screen.getByRole("button", { name: "Remove Title B from the queue" }));
    expect(screen.getByText("Cancel the update? What it downloaded is deleted, and Title B stays at the version you have.")).toBeTruthy();
  });

  it("an_update_that_already_changed_files_offers_no_cancel_and_says_why", async () => {
    mock();
    downloads.set([item("A", "paused", { update: true, can_cancel: false }), item("B", "queued", { update: true, can_cancel: false })]);
    render(DownloadsPage);
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove Title B from the queue" })).toBeNull();
    expect(screen.getByText("This update started before 1.9.5 and already changed some files, so it can only be paused or finished.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Resume" })).toBeTruthy();
  });

  it("speed_limit_line_shows_when_a_limit_is_on", async () => {
    settings.set({ ...DEFAULT_SETTINGS, speed_limit_mbps: 1 });
    let opened = 0;
    render(DownloadsPage, { onsettings: () => opened++ });
    expect(screen.getByText(/Limited to 1 MB\/s/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Change in Settings" }));
    expect(opened).toBe(1);
  });

  it("no_speed_limit_line_without_a_limit", () => {
    render(DownloadsPage);
    expect(screen.queryByText(/Limited to/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Change in Settings" })).toBeNull();
  });
});
