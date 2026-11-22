import { invoke } from "@tauri-apps/api/core";
import { authState, loginError } from "$lib/auth/auth";
import { resetLibrary } from "$lib/library/library";
import { resetInstalls } from "$lib/install/installs";
import { resetSettings } from "./settings";

/** Mirrors the app's `DeleteReport` (spec §6f.1). */
export type DeleteReport = { failed: number; message: string | null };

const LOGS_ERROR = "Couldn't open the logs folder. It's at %APPDATA%\\FishyClient\\logs.";
export const DELETE_ERROR = "Couldn't delete your data. Please try again.";

/** Opens the logs folder in File Explorer. Returns an error message, or null when it worked. */
export async function openLogsFolder(): Promise<string | null> {
  try {
    await invoke("open_logs_folder");
    return null;
  } catch (e) {
    return typeof e === "string" ? e : LOGS_ERROR;
  }
}

/**
 * Delete all my data. On success the app is signed out and shows Login; anything that couldn't be
 * deleted is reported there. Returns an error message if the command itself failed.
 */
export async function deleteAllData(alsoGames: boolean, alsoEpic = false): Promise<string | null> {
  let report: DeleteReport;
  try {
    report = await invoke<DeleteReport>("delete_all_data", { alsoGames, alsoEpic });
  } catch (e) {
    // The app's own refusal ("A download wouldn't stop…") is a plain message; anything else isn't.
    return typeof e === "string" ? e : DELETE_ERROR;
  }
  resetLibrary();
  resetInstalls();
  resetSettings();
  loginError.set(report?.message ?? "");
  authState.set({ kind: "SignedOut" });
  return null;
}

/** The speed box: whole numbers from 1 to 1000 (MB/s), else null. */
export function parseSpeed(text: string): number | null {
  const t = text.trim();
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  return n >= 1 && n <= 1000 ? n : null;
}

/** `%USERPROFILE%\Games`, shown when no default folder is saved. */
export async function defaultInstallFolder(): Promise<string> {
  try {
    return await invoke<string>("default_install_folder");
  } catch {
    return "";
  }
}

const AUTOSTART_ERROR = "Couldn't change Start with Windows.";

/** Whether Fishy Client starts with Windows (false if that can't be read). */
export async function getAutostart(): Promise<boolean> {
  try {
    return (await invoke<boolean>("autostart_get")) === true;
  } catch {
    return false;
  }
}

/** Turns Start with Windows on or off. Returns the real state, or an error message. */
export async function setAutostart(on: boolean): Promise<boolean | string> {
  try {
    return (await invoke<boolean>("autostart_set", { on })) === true;
  } catch (e) {
    return typeof e === "string" ? e : AUTOSTART_ERROR;
  }
}
