<script lang="ts">
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import icon from "../../../src-tauri/icons/128x128.png";
  import { openLogsFolder } from "$lib/settings/data";

  // Reloading the window restarts only the screen; downloads keep going in the Rust part.
  let { onreload = () => location.reload() }: { onreload?: () => void } = $props();

  let error = $state("");

  async function openLogs() {
    error = (await openLogsFolder()) ?? "";
  }
</script>

<section class="crash">
  <img class="logo" src={icon} width="96" height="96" alt="" />
  <h1>Something went wrong</h1>
  <p>Fishy Client hit a problem it didn't expect. Your games and downloads are safe.</p>
  <div class="buttons">
    <PixelButton onclick={() => onreload()}>Reload</PixelButton>
    <PixelButton variant="ghost" onclick={openLogs}>Open logs folder</PixelButton>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .crash {
    position: relative;
    z-index: 1;
    min-height: calc(100vh - var(--titlebar));
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 48px 24px;
    text-align: center;
    background: var(--bg, #000);
  }
  .logo {
    image-rendering: pixelated;
  }
  h1 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 36px;
    line-height: 1.1;
    color: var(--aqua);
    margin: 0;
  }
  p {
    margin: 0;
    max-width: 52ch;
    color: var(--text);
  }
  .buttons {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .error {
    color: var(--error);
  }
</style>
