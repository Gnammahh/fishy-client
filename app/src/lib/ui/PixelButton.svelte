<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "primary",
    type = "button",
    disabled = false,
    onclick = undefined,
    children,
  }: {
    variant?: "primary" | "ghost" | "play";
    type?: "button" | "submit";
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button class="pixel-btn {variant}" {type} {disabled} {onclick}>{@render children()}</button>

<style>
  .pixel-btn {
    font-family: var(--font-pixel);
    font-size: 15px;
    font-weight: 500;
    line-height: 1;
    color: #000;
    background: var(--aqua);
    border: 0;
    padding: 10px 16px;
    cursor: pointer;
    clip-path: polygon(
      0 4px, 4px 4px, 4px 0, calc(100% - 4px) 0, calc(100% - 4px) 4px, 100% 4px,
      100% calc(100% - 4px), calc(100% - 4px) calc(100% - 4px), calc(100% - 4px) 100%,
      4px 100%, 4px calc(100% - 4px), 0 calc(100% - 4px)
    );
  }
  .pixel-btn:hover:not(:disabled) {
    filter: brightness(1.12);
  }
  .pixel-btn:focus-visible {
    background: var(--orange);
    color: #000;
    outline: none;
  }
  .ghost {
    background: transparent;
    color: var(--aqua);
    box-shadow: inset 0 0 0 2px var(--aqua);
  }
  .ghost:focus-visible {
    box-shadow: inset 0 0 0 2px var(--orange);
  }
  .play {
    background: var(--success);
  }
  .pixel-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
