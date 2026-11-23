<script lang="ts">
  import PixelButton from "./PixelButton.svelte";

  let {
    message,
    confirmLabel,
    onconfirm,
    oncancel,
  }: { message: string; confirmLabel: string; onconfirm: () => void; oncancel: () => void } = $props();

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
    <div class="actions">
      <PixelButton variant="ghost" onclick={oncancel}>Keep it</PixelButton>
      <PixelButton onclick={onconfirm}>{confirmLabel}</PixelButton>
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
    border: 2px solid var(--orange);
    padding: 20px 22px;
    display: grid;
    gap: 18px;
    max-width: min(440px, calc(100vw - 32px));
    max-height: calc(100vh - var(--titlebar) - 32px);
    box-sizing: border-box; /* padding and border count inside the cap */
    overflow-y: auto;
  }
  p {
    margin: 0;
    color: var(--text);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
</style>
