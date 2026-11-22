<script lang="ts">
  import { onMount } from "svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { epicNote, scanOnce, addAll, repair } from "./epicImport";

  onMount(() => {
    scanOnce();
  });

  const count = (n: number) => (n === 1 ? "1 game" : `${n} games`);
  const shown = $derived(!$epicNote.hidden && ($epicNote.waiting.length > 0 || $epicNote.lines.length > 0 || $epicNote.checking !== null));
</script>

{#if shown}
  <div class="note" role="status">
    {#if $epicNote.waiting.length > 0 && !$epicNote.checking}
      <div class="row">
        <span>{count($epicNote.waiting.length)} installed by Epic's launcher can be added.</span>
        <PixelButton onclick={addAll}>Add them</PixelButton>
      </div>
    {/if}
    {#if $epicNote.checking}<p class="row">Checking {$epicNote.checking}…</p>{/if}
    {#each $epicNote.lines as line (line.game.app_name)}
      <div class="row">
        <span>{line.text}</span>
        {#if line.repair}
          <PixelButton variant="ghost" disabled={line.busy} onclick={() => repair(line.game.app_name)}>Download them</PixelButton>
        {/if}
      </div>
      {#if line.error}<p class="error" role="alert">{line.error}</p>{/if}
    {/each}
    {#if !$epicNote.checking}
      <button class="hide" aria-label="Hide" onclick={() => epicNote.update((n) => ({ ...n, hidden: true }))}>×</button>
    {/if}
  </div>
{/if}

<style>
  .note {
    position: relative;
    display: grid;
    gap: 8px;
    padding: 10px 40px 10px 14px;
    border: 2px solid var(--aqua);
    background: rgba(11, 15, 20, 0.9);
    color: var(--text);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    margin: 0;
  }
  .error {
    margin: 0;
    color: var(--error);
  }
  .hide {
    position: absolute;
    top: 6px;
    right: 8px;
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 18px;
    cursor: pointer;
  }
</style>
