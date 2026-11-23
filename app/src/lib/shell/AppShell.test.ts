import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import AppShell from "./AppShell.svelte";
import { playRequest, resetInstalls } from "$lib/install/installs";
import { resetSettings } from "$lib/settings/settings";

afterEach(() => {
  cleanup();
  clearMocks();
  resetInstalls();
  resetSettings();
});

describe("AppShell", () => {
  it("a_shortcut_play_request_shows_the_library", async () => {
    mockIPC(() => null, { shouldMockEvents: true });
    render(AppShell, { state: { kind: "SignedIn", display_name: "FishFan", account_id: "acc" } as never });
    await fireEvent.click(screen.getByRole("button", { name: /Downloads/ }));
    expect(await screen.findByRole("heading", { name: "Downloads" })).toBeTruthy();
    playRequest.set("KelpRunner");
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Downloads" })).toBeNull());
  });
});
