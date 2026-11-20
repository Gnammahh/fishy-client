<script lang="ts">
  import Sidebar, { type Page } from "./Sidebar.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { signOut, retry, type AuthState } from "$lib/auth/auth";

  // `state` is the prop name; renamed locally so it can't clash with the $state rune.
  let { state: auth }: { state: Exclude<AuthState, { kind: "SignedOut" }> } = $props();

  let page = $state<Page>("library");
  let busy = $state(false);

  async function logOut() {
    busy = true;
    try {
      await signOut();
    } finally {
      busy = false;
    }
  }
</script>

<div class="shell">
  <Sidebar {page} onselect={(p) => (page = p)} />
  <main>
    {#if auth.kind === "Offline"}
      <div class="banner" role="status">
        <span>Couldn't reach Epic. Check your internet connection.</span>
        <PixelButton variant="ghost" onclick={retry}>Try again</PixelButton>
      </div>
    {/if}

    {#if page === "library"}
      <h2>Library</h2>
      <p>Signed in as <strong>{auth.display_name}</strong>. Your games show up here in the next update.</p>
    {:else if page === "downloads"}
      <h2>Downloads</h2>
      <p>No downloads yet.</p>
    {:else if page === "settings"}
      <h2>Settings</h2>
      <p>Settings arrive together with installs and downloads.</p>
    {:else}
      <h2>Account</h2>
      <dl>
        <dt>Epic display name</dt>
        <dd>{auth.display_name}</dd>
        {#if auth.kind === "SignedIn"}
          <dt>Account ID</dt>
          <dd class="mono">{auth.account_id}</dd>
        {/if}
      </dl>
      <p class="note">Logging out ends this session at Epic and removes it from this PC.</p>
      <PixelButton variant="ghost" disabled={busy} onclick={logOut}>Log out</PixelButton>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    min-height: 100vh;
  }
  main {
    padding: 32px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    min-width: 0;
  }
  h2 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 28px;
    margin: 0;
  }
  p {
    margin: 0;
    max-width: 62ch;
    color: var(--text);
  }
  strong {
    color: var(--aqua);
    font-weight: 600;
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
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 18px;
    margin: 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
  }
  .mono {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 13px;
  }
  .note {
    color: var(--text-2);
    font-size: 14px;
  }
</style>
