export type DownloadState = "downloading" | "paused" | "verifying" | "error" | "done";

/** NaN → 0, otherwise clamped to [0, 1]. */
export function clampProgress(p: number): number {
  if (Number.isNaN(p)) return 0;
  return Math.min(1, Math.max(0, p));
}

export function dotsEaten(progress: number, dotCount: number): number {
  return Math.floor(clampProgress(progress) * dotCount);
}

/** Binary units (1 GB = 1024³ B), one decimal: "7.4 GB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = Math.max(0, bytes);
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(1)} ${units[i]}`;
}

/** "42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left" */
export function formatProgress(downloaded: number, total: number, bytesPerSec: number): string {
  const pct = total > 0 ? Math.round(clampProgress(downloaded / total) * 100) : 0;
  let left = "—";
  if (bytesPerSec > 0 && total > 0) {
    const seconds = Math.max(0, total - downloaded) / bytesPerSec;
    left = seconds < 60 ? `~${Math.round(seconds)} s left` : `~${Math.round(seconds / 60)} min left`;
  }
  return `${pct}% · ${formatBytes(downloaded)} / ${formatBytes(total)} · ${formatBytes(bytesPerSec)}/s · ${left}`;
}

/** Status line for every non-downloading state, worded as in the approved mockup. */
export function statusText(state: DownloadState, progress: number, total: number, bytesPerSec: number): string {
  const p = clampProgress(progress);
  const done = p * total;
  switch (state) {
    case "downloading":
      return formatProgress(done, total, bytesPerSec);
    case "paused":
      return `Paused at ${Math.round(p * 100)}%. ${formatBytes(done)} of ${formatBytes(total)} downloaded.`;
    case "verifying":
      return `Checking installed files, ${Math.round(p * 100)}% done.`;
    case "error":
      return "Download stopped: the connection to the download server dropped.";
    case "done":
      return `Installed. ${formatBytes(total)} on disk.`;
  }
}
