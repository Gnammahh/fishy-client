<script lang="ts">
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { appUpdate, installUpdate, later } from "./appUpdate";

  let showNotes = $state(false);
</script>

{#if $appUpdate.offer && !$appUpdate.hidden}
  <div class="bar" role="status">
    <div class="row">
      <span>Fishy Client {$appUpdate.offer.version} is available.</span>
      {#if $appUpdate.installing}
        <span class="progress">Downloading the update… {$appUpdate.percent}%</span>
      {:else}
        <PixelButton variant="ghost" onclick={() => (showNotes = !showNotes)}>What's new</PixelButton>
        <PixelButton onclick={installUpdate}>Update now</PixelButton>
        <PixelButton variant="ghost" onclick={later}>Later</PixelButton>
      {/if}
    </div>
    {#if showNotes}<p class="notes">{$appUpdate.offer.notes}</p>{/if}
    {#if $appUpdate.error}<p class="error" role="alert">{$appUpdate.error}</p>{/if}
  </div>
{/if}

<style>
  .bar {
    display: grid;
    gap: 8px;
    padding: 10px 14px;
    margin-bottom: 14px;
    border: 2px solid var(--aqua);
    background: rgba(11, 15, 20, 0.9);
    color: var(--text);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
  }
  .progress {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .notes {
    margin: 0;
    white-space: pre-line;
    font-size: 14px;
    color: var(--text-2);
  }
  .error {
    margin: 0;
    color: var(--error);
  }
</style>
