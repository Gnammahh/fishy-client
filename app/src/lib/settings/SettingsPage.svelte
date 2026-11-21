<script lang="ts">
  import { settings, saveSettings, type Settings } from "./settings";
  import { openLogsFolder } from "./data";
  import { signOut } from "$lib/auth/auth";
  import { installed } from "$lib/install/installs";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import DeleteDataBox from "./DeleteDataBox.svelte";

  let { account }: { account?: { display_name: string; account_id?: string } } = $props();

  let error = $state("");
  let accountError = $state("");
  let busy = $state(false);
  let deleting = $state(false);

  async function logOut() {
    busy = true;
    accountError = "";
    try {
      if (!(await signOut())) accountError = "Couldn't log out. Please try again.";
    } finally {
      busy = false;
    }
  }

  async function openLogs() {
    accountError = (await openLogsFolder()) ?? "";
  }

  async function toggle(key: keyof Settings, e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    const wanted = box.checked;
    error = (await saveSettings({ [key]: wanted })) ?? "";
    if (error) box.checked = !wanted; // show what's really saved
  }
</script>

<section class="settings">
  <h2>Settings</h2>

  <h3>Updates</h3>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="auto-name"
      aria-describedby="auto-hint"
      checked={$settings.auto_update}
      onchange={(e) => toggle("auto_update", e)}
    />
    <span>
      <span class="name" id="auto-name">Update games automatically</span>
      <span class="hint" id="auto-hint">When your Library finds a new version of an installed game, it starts downloading by itself.</span>
    </span>
  </label>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="warn-name"
      aria-describedby="warn-hint"
      checked={$settings.warn_outdated_play}
      onchange={(e) => toggle("warn_outdated_play", e)}
    />
    <span>
      <span class="name" id="warn-name">Warn before playing a game with an update waiting</span>
      <span class="hint" id="warn-hint">Some online games won't start until they're updated.</span>
    </span>
  </label>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <h3>Account</h3>
  {#if account}
    <dl>
      <dt>Epic display name</dt>
      <dd>{account.display_name}</dd>
      {#if account.account_id}
        <dt>Account ID</dt>
        <dd class="mono">{account.account_id}</dd>
      {/if}
    </dl>
  {/if}
  <p class="hint">Logging out ends this session at Epic and removes it from this PC.</p>
  <div class="buttons">
    <PixelButton variant="ghost" disabled={busy} onclick={logOut}>Log out</PixelButton>
    <PixelButton variant="ghost" onclick={openLogs}>Open logs folder</PixelButton>
    <PixelButton variant="danger" onclick={() => (deleting = true)}>Delete all my data…</PixelButton>
  </div>
  {#if accountError}<p class="error" role="alert">{accountError}</p>{/if}
</section>

{#if deleting}
  <DeleteDataBox games={$installed} onclose={() => (deleting = false)} />
{/if}

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 640px;
  }
  h2 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 28px;
    margin: 0;
  }
  h3 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 16px;
    margin: 8px 0 0;
    color: var(--aqua);
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    cursor: pointer;
  }
  .row > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-weight: 600;
  }
  .hint {
    font-size: 13px;
    opacity: 0.75;
  }
  input {
    width: 18px;
    height: 18px;
    margin-top: 2px;
    accent-color: var(--orange);
    flex-shrink: 0;
  }
  .error {
    color: var(--error);
  }
  p {
    margin: 0;
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
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
</style>
