<script lang="ts">
  import Sidebar, { type Page } from "./Sidebar.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import LibraryPage from "$lib/library/LibraryPage.svelte";
  import DownloadsPage from "$lib/install/DownloadsPage.svelte";
  import StorePage from "$lib/store/StorePage.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { refreshLibrary } from "$lib/library/library";
  import SettingsPage from "$lib/settings/SettingsPage.svelte";
  import { loadSettings, saveSettings } from "$lib/settings/settings";
  import WhatsNewBox from "$lib/notes/WhatsNewBox.svelte";
  import UpdateBar from "$lib/appupdate/UpdateBar.svelte";
  import { checkForUpdate } from "$lib/appupdate/appUpdate";
  import { APP_VERSION, shouldAutoOpen } from "$lib/notes/releaseNotes";
  import { initInstalls, playRequest } from "$lib/install/installs";
  import { onMount } from "svelte";
  import { retry, type AuthState } from "$lib/auth/auth";

  // `state` is the prop name; renamed locally so it can't clash with the $state rune.
  let { state: auth }: { state: Exclude<AuthState, { kind: "SignedOut" }> } = $props();

  let page = $state<Page>("library");
  let notesOpen = $state(false);
  // A desktop shortcut's game is played from the Library (spec §6t).
  $effect(() => {
    if ($playRequest) page = "library";
  });

  // Downloads and the installed list, kept up to date by the backend's events.
  onMount(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    initInstalls().then((s) => (gone ? s() : (stop = s)));
    checkForUpdate(); // once per start (spec §6j); the Rust side skips it when switched off
    // A game bought or claimed in Epic's store shows up once the Store window closes (spec §6o).
    let unlisten: (() => void) | undefined;
    listen("store-closed", () => refreshLibrary())
      .then((u) => (gone ? u() : (unlisten = u)))
      .catch(() => {});
    // The notes open by themselves once per new version, then only from the sidebar.
    loadSettings().then((s) => {
      if (!gone && shouldAutoOpen(s.notes_seen)) {
        notesOpen = true;
        saveSettings({ notes_seen: APP_VERSION });
      }
    });
    return () => {
      gone = true;
      stop?.();
      unlisten?.();
    };
  });
  let checking = $state(false);

  async function tryAgain() {
    checking = true;
    try {
      await retry();
    } finally {
      checking = false;
    }
  }
</script>

<div class="shell">
  <Sidebar {page} onselect={(p) => (page = p)} onnotes={() => (notesOpen = true)} />
  <main>
    <UpdateBar />
    {#if auth.kind === "Offline"}
      <div class="banner" role="status">
        <span>Couldn't reach Epic. Check your internet connection.</span>
        <PixelButton variant="ghost" disabled={checking} onclick={tryAgain}>Try again</PixelButton>
      </div>
    {/if}

    {#if page === "library"}
      <LibraryPage name={auth.display_name} offline={auth.kind === "Offline"} />
    {:else if page === "store"}
      <StorePage />
    {:else if page === "downloads"}
      <DownloadsPage onsettings={() => (page = "settings")} />
    {:else}
      <SettingsPage account={{ display_name: auth.display_name, account_id: auth.kind === "SignedIn" ? auth.account_id : undefined }} />
    {/if}
  </main>
</div>
{#if notesOpen}<WhatsNewBox onclose={() => (notesOpen = false)} />{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    min-height: calc(100vh - var(--titlebar));
  }
  main {
    padding: 32px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    min-width: 0;
  }
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border: 2px solid var(--orange);
    background: rgba(11, 15, 20, 0.9);
    color: var(--orange);
    align-self: stretch;
  }
</style>
