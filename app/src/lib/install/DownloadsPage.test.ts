import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import DownloadsPage from "./DownloadsPage.svelte";
import { downloads, installed, resetInstalls, type DownloadItem } from "./installs";

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
});

describe("DownloadsPage", () => {
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
});
