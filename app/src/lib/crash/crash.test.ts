import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import CrashHarness from "./CrashHarness.test.svelte";
import { reportUiError, installGlobalHandlers } from "./crash";

afterEach(() => {
  cleanup();
  clearMocks();
});

function backend(handler: (cmd: string, args: Record<string, unknown>) => unknown = () => null) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    return handler(cmd, (args ?? {}) as Record<string, unknown>);
  });
  return calls;
}

describe("crash screen", () => {
  it("a_screen_that_throws_shows_the_crash_page", async () => {
    const calls = backend();
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(CrashHarness);
    expect(screen.getByRole("heading", { name: "Something went wrong" })).toBeTruthy();
    expect(screen.getByText("Fishy Client hit a problem it didn't expect. Your games and downloads are safe.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open logs folder" })).toBeTruthy();
    await waitFor(() => expect(calls.filter((c) => c.cmd === "log_ui_error").length).toBe(1));
    expect(String(calls.find((c) => c.cmd === "log_ui_error")!.args.message)).toMatch(/^Error: boom/);
  });

  it("reload_calls_onreload", async () => {
    backend();
    vi.spyOn(console, "error").mockImplementation(() => {});
    const onreload = vi.fn();
    render(CrashHarness, { onreload });
    await fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    expect(onreload).toHaveBeenCalledOnce();
  });

  it("open_logs_folder_calls_the_command", async () => {
    const calls = backend();
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(CrashHarness);
    await fireEvent.click(screen.getByRole("button", { name: "Open logs folder" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_logs_folder")).toBe(true));
  });

  it("logging_that_fails_never_throws", async () => {
    backend((cmd) => {
      if (cmd === "log_ui_error") throw "ipc down";
      return null;
    });
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => reportUiError(new Error("x"))).not.toThrow();
    render(CrashHarness);
    expect(screen.getByRole("heading", { name: "Something went wrong" })).toBeTruthy();
  });

  it("an_unhandled_rejection_is_logged_and_nothing_else_changes", async () => {
    const calls = backend();
    const stop = installGlobalHandlers(window);
    const event = new Event("unhandledrejection") as Event & { reason?: unknown };
    event.reason = new TypeError("late failure");
    window.dispatchEvent(event);
    await waitFor(() => expect(calls.find((c) => c.cmd === "log_ui_error")?.args.message).toBe("TypeError: late failure"));
    expect(screen.queryByRole("heading", { name: "Something went wrong" })).toBeNull();
    stop();
  });

  it("long_messages_are_cut_to_500", async () => {
    const calls = backend();
    reportUiError(new Error("x".repeat(2000)));
    await waitFor(() => expect(String(calls.find((c) => c.cmd === "log_ui_error")?.args.message).length).toBe(500));
  });
});
