import { invoke } from "@tauri-apps/api/core";
import { asItems, downloads, message, type DownloadsView } from "$lib/install/installs";

/** Mirrors app/src-tauri epic_import.rs (spec §6g). */
export type Candidate = { app_name: string; title: string };
export type AddResult =
  | { kind: "added" }
  | { kind: "missing"; files: number; bytes: number }
  | { kind: "error"; message: string; signed_out: boolean };

/** Games from Epic's launcher that can be added. A failed scan counts as none. */
export async function scanEpic(): Promise<Candidate[]> {
  try {
    const list = await invoke<Candidate[]>("epic_import_scan");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

export async function addFromEpic(appName: string): Promise<AddResult> {
  try {
    return await invoke<AddResult>("epic_import_add", { appName });
  } catch (e) {
    return { kind: "error", message: message(e), signed_out: false };
  }
}

/** Queues the missing files. Returns an error message, or null when it worked. */
export async function repairFromEpic(appName: string): Promise<string | null> {
  try {
    downloads.set(asItems(await invoke<DownloadsView>("epic_import_repair", { appName })));
    return null;
  } catch (e) {
    return message(e);
  }
}
