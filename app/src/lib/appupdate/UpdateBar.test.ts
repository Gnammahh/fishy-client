import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import UpdateBar from "./UpdateBar.svelte";
import { checkForUpdate, resetAppUpdate } from "./appUpdate";

const OFFER = { version: "1.2.0", notes: "Faster fish.\nFewer bugs." };

function mock(handlers: Record<string, (args: unknown) => unknown>) {
  const calls: string[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      const h = handlers[cmd];
      return h ? h(args) : undefined;
    },
    { shouldMockEvents: true },
  );
  return calls;
}

afterEach(() => {
  cleanup();
  clearMocks();
  resetAppUpdate();
});

describe("UpdateBar", () => {
  it("bar_shows_version_and_buttons", async () => {
    mock({ app_update_check: () => OFFER });
    await checkForUpdate();
    render(UpdateBar);
    expect(screen.getByText("Fishy Client 1.2.0 is available.")).toBeTruthy();
    for (const name of ["What's new", "Update now", "Later"]) expect(screen.getByRole("button", { name })).toBeTruthy();
  });

  it("no_bar_when_nothing_newer", async () => {
    mock({ app_update_check: () => null });
    await checkForUpdate();
    render(UpdateBar);
    expect(screen.queryByText(/is available/)).toBeNull();
  });

  it("a_failed_check_shows_nothing", async () => {
    mock({
      app_update_check: () => {
        throw "boom";
      },
    });
    await checkForUpdate();
    render(UpdateBar);
    expect(screen.queryByText(/is available/)).toBeNull();
  });

  it("whats_new_shows_the_notes", async () => {
    mock({ app_update_check: () => OFFER });
    await checkForUpdate();
    render(UpdateBar);
    await fireEvent.click(screen.getByRole("button", { name: "What's new" }));
    expect(screen.getByText(/Faster fish\./)).toBeTruthy();
  });

  it("later_hides_it", async () => {
    mock({ app_update_check: () => OFFER });
    await checkForUpdate();
    render(UpdateBar);
    await fireEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByText(/is available/)).toBeNull();
  });

  it("update_now_installs_and_shows_progress", async () => {
    const calls = mock({ app_update_check: () => OFFER, app_update_install: () => new Promise(() => {}) });
    await checkForUpdate();
    render(UpdateBar);
    await fireEvent.click(screen.getByRole("button", { name: "Update now" }));
    await waitFor(() => expect(calls).toContain("app_update_install"));
    await emit("app-update-progress", { percent: 42 });
    expect(await screen.findByText("Downloading the update… 42%")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Update now" })).toBeNull();
  });

  it("update_error_is_shown", async () => {
    mock({
      app_update_check: () => OFFER,
      app_update_install: () => {
        throw "Couldn't download the update. Try again later.";
      },
    });
    await checkForUpdate();
    render(UpdateBar);
    await fireEvent.click(screen.getByRole("button", { name: "Update now" }));
    expect(await screen.findByText("Couldn't download the update. Try again later.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Update now" })).toBeTruthy();
  });

  it("close_game_first_is_shown", async () => {
    mock({
      app_update_check: () => OFFER,
      app_update_install: () => {
        throw "Close Bendy first.";
      },
    });
    await checkForUpdate();
    render(UpdateBar);
    await fireEvent.click(screen.getByRole("button", { name: "Update now" }));
    expect(await screen.findByText("Close Bendy first.")).toBeTruthy();
  });
});
