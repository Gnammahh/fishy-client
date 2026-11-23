import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, mockWindows, clearMocks } from "@tauri-apps/api/mocks";
import TitleBar from "./TitleBar.svelte";

afterEach(() => {
  cleanup();
  clearMocks();
});

function setup(maximized = false) {
  mockWindows("main");
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    if (cmd === "plugin:window|is_maximized") return maximized;
    return null;
  });
  return calls;
}

describe("TitleBar (spec §6s)", () => {
  it("shows_the_name_and_is_a_drag_region", () => {
    setup();
    render(TitleBar);
    const bar = screen.getByText("Fishy Client").closest("header");
    expect(bar?.hasAttribute("data-tauri-drag-region")).toBe(true);
  });

  it("buttons_minimize_maximize_and_close_the_window", async () => {
    const calls = setup();
    render(TitleBar);
    await fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    await fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(calls).toEqual(expect.arrayContaining(["plugin:window|minimize", "plugin:window|toggle_maximize", "plugin:window|close"])));
  });

  it("a_maximized_window_offers_restore", async () => {
    setup(true);
    render(TitleBar);
    expect(await screen.findByRole("button", { name: "Restore" })).toBeTruthy();
  });
});
