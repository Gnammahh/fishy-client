<script lang="ts">
  import { onMount } from "svelte";
  import { settings, saveSettings, type Settings } from "./settings";
  import { openLogsFolder, parseSpeed, defaultInstallFolder, getAutostart, setAutostart } from "./data";
  import { signOut } from "$lib/auth/auth";
  import { downloads, installed, pickFolder } from "$lib/install/installs";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import DeleteDataBox from "./DeleteDataBox.svelte";

  let { account }: { account?: { display_name: string; account_id?: string } } = $props();

  const SPEED_ERROR = "Pick a number from 1 to 1000.";
  const PICKER_ERROR = "Couldn't open the folder picker.";
  /** What the speed box starts at when the limit is turned on. */
  const DEFAULT_SPEED = 10;

  let downloadsError = $state("");
  let appearanceError = $state("");
  let accountError = $state("");
  let busy = $state(false);
  let deleting = $state(false);
  let defaultFolder = $state("");
  let autostart = $state(false);

  onMount(async () => {
    defaultFolder = await defaultInstallFolder();
    autostart = await getAutostart();
  });

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

  /** A switch saved in settings.json; on failure it goes back to what's really saved. */
  async function toggle(key: "auto_update" | "warn_outdated_play" | "reduce_motion", e: Event, where: "downloads" | "appearance") {
    const box = e.currentTarget as HTMLInputElement;
    const wanted = box.checked;
    const err = (await saveSettings({ [key]: wanted })) ?? "";
    if (where === "downloads") downloadsError = err;
    else appearanceError = err;
    if (err) box.checked = !wanted;
  }

  async function save(patch: Partial<Settings>) {
    downloadsError = (await saveSettings(patch)) ?? "";
  }

  async function changeFolder() {
    let picked: string | null;
    try {
      picked = await pickFolder();
    } catch {
      downloadsError = PICKER_ERROR;
      return;
    }
    if (picked) await save({ install_folder: picked });
  }

  async function toggleSpeed(e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    const wanted = box.checked;
    await save({ speed_limit_mbps: wanted ? DEFAULT_SPEED : null });
    if (downloadsError) box.checked = !wanted;
  }

  async function changeSpeed(e: Event) {
    const n = parseSpeed((e.currentTarget as HTMLInputElement).value);
    if (n === null) {
      downloadsError = SPEED_ERROR;
      return;
    }
    await save({ speed_limit_mbps: n });
  }

  async function toggleAutostart(e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    const result = await setAutostart(box.checked);
    if (typeof result === "string") {
      appearanceError = result;
      box.checked = autostart; // what Windows really has
      return;
    }
    appearanceError = "";
    autostart = result;
    box.checked = result;
  }
</script>

<section class="settings">
  <h2>Settings</h2>

  <h3>Downloads</h3>
  <div class="row static">
    <span>
      <span class="name" id="folder-name">Default install folder</span>
      <span class="path" aria-labelledby="folder-name">{$settings.install_folder ?? defaultFolder}</span>
      <span class="hint">New games go here. You can still pick another folder in the Install box.</span>
      <span class="buttons">
        <PixelButton variant="ghost" onclick={changeFolder}>Change…</PixelButton>
        {#if $settings.install_folder}
          <PixelButton variant="ghost" onclick={() => save({ install_folder: null })}>Use default</PixelButton>
        {/if}
      </span>
    </span>
  </div>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="speed-name"
      aria-describedby="speed-hint"
      checked={$settings.speed_limit_mbps !== null}
      onchange={toggleSpeed}
    />
    <span>
      <span class="name" id="speed-name">Limit download speed</span>
      <span class="hint" id="speed-hint">Leaves room for other things on your internet, like video calls.</span>
    </span>
  </label>
  {#if $settings.speed_limit_mbps !== null}
    <div class="speed">
      <input type="text" inputmode="numeric" aria-label="MB/s" value={String($settings.speed_limit_mbps)} onchange={changeSpeed} />
      <span aria-hidden="true">MB/s</span>
    </div>
  {/if}
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="auto-name"
      aria-describedby="auto-hint"
      checked={$settings.auto_update}
      onchange={(e) => toggle("auto_update", e, "downloads")}
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
      onchange={(e) => toggle("warn_outdated_play", e, "downloads")}
    />
    <span>
      <span class="name" id="warn-name">Warn before playing a game with an update waiting</span>
      <span class="hint" id="warn-hint">Some online games won't start until they're updated.</span>
    </span>
  </label>
  {#if downloadsError}<p class="error" role="alert">{downloadsError}</p>{/if}

  <h3>Appearance and startup</h3>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="motion-name"
      aria-describedby="motion-hint"
      checked={$settings.reduce_motion}
      onchange={(e) => toggle("reduce_motion", e, "appearance")}
    />
    <span>
      <span class="name" id="motion-name">Reduce motion</span>
      <span class="hint" id="motion-hint">Calms the fish animations. When it's off, Fishy Client follows Windows' "Animation effects" setting.</span>
    </span>
  </label>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="startup-name"
      aria-describedby="startup-hint"
      checked={autostart}
      onchange={toggleAutostart}
    />
    <span>
      <span class="name" id="startup-name">Start with Windows</span>
      <span class="hint" id="startup-hint">Opens Fishy Client when you sign in to Windows.</span>
    </span>
  </label>
  {#if appearanceError}<p class="error" role="alert">{appearanceError}</p>{/if}

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
  <DeleteDataBox games={$installed} downloads={$downloads.filter((d) => !d.update)} onclose={() => (deleting = false)} />
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
  .static {
    cursor: default;
  }
  .row .buttons {
    flex-direction: row;
    margin-top: 6px;
  }
  .path {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 13px;
    word-break: break-all;
  }
  .speed {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: 30px;
  }
  .speed input {
    width: 72px;
    height: auto;
    margin: 0;
    padding: 6px 8px;
    font: inherit;
    color: var(--text);
    background: rgba(11, 15, 20, 0.9);
    border: 2px solid var(--aqua);
  }
</style>
