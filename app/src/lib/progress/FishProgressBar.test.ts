import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import FishProgressBar from "./FishProgressBar.svelte";

const GiB = 1024 ** 3;
const MiB = 1024 ** 2;

afterEach(() => cleanup());

describe("FishProgressBar", () => {
  it("is exposed as a progress bar with its value", () => {
    render(FishProgressBar, { downloaded: 3.1 * GiB, total: 7.4 * GiB, bytesPerSec: 18.2 * MiB, state: "downloading" });
    const bar = screen.getByRole("progressbar");
    expect(bar.getAttribute("aria-valuemin")).toBe("0");
    expect(bar.getAttribute("aria-valuemax")).toBe("100");
    expect(bar.getAttribute("aria-valuenow")).toBe("42");
    expect(bar.getAttribute("aria-valuetext")).toBe("42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left");
  });

  it("the constantly changing status line is not a live region", () => {
    render(FishProgressBar, { downloaded: GiB, total: 2 * GiB, bytesPerSec: MiB, state: "downloading" });
    const line = screen.getByText(/^50% ·/);
    expect(line.closest("[aria-live]")).toBeNull();
  });

  it("announces state changes once, not every byte", () => {
    render(FishProgressBar, { downloaded: GiB, total: 2 * GiB, bytesPerSec: 0, state: "paused" });
    expect(screen.getByRole("status").textContent).toBe("Download paused.");
  });

  it("shows the real downloaded bytes when the total is unknown", () => {
    render(FishProgressBar, { downloaded: 500 * MiB, total: 0, bytesPerSec: 0, state: "paused" });
    expect(screen.getByText("Paused at 0%. 500.0 MB of — downloaded.")).toBeTruthy();
  });
});
