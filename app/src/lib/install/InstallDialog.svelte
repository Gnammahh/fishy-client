<script lang="ts">
  import { onMount } from "svelte";
  import type { Game } from "$lib/library/library";
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { formatBytes } from "$lib/progress/progress";
  import { epicFolder, forgetInstall, getInstallOptions, pickFolder, startInstall, type InstallOptions } from "./installs";

  let { game, onclose }: { game: Game; onclose: () => void } = $props();
  /** Closing without installing frees the file list Fishy Client fetched for the box. */
  function cancel() {
    forgetInstall(game.app_name);
    onclose();
  }

  let parent = $state<string | null>(null);
  let options = $state<InstallOptions | null>(null);
  let loadError = $state("");
  let startError = $state("");
  let busy = $state(false);
  let dialog: HTMLDivElement;

  const message = (e: unknown) => (typeof e === "string" ? e : "Couldn't reach Epic. Check your internet connection.");

  async function load() {
    loadError = "";
    try {
      options = await getInstallOptions(game, parent);
    } catch (e) {
      options = null;
      loadError = message(e);
    }
  }

  let epic = $state<string | null>(null);

  async function useEpic() {
    if (!epic) return;
    parent = epic;
    await load();
  }

  async function changeFolder() {
    const picked = await pickFolder().catch(() => null);
    if (!picked) return;
    parent = picked;
    await load();
  }

  async function install() {
    busy = true;
    startError = "";
    const err = await startInstall(game, parent);
    busy = false;
    if (err) startError = err;
    else onclose();
  }

  onMount(() => {
    epicFolder().then((f) => (epic = f));
    dialog?.focus();
    load();
  });
</script>

<div class="overlay">
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Install {game.title}"
    tabindex="-1"
    bind:this={dialog}
    onkeydown={(e) => e.key === "Escape" && cancel()}
  >
    <h3>Install {game.title}</h3>

    {#if loadError}
      <p class="error" role="alert">{loadError}</p>
      <div class="actions">
        <PixelButton variant="ghost" onclick={cancel}>Cancel</PixelButton>
      </div>
    {:else if !options}
      <div class="loading" role="status">
        <FishLoader size="small" />
        <span>Getting download info…</span>
      </div>
      <div class="actions">
        <PixelButton variant="ghost" onclick={cancel}>Cancel</PixelButton>
      </div>
    {:else if options.has_parts}
      <p class="parts">This game has optional parts. Install it with Epic's launcher; Fishy Client adds it by itself afterwards.</p>
      <div class="actions">
        <PixelButton variant="ghost" onclick={cancel}>Close</PixelButton>
      </div>
    {:else}
      <dl>
        <dt>Folder</dt>
        <dd class="path">{options.install_path}</dd>
        <dt>Download</dt>
        <dd>{formatBytes(options.download_size)}</dd>
        <dt>Space needed</dt>
        <dd>{formatBytes(options.install_size)}</dd>
        <dt>Free space</dt>
        <dd>{formatBytes(options.free_space)}</dd>
      </dl>
      <p class="only-here">Only Fishy Client will know about this game. Epic's launcher can't see it and would download it again.</p>
      {#if options.needs_permission}<p class="permission">Windows will ask for permission once to create this folder.</p>{/if}
      {#if options.note}<p class="note">{options.note}</p>{/if}
      {#if options.error}<p class="error" role="alert">{options.error}</p>{/if}
      {#if startError}<p class="error" role="alert">{startError}</p>{/if}
      <div class="actions">
        <PixelButton variant="ghost" onclick={changeFolder}>Change folder</PixelButton>
        {#if epic && !options.install_path.toLowerCase().startsWith(epic.toLowerCase() + "\\")}
          <PixelButton variant="ghost" onclick={useEpic}>Use Epic's folder</PixelButton>
        {/if}
        <span class="spacer"></span>
        <PixelButton variant="ghost" onclick={cancel}>Cancel</PixelButton>
        <PixelButton disabled={!options.enough_space || options.folder_ok === false || busy} onclick={install}>Install</PixelButton>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: var(--titlebar) 0 0 0; /* under Fishy's title bar (spec §6s) */
    background: rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    z-index: 10;
    padding: 16px;
  }
  .dialog {
    background: #0b0f14;
    border: 2px solid var(--aqua);
    padding: 22px 24px;
    display: grid;
    gap: 16px;
    width: min(520px, calc(100vw - 32px));
    max-height: calc(100vh - var(--titlebar) - 32px);
    overflow-y: auto;
  }
  .dialog:focus {
    outline: none;
  }
  h3 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 20px;
    margin: 0;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 18px;
    margin: 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .path {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--text-2);
  }
  /* Epic's launcher can't be told about games installed here (spec §6h). */
  .permission,
  .parts {
    color: var(--aqua);
    font-size: 14px;
    margin: 0;
  }
  .only-here {
    color: var(--text-2);
    font-size: 13px;
    margin: 0;
  }
  .note {
    color: var(--orange);
    font-size: 14px;
  }
  .error {
    color: var(--error);
    margin: 0;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    justify-content: flex-end;
  }
  .spacer {
    flex: 1;
  }
</style>
