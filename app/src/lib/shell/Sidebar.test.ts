import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";

afterEach(() => cleanup());

describe("Sidebar", () => {
  it("has_drifting_gray_fish_hidden_from_screen_readers", () => {
    const { container } = render(Sidebar, { page: "library", onselect: () => {} });
    const school = container.querySelector(".school");
    expect(school).toBeTruthy();
    expect(school!.getAttribute("aria-hidden")).toBe("true");
    expect(school!.querySelectorAll(".drifter").length).toBeGreaterThanOrEqual(6);
    // gray, not the aqua accent
    expect(school!.innerHTML).toContain("#7A8A99");
    expect(school!.innerHTML).not.toContain("#4FE3D1");
  });
});
