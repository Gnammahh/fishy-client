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

  it("has_no_account_page_any_more", () => {
    const { getByRole, queryByRole } = render(Sidebar, { page: "library", onselect: () => {} });
    expect(getByRole("button", { name: "Settings" })).toBeTruthy();
    expect(queryByRole("button", { name: "Account" })).toBeNull();
  });

  it("store_is_between_library_and_downloads", () => {
    const { getAllByRole } = render(Sidebar, { page: "library", onselect: () => {} });
    const names = getAllByRole("button").map((b) => b.textContent?.trim());
    expect(names.slice(0, 3)).toEqual(["Library", "Store", "Downloads"]);
  });
});
