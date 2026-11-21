import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import LoginScreen from "./LoginScreen.svelte";
import AppShell from "$lib/shell/AppShell.svelte";
import { authState, init, signingIn, loginError } from "./auth";
import { library, resetLibrary } from "$lib/library/library";

const NOT_A_CODE = "That doesn't look like a sign-in code. Paste the authorizationCode value.";

function mockCommands(handlers: Record<string, (args: unknown) => unknown>) {
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
  vi.restoreAllMocks();
  signingIn.set(false);
  loginError.set("");
  authState.set({ kind: "Loading" });
  resetLibrary();
});

describe("LoginScreen", () => {
  it("login_screen_shows_sign_in_button", () => {
    mockCommands({});
    render(LoginScreen);
    expect(screen.getByRole("button", { name: "Sign in with Epic" })).toBeTruthy();
    expect(screen.getByText(/never sees your password/)).toBeTruthy();
  });

  it("sign_in_button_calls_start_login_and_shows_loader", async () => {
    const calls = mockCommands({ start_login: () => null });
    render(LoginScreen);
    await fireEvent.click(screen.getByRole("button", { name: "Sign in with Epic" }));
    await waitFor(() => expect(calls).toContain("start_login"));
    expect(screen.getByText("Finish signing in in the Epic window.")).toBeTruthy();
  });

  it("closing_the_login_window_brings_the_button_back", async () => {
    mockCommands({ start_login: () => null, auth_state: () => ({ kind: "SignedOut" }) });
    await init();
    render(LoginScreen);
    await fireEvent.click(screen.getByRole("button", { name: "Sign in with Epic" }));
    await screen.findByText("Finish signing in in the Epic window.");
    await emit("login-window-closed");
    expect(await screen.findByRole("button", { name: "Sign in with Epic" })).toBeTruthy();
  });

  it("paste_fallback_submits_text_and_shows_error_message", async () => {
    const calls = mockCommands({
      submit_code: () => {
        throw NOT_A_CODE;
      },
    });
    render(LoginScreen);
    await fireEvent.click(screen.getByRole("button", { name: "Trouble signing in? Paste a code instead" }));
    const box = screen.getByLabelText("Sign-in code");
    await fireEvent.input(box, { target: { value: "hello" } });
    await fireEvent.click(screen.getByRole("button", { name: "Sign in with code" }));
    await waitFor(() => expect(calls).toContain("submit_code"));
    expect((await screen.findByRole("alert")).textContent).toContain(NOT_A_CODE);
  });
});

describe("AppShell", () => {
  it("shell_shows_signed_in_name", () => {
    mockCommands({});
    render(AppShell, { state: { kind: "SignedIn", display_name: "FishFan", account_id: "acc123" } });
    expect(
      screen.getByText((_, el) => el?.tagName === "P" && (el.textContent ?? "").startsWith("Signed in as FishFan.")),
    ).toBeTruthy();
  });

  it("offline_shows_banner_with_try_again", () => {
    mockCommands({});
    render(AppShell, { state: { kind: "Offline", display_name: "FishFan" } });
    expect(screen.getByText("Couldn't reach Epic. Check your internet connection.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });

  it("account_log_out_calls_logout", async () => {
    const calls = mockCommands({ logout: () => ({ kind: "SignedOut" }) });
    render(AppShell, { state: { kind: "SignedIn", display_name: "FishFan", account_id: "acc123" } });
    await fireEvent.click(screen.getByRole("button", { name: "Account" }));
    await fireEvent.click(screen.getByRole("button", { name: "Log out" }));
    await waitFor(() => expect(calls).toContain("logout"));
    let current: unknown;
    authState.subscribe((v) => (current = v))();
    expect(current).toEqual({ kind: "SignedOut" });
  });

  it("log_out_forgets_the_previous_accounts_games", async () => {
    mockCommands({ logout: () => ({ kind: "SignedOut" }) });
    library.set({
      status: "ready",
      updating: false,
      latestVersions: {},
      error: "",
      games: [{ app_name: "A", namespace: "n", catalog_item_id: "a", title: "Private Game", cover_url: null, installable: true }],
    });
    render(AppShell, { state: { kind: "SignedIn", display_name: "FishFan", account_id: "acc123" } });
    await fireEvent.click(screen.getByRole("button", { name: "Account" }));
    await fireEvent.click(screen.getByRole("button", { name: "Log out" }));
    await waitFor(() => {
      let games: unknown[] = [1];
      library.subscribe((v) => (games = v.games))();
      expect(games).toEqual([]);
    });
  });

  it("try_again_is_disabled_while_checking", async () => {
    mockCommands({ retry_auth: () => new Promise(() => {}) }); // never answers
    render(AppShell, { state: { kind: "Offline", display_name: "FishFan" } });
    const button = screen.getByRole("button", { name: "Try again" }) as HTMLButtonElement;
    await fireEvent.click(button);
    await waitFor(() => expect(button.disabled).toBe(true));
  });

  it("failed_log_out_says_so_and_stays_signed_in", async () => {
    mockCommands({
      logout: () => {
        throw "ipc down";
      },
    });
    authState.set({ kind: "SignedIn", display_name: "FishFan", account_id: "acc123" });
    render(AppShell, { state: { kind: "SignedIn", display_name: "FishFan", account_id: "acc123" } });
    await fireEvent.click(screen.getByRole("button", { name: "Account" }));
    await fireEvent.click(screen.getByRole("button", { name: "Log out" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Couldn't log out. Please try again.");
    let current: unknown;
    authState.subscribe((v) => (current = v))();
    expect(current).toEqual({ kind: "SignedIn", display_name: "FishFan", account_id: "acc123" });
  });
});

describe("paste flow", () => {
  it("a_failed_paste_does_not_hide_the_open_login_window_state", async () => {
    mockCommands({
      start_login: () => null,
      submit_code: () => {
        throw NOT_A_CODE;
      },
    });
    render(LoginScreen);
    await fireEvent.click(screen.getByRole("button", { name: "Sign in with Epic" }));
    await waitFor(() => {
      let v = false;
      signingIn.subscribe((x) => (v = x))();
      expect(v).toBe(true);
    });
    await fireEvent.click(screen.getByRole("button", { name: "Trouble signing in? Paste a code instead" }));
    await fireEvent.input(screen.getByLabelText("Sign-in code"), { target: { value: "hello" } });
    await fireEvent.click(screen.getByRole("button", { name: "Sign in with code" }));
    await screen.findByRole("alert");
    let still = false;
    signingIn.subscribe((x) => (still = x))();
    expect(still).toBe(true); // the Epic window is still open
  });
});
