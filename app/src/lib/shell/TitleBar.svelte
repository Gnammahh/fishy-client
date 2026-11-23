<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import icon from "../../../src-tauri/icons/32x32.png";

  /** Fishy Client's own title bar, in place of Windows' white one (spec §6s). */
  const win = getCurrentWindow();
  let maximized = $state(false);

  async function readMaximized() {
    try {
      maximized = await win.isMaximized();
    } catch {
      // Not in the app (tests, browser preview): keep "Maximize".
    }
  }

  onMount(() => {
    readMaximized();
    let stop: (() => void) | null = null;
    let gone = false;
    win
      .onResized(() => readMaximized())
      .then((un) => (gone ? un() : (stop = un)))
      .catch(() => {});
    return () => {
      gone = true;
      stop?.();
    };
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={icon} width="16" height="16" alt="" data-tauri-drag-region />
    <span data-tauri-drag-region>Fishy Client</span>
  </div>
  <div class="buttons">
    <button aria-label="Minimize" title="Minimize" onclick={() => win.minimize()}>
      <svg viewBox="0 0 10 10" width="10" height="10" shape-rendering="crispEdges" aria-hidden="true"><rect x="0" y="8" width="10" height="2" /></svg>
    </button>
    <button aria-label={maximized ? "Restore" : "Maximize"} title={maximized ? "Restore" : "Maximize"} onclick={() => win.toggleMaximize()}>
      {#if maximized}
        <svg viewBox="0 0 10 10" width="10" height="10" shape-rendering="crispEdges" aria-hidden="true">
          <rect x="3" y="0" width="7" height="2" /><rect x="8" y="0" width="2" height="7" />
          <path d="M0 3h7v7H0z M2 5v3h3V5z" fill-rule="evenodd" />
        </svg>
      {:else}
        <svg viewBox="0 0 10 10" width="10" height="10" shape-rendering="crispEdges" aria-hidden="true">
          <path d="M0 0h10v10H0z M2 2v6h6V2z" fill-rule="evenodd" />
        </svg>
      {/if}
    </button>
    <button class="close" aria-label="Close" title="Close" onclick={() => win.close()}>
      <svg viewBox="0 0 10 10" width="10" height="10" shape-rendering="crispEdges" aria-hidden="true">
        <path d="M0 0h2v2H0zM2 2h2v2H2zM4 4h2v2H4zM6 6h2v2H6zM8 8h2v2H8zM8 0h2v2H8zM6 2h2v2H6zM2 6h2v2H2zM0 8h2v2H0z" />
      </svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 50;
    height: var(--titlebar);
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--bg); /* solid: nothing shows through (user choice, 1.9.1) */
    border-bottom: 2px solid rgba(79, 227, 209, 0.18);
    user-select: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
    height: 100%;
    font-family: var(--font-pixel);
    font-size: 13px;
    color: var(--aqua);
  }
  .brand img {
    image-rendering: pixelated;
  }
  .buttons {
    display: flex;
    height: 100%;
  }
  .buttons button {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    background: transparent;
    border: 0;
    color: var(--aqua);
    cursor: pointer;
  }
  .buttons svg {
    fill: currentColor;
  }
  .buttons button:hover {
    background: rgba(79, 227, 209, 0.15);
  }
  .buttons .close:hover {
    background: #c42b1c;
    color: #fff;
  }
  .buttons button:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: -2px;
  }
</style>
