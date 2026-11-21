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
export async function deleteAllData(alsoGames: boolean): Promise<string | null> {
  let report: DeleteReport;
  try {
    report = await invoke<DeleteReport>("delete_all_data", { alsoGames });
  } catch {
    return DELETE_ERROR;
  }
  resetLibrary();
  resetInstalls();
  resetSettings();
  loginError.set(report?.message ?? "");
  authState.set({ kind: "SignedOut" });
  return null;
}
