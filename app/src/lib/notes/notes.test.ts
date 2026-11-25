import { describe, it, expect, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import pkg from "../../../package.json";
import { RELEASE_NOTES, APP_VERSION, shouldAutoOpen } from "./releaseNotes";
import WhatsNewBox from "./WhatsNewBox.svelte";
import Sidebar from "$lib/shell/Sidebar.svelte";
import AppShell from "$lib/shell/AppShell.svelte";
import { resetSettings } from "$lib/settings/settings";

afterEach(() => {
  cleanup();
  clearMocks();
  resetSettings();
});

describe("release notes", () => {
  it("the_newest_notes_are_for_this_version", () => {
    // A release without its notes fails here.
    expect(APP_VERSION).toBe(pkg.version);
    expect(RELEASE_NOTES[0].version).toBe(pkg.version);
  });

  it("every_version_has_a_date_and_lines", () => {
    expect(RELEASE_NOTES.map((n) => n.version)).toEqual(["2.0.0", "1.9.5", "1.9.4", "1.9.3", "1.9.2", "1.9.1", "1.9.0", "1.8.0", "1.7.1", "1.7.0", "1.6.0", "1.5.1", "1.5.0", "1.4.1", "1.4.0", "1.3.0", "1.2.0", "1.1.1", "1.1.0", "1.0.0"]);
    for (const n of RELEASE_NOTES) {
      expect(n.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
      expect(n.items.length).toBeGreaterThan(0);
    }
  });

  it("opens_by_itself_only_when_not_seen", () => {
    expect(shouldAutoOpen(null)).toBe(true);
    expect(shouldAutoOpen("1.0.0")).toBe(true);
    expect(shouldAutoOpen(APP_VERSION)).toBe(false);
  });
});

describe("WhatsNewBox", () => {
  it("lists_every_version_newest_first_and_closes", async () => {
    let closed = 0;
    render(WhatsNewBox, { onclose: () => closed++ });
    const dialog = screen.getByRole("dialog", { name: "What's new" });
    const headings = Array.from(dialog.querySelectorAll("h4")).map((h) => h.textContent);
    expect(headings[0]).toContain("2.0.0");
    expect(headings[1]).toContain("1.9.5");
    expect(dialog.textContent).toContain(RELEASE_NOTES[0].items[0]);
    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(closed).toBe(2);
  });
});

describe("Sidebar What's new button", () => {
  it("shows_the_version_and_opens_the_notes", async () => {
    let opened = 0;
    render(Sidebar, { page: "library", onselect: () => {}, onnotes: () => opened++ });
    await fireEvent.click(screen.getByRole("button", { name: `What's new · v${APP_VERSION}` }));
    expect(opened).toBe(1);
  });
});

describe("opening once per version", () => {
  const auth = { kind: "SignedIn" as const, display_name: "FishFan", account_id: "acc" };

  function mock(seen: string | null) {
    const saved: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "settings_get") return { notes_seen: seen };
      if (cmd === "settings_set") {
        saved.push((args as { value: unknown }).value);
        return (args as { value: unknown }).value;
      }
      if (cmd === "plugin:event|listen") return 1;
      return null;
    });
    return saved;
  }

  it("a_new_version_opens_the_notes_once_and_remembers", async () => {
    const saved = mock("1.0.0");
    render(AppShell, { state: auth });
    expect(await screen.findByRole("dialog", { name: "What's new" })).toBeTruthy();
    await waitFor(() => expect(saved).toContainEqual(expect.objectContaining({ notes_seen: APP_VERSION })));
  });

  it("an_already_seen_version_doesnt_open_them", async () => {
    const saved = mock(APP_VERSION);
    render(AppShell, { state: auth });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByRole("dialog", { name: "What's new" })).toBeNull();
    expect(saved).toEqual([]);
  });
});
