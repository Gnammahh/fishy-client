import { describe, it, expect } from "vitest";
import { clampProgress, dotsEaten, formatBytes, formatProgress, statusText } from "./progress";

const GiB = 1024 ** 3;
const MiB = 1024 ** 2;

describe("progress", () => {
  it("formats the spec example", () => {
    expect(formatProgress(3.1 * GiB, 7.4 * GiB, 18.2 * MiB)).toBe("42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left");
  });

  it("unknown total shows 0% and no infinity", () => {
    expect(formatProgress(0, 0, 0)).toBe("0% · 0.0 B / — · 0.0 B/s · —");
  });

  it("unknown total still shows the bytes already downloaded", () => {
    expect(formatProgress(500 * MiB, 0, MiB)).toBe("0% · 500.0 MB / — · 1.0 MB/s · —");
  });

  it("NaN and Infinity never reach the text", () => {
    for (const s of [
      formatProgress(NaN, NaN, NaN),
      formatProgress(MiB, Infinity, Infinity),
      formatProgress(Infinity, GiB, MiB),
      statusText("paused", NaN, NaN, NaN),
      statusText("done", NaN, NaN, NaN),
    ]) {
      expect(s).not.toMatch(/NaN|Infinity|∞/);
    }
  });

  it("never shows 100% before the download is complete", () => {
    expect(formatProgress(0.995 * GiB, GiB, MiB).startsWith("99%")).toBe(true);
    expect(formatProgress(GiB, GiB, MiB).startsWith("100%")).toBe(true);
    expect(statusText("verifying", 0.999 * GiB, GiB, 0)).toBe("Checking installed files, 99% done.");
  });

  it("stalled speed shows dash", () => {
    expect(formatProgress(GiB, 2 * GiB, 0).endsWith("· —")).toBe(true);
  });

  it("seconds when under a minute", () => {
    expect(formatProgress(0, 30 * MiB, MiB).endsWith("~30 s left")).toBe(true);
  });

  it("clampProgress handles out-of-range", () => {
    expect(clampProgress(1.0000001)).toBe(1);
    expect(clampProgress(-0.2)).toBe(0);
    expect(clampProgress(NaN)).toBe(0);
  });

  it("dotsEaten never exceeds dotCount", () => {
    expect(dotsEaten(1.0000001, 40)).toBe(40);
    expect(dotsEaten(0.5, 40)).toBe(20);
  });

  it("status text matches the mockup wording for each state", () => {
    const total = 7.4 * GiB;
    expect(statusText("paused", 3.1 * GiB, total, 0)).toBe("Paused at 42%. 3.1 GB of 7.4 GB downloaded.");
    expect(statusText("verifying", 0.7 * total, total, 0)).toBe("Checking installed files, 70% done.");
    expect(statusText("error", 0.42 * total, total, 0)).toBe("Download stopped: the connection to the download server dropped. Check your internet, then press Retry.");
    expect(statusText("done", total, total, 0)).toBe("Installed. 7.4 GB on disk.");
    expect(statusText("downloading", GiB, 2 * GiB, 0)).toBe("50% · 1.0 GB / 2.0 GB · 0.0 B/s · —");
  });

  it("status text uses the real downloaded bytes when the total is unknown", () => {
    expect(statusText("paused", 500 * MiB, 0, 0)).toBe("Paused at 0%. 500.0 MB of — downloaded.");
    expect(statusText("done", 500 * MiB, 0, 0)).toBe("Installed. 500.0 MB on disk.");
  });

  it("formatBytes uses binary units with one decimal", () => {
    expect(formatBytes(512)).toBe("512.0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(7.4 * GiB)).toBe("7.4 GB");
  });
});
