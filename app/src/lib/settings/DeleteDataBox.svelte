<script lang="ts">
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { formatBytes } from "$lib/progress/progress";
  import type { InstalledGame } from "$lib/install/installs";
  import { deleteAllData } from "./data";

  let { games, onclose }: { games: InstalledGame[]; onclose: () => void } = $props();

  let alsoGames = $state(false);
  let deleting = $state(false);
  let error = $state("");

  const size = $derived(formatBytes(games.reduce((sum, g) => sum + g.install_size, 0)));
  const count = $derived(games.length === 1 ? "1 game" : `${games.length} games`);

  async function remove() {
    if (deleting) return;
    deleting = true;
    error = "";
    const failed = await deleteAllData(alsoGames && games.length > 0);
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
    {#if games.length > 0}
      <label class="tick">
        <input type="checkbox" bind:checked={alsoGames} disabled={deleting} />
        <span>Also delete my installed games ({count}, {size})</span>
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
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    z-index: 20;
  }
  .box {
    background: #0b0f14;
    border: 2px solid var(--error);
    padding: 20px 22px;
    display: grid;
    gap: 14px;
    max-width: min(480px, calc(100vw - 32px));
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
