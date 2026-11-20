import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writable } from "svelte/store";
import { resetLibrary } from "$lib/library/library";

/** Mirrors the Rust `AuthState` enum (tagged by `kind`). */
export type AuthState =
  | { kind: "SignedOut" }
  | { kind: "SignedIn"; display_name: string; account_id: string }
  | { kind: "Offline"; display_name: string };

export type ViewState = AuthState | { kind: "Loading" };

export const authState = writable<ViewState>({ kind: "Loading" });
/** True while the Epic login window is open or a code is being exchanged. */
export const signingIn = writable(false);
/** User-facing error from the last sign-in attempt ("" when none). */
export const loginError = writable("");

const message = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong. Please try again.");

let unlisteners: UnlistenFn[] = [];

/** (Re)subscribes to the login events coming from Rust. */
async function subscribe() {
  for (const off of unlisteners) {
    try {
      off();
    } catch {
      /* already gone */
    }
  }
  unlisteners = await Promise.all([
    listen<AuthState>("auth-changed", (e) => {
      authState.set(e.payload);
      signingIn.set(false);
    }),
    listen("login-window-closed", () => signingIn.set(false)),
    listen<string>("login-failed", (e) => loginError.set(e.payload)),
  ]);
}

/** Call at app start: subscribes to login events and loads the saved session. */
export async function init() {
  await subscribe();
  authState.set(await invoke<AuthState>("auth_state"));
}

/** Opens Epic's login window. */
export async function signIn() {
  loginError.set("");
  signingIn.set(true);
  try {
    await invoke("start_login");
  } catch (e) {
    signingIn.set(false);
    loginError.set(message(e));
  }
}

/** True while a pasted code is being checked (separate from the login window's state). */
export const submitting = writable(false);

/** Backup flow: exchange a pasted code (or Epic's whole JSON). */
export async function submitCode(text: string) {
  loginError.set("");
  submitting.set(true);
  try {
    authState.set(await invoke<AuthState>("submit_code", { input: text }));
    signingIn.set(false);
  } catch (e) {
    loginError.set(message(e));
  } finally {
    submitting.set(false);
  }
}

export async function openBrowserLogin() {
  try {
    await invoke("open_login_in_browser");
  } catch (e) {
    loginError.set(message(e));
  }
}

/** Logs out. Resolves to false (and leaves the state alone) if it couldn't. */
export async function signOut(): Promise<boolean> {
  try {
    authState.set(await invoke<AuthState>("logout"));
    resetLibrary(); // the previous account's games must not stay in memory
    return true;
  } catch {
    return false;
  }
}

/** "Try again" when offline. Keeps the current state if the check itself fails. */
export async function retry() {
  try {
    authState.set(await invoke<AuthState>("retry_auth"));
  } catch {
    /* still offline: the banner stays */
  }
}
