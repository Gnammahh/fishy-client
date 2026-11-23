<script lang="ts">
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { RELEASE_NOTES } from "./releaseNotes";

  let { onclose }: { onclose: () => void } = $props();

  let box: HTMLDivElement;
  $effect(() => {
    box?.querySelector<HTMLButtonElement>("button")?.focus();
  });
</script>

<div class="overlay">
  <div
    class="box"
    role="dialog"
    aria-modal="true"
    aria-labelledby="notes-title"
    tabindex="-1"
    bind:this={box}
    onkeydown={(e) => e.key === "Escape" && onclose()}
  >
    <h3 id="notes-title">What's new</h3>
    <div class="list">
      {#each RELEASE_NOTES as note (note.version)}
        <section>
          <h4>Version {note.version} <span class="date">{note.date}</span></h4>
          <ul>
            {#each note.items as item, i (i)}<li>{item}</li>{/each}
          </ul>
        </section>
      {/each}
    </div>
    <div class="actions">
      <PixelButton onclick={onclose}>Close</PixelButton>
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
    width: min(560px, calc(100vw - 32px));
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: 12px;
    padding: 20px 22px;
    background: var(--panel);
    border: 2px solid var(--aqua);
    color: var(--text);
    max-height: calc(100vh - var(--titlebar) - 32px);
    overflow-y: auto;
  }
  h3 {
    margin: 0;
    font-family: var(--font-pixel);
    color: var(--aqua);
  }
  .list {
    overflow-y: auto;
    padding-right: 6px;
  }
  section + section {
    margin-top: 14px;
  }
  h4 {
    margin: 0 0 6px;
    font-family: var(--font-pixel);
    font-weight: normal;
  }
  .date {
    color: var(--text-2);
    font-size: 12px;
    margin-left: 6px;
  }
  ul {
    margin: 0;
    padding-left: 18px;
    display: grid;
    gap: 4px;
    font-size: 14px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
