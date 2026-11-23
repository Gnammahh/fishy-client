<script lang="ts">
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { formatBytes } from "$lib/progress/progress";
  import type { DownloadItem, InstalledGame } from "$lib/install/installs";
  import { deleteAllData } from "./data";

  // `downloads`: unfinished installs (not updates; an update's files go with its installed game).
  let {
    games,
    downloads = [],
    onclose,
  }: { games: InstalledGame[]; downloads?: DownloadItem[]; onclose: () => void } = $props();

  let alsoGames = $state(false);
  let deleting = $state(false);
  let error = $state("");

  const plural = (n: number, word: string) => (n === 1 ? `1 ${word}` : `${n} ${word}s`);
  const anything = $derived(games.length > 0 || downloads.length > 0);
  const tickLabel = $derived.by(() => {
    const gamesPart = `${plural(games.length, "game")}, ${formatBytes(games.reduce((sum, g) => sum + g.install_size, 0))}`;
    const downloadsPart = plural(downloads.length, "download");
    if (downloads.length === 0) return `Also delete my installed games (${gamesPart})`;
    if (games.length === 0) return `Also delete my unfinished downloads (${downloadsPart})`;
    return `Also delete my installed games and unfinished downloads (${gamesPart}; ${downloadsPart})`;
  });

  async function remove() {
    if (deleting) return;
    deleting = true;
    error = "";
    const failed = await deleteAllData(alsoGames && anything);
    if (failed) {
      error = failed;
      deleting = false;
    }
    // On success the app switches to the Login screen, which removes this box.
  }

  let box: HTMLDivElement;
  $effect(() => {
    box?.querySelector<HTMLButtonElement>("button.ghost")?.focus();
  });
</script>

<div class="overlay">
  <div
    class="box"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="delete-title"
    tabindex="-1"
    bind:this={box}
    onkeydown={(e) => e.key === "Escape" && !deleting && onclose()}
  >
    <h3 id="delete-title">Delete all my data?</h3>
    <p>This logs you out and removes everything Fishy Client saved on this PC: settings, your game list, download progress and logs.</p>
    {#if anything}
      <label class="tick">
        <input type="checkbox" bind:checked={alsoGames} disabled={deleting} />
        <span>{tickLabel}</span>
      </label>
      {#if !alsoGames}<p class="note">Your game folders stay on disk, but Fishy Client will forget them.</p>{/if}
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <PixelButton variant="ghost" disabled={deleting} onclick={onclose}>Cancel</PixelButton>
      <PixelButton variant="danger" disabled={deleting} onclick={remove}>Delete</PixelButton>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: var(--titlebar) 0 0 0; /* under Fishy's title bar (spec §6s) */
    background: rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    z-index: 20;
    padding: 16px;
  }
  .box {
    background: #0b0f14;
    border: 2px solid var(--error);
    padding: 20px 22px;
    display: grid;
    gap: 14px;
    max-width: min(480px, calc(100vw - 32px));
    max-height: calc(100vh - var(--titlebar) - 32px);
    box-sizing: border-box; /* padding and border count inside the cap */
    overflow-y: auto;
  }
  h3 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 18px;
    margin: 0;
  }
  p {
    margin: 0;
    color: var(--text);
  }
  .note {
    color: var(--text-2);
    font-size: 14px;
  }
  .tick {
    display: flex;
    gap: 10px;
    align-items: center;
    cursor: pointer;
  }
  .tick input {
    width: 18px;
    height: 18px;
    accent-color: var(--error);
  }
  .error {
    color: var(--error);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 4px;
  }
</style>
