<script lang="ts">
  import PixelButton from "$lib/ui/PixelButton.svelte";

  /** Asked before playing a game that has an update waiting (spec §6e). */
  let {
    title,
    onplay,
    onupdate,
    oncancel,
  }: { title: string; onplay: (dontShowAgain: boolean) => void; onupdate: (dontShowAgain: boolean) => void; oncancel: () => void } = $props();

  let dontShow = $state(false);
  const message = $derived(`${title} has an update waiting. You'll play the older version, and some online games won't start until they're updated.`);

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
    aria-label={message}
    tabindex="-1"
    bind:this={box}
    onkeydown={(e) => e.key === "Escape" && oncancel()}
  >
    <p>{message}</p>
    <label class="tick">
      <input type="checkbox" bind:checked={dontShow} />
      Don't show this again
    </label>
    <div class="actions">
      <PixelButton variant="ghost" onclick={oncancel}>Cancel</PixelButton>
      <PixelButton variant="ghost" onclick={() => onupdate(dontShow)}>Update now</PixelButton>
      <PixelButton onclick={() => onplay(dontShow)}>Play anyway</PixelButton>
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
    background: var(--panel);
    border: 2px solid var(--aqua);
    padding: 22px;
    max-width: 440px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-height: calc(100vh - var(--titlebar) - 32px);
    box-sizing: border-box; /* padding and border count inside the cap */
    overflow-y: auto;
  }
  p {
    margin: 0;
    line-height: 1.45;
  }
  .tick {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    cursor: pointer;
  }
  .tick input {
    accent-color: var(--orange);
  }
  .actions {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
</style>
