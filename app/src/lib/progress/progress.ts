export type DownloadState = "downloading" | "paused" | "verifying" | "error" | "done";

/** NaN → 0, otherwise clamped to [0, 1]. */
export function clampProgress(p: number): number {
  if (Number.isNaN(p)) return 0;
  return Math.min(1, Math.max(0, p));
}

export function dotsEaten(progress: number, dotCount: number): number {
  return Math.floor(clampProgress(progress) * dotCount);
}

/** A byte count that is safe to show: NaN, ±Infinity and negatives become 0. */
const safeBytes = (n: number) => (Number.isFinite(n) && n > 0 ? n : 0);
const knownTotal = (total: number) => safeBytes(total) > 0;

/** Binary units (1 GB = 1024³ B), one decimal: "7.4 GB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = safeBytes(bytes);
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(1)} ${units[i]}`;
}

/** Whole percent; never 100 until every byte is in. Unknown total → 0. */
export function percent(downloaded: number, total: number): number {
  if (!knownTotal(total)) return 0;
  const d = safeBytes(downloaded);
  if (d >= total) return 100;
  return Math.min(99, Math.round(clampProgress(d / total) * 100));
}

const totalText = (total: number) => (knownTotal(total) ? formatBytes(total) : "—");

/** "42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left" */
export function formatProgress(downloaded: number, total: number, bytesPerSec: number): string {
  const speed = safeBytes(bytesPerSec);
  let left = "—";
  if (speed > 0 && knownTotal(total)) {
    const seconds = Math.max(0, total - safeBytes(downloaded)) / speed;
    left = seconds < 60 ? `~${Math.round(seconds)} s left` : `~${Math.round(seconds / 60)} min left`;
  }
  return `${percent(downloaded, total)}% · ${formatBytes(downloaded)} / ${totalText(total)} · ${formatBytes(speed)}/s · ${left}`;
}

/** Status line for every state, worded as in the approved mockup. */
export function statusText(state: DownloadState, downloaded: number, total: number, bytesPerSec: number): string {
  switch (state) {
    case "downloading":
      return formatProgress(downloaded, total, bytesPerSec);
    case "paused":
      return `Paused at ${percent(downloaded, total)}%. ${formatBytes(downloaded)} of ${totalText(total)} downloaded.`;
    case "verifying":
      return `Checking installed files, ${percent(downloaded, total)}% done.`;
    case "error":
      return "Download stopped: the connection to the download server dropped. Check your internet, then press Retry.";
    case "done":
      return `Installed. ${formatBytes(knownTotal(total) ? total : downloaded)} on disk.`;
  }
}
