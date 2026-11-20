<script lang="ts">
  import { onMount } from "svelte";
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import FishProgressBar from "$lib/progress/FishProgressBar.svelte";
  import type { DownloadState } from "$lib/progress/progress";
  import icon128 from "../../../src-tauri/icons/128x128.png";
  import icon32 from "../../../src-tauri/icons/32x32.png";

  const GiB = 1024 ** 3;
  const MiB = 1024 ** 2;
  const TOTAL = 7.4 * GiB;

  // Loaders
  let loadersDone = $state(false);

  // Live fake download: 7.4 GB at ~18.2 MB/s, sped up to finish in 20 s.
  let liveP = $state(0);
  let liveState: DownloadState = $state("downloading");
  let liveSpeed = $state(18.2 * MiB);
  let toast = $state("");

  // A slowly looping "downloading" example and a retryable "error" example.
  let demoP = $state(0.2);
  let errorDemo: DownloadState = $state("error");

  onMount(() => {
    let raf = 0;
    let prev = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(250, now - prev);
      prev = now;
      if (liveState === "downloading") {
        liveP = Math.min(1, liveP + dt / 20000);
        if (liveP >= 1) liveState = "done";
      }
      liveSpeed = MiB * (18.2 + Math.sin(now / 900) * 1.1);
      demoP = 0.15 + ((now / 30000) % 1) * 0.7;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  function restart() {
    liveP = 0;
    liveState = "downloading";
    toast = "";
  }
  function retryDemo() {
    errorDemo = "downloading";
    setTimeout(() => (errorDemo = "error"), 2500);
  }
</script>

<main>
  <header>
    <img src={icon128} width="64" height="64" alt="" />
    <div>
      <h1>Fishy Client</h1>
      <p class="lede">Component preview. Everything on this screen uses fake data.</p>
    </div>
  </header>

  <section>
    <h2>Download</h2>
    <div class="card">
      <h3>Tidepool Odyssey</h3>
      <FishProgressBar
        downloaded={liveP * TOTAL}
        total={TOTAL}
        bytesPerSec={liveSpeed}
        state={liveState}
        onretry={() => (liveState = "downloading")}
        onplay={() => (toast = "Tidepool Odyssey is starting. In the real app, this launches the game.")}
      />
      <div class="row">
        <button
          class="ghost"
          type="button"
          disabled={liveState === "done" || liveState === "error"}
          onclick={() => (liveState = liveState === "paused" ? "downloading" : "paused")}
        >
          {liveState === "paused" ? "Resume" : "Pause"}
        </button>
        <button class="ghost" type="button" disabled={liveState === "done" || liveState === "error"} onclick={() => (liveState = "error")}>
          Simulate an error
        </button>
        <button class="ghost" type="button" onclick={restart}>Restart download</button>
      </div>
      <p class="toast" aria-live="polite">{toast}</p>
    </div>
  </section>

  <section>
    <h2>Download bar states</h2>
    <div class="states">
      <div class="card"><h3>Downloading</h3><FishProgressBar downloaded={demoP * TOTAL} total={TOTAL} bytesPerSec={18.2 * MiB} state="downloading" /></div>
      <div class="card"><h3>Paused</h3><FishProgressBar downloaded={0.42 * TOTAL} total={TOTAL} bytesPerSec={0} state="paused" /></div>
      <div class="card"><h3>Checking files</h3><FishProgressBar downloaded={0.7 * TOTAL} total={TOTAL} bytesPerSec={0} state="verifying" /></div>
      <div class="card"><h3>Error</h3><FishProgressBar downloaded={0.42 * TOTAL} total={TOTAL} bytesPerSec={18.2 * MiB} state={errorDemo} onretry={retryDemo} /></div>
      <div class="card"><h3>Done</h3><FishProgressBar downloaded={TOTAL} total={TOTAL} bytesPerSec={0} state="done" autoLoop /></div>
    </div>
  </section>

  <section>
    <h2>Loading fish</h2>
    <div class="loaders">
      <figure><FishLoader size="small" done={loadersDone} /><figcaption>Small</figcaption></figure>
      <figure><FishLoader size="medium" done={loadersDone} /><figcaption>Medium</figcaption></figure>
      <figure><FishLoader size="large" done={loadersDone} /><figcaption>Large</figcaption></figure>
    </div>
    <button type="button" onclick={() => (loadersDone = !loadersDone)}>{loadersDone ? "Start again" : "Finish loading"}</button>
  </section>

  <section>
    <h2>App icon</h2>
    <div class="icons">
      <figure><img src={icon128} width="128" height="128" alt="Fishy Client icon, 128 px" /><figcaption>128 px</figcaption></figure>
      <figure><img src={icon32} width="32" height="32" alt="Fishy Client icon, 32 px" /><figcaption>32 px</figcaption></figure>
    </div>
  </section>
</main>

<style>
  main {
    max-width: 1100px;
    margin: 0 auto;
    padding: 32px 24px 64px;
    display: flex;
    flex-direction: column;
    gap: 40px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  header img {
    image-rendering: pixelated;
  }
  h1 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 44px;
    line-height: 1;
    color: var(--aqua);
    margin: 0;
  }
  .lede {
    margin: 6px 0 0;
    color: var(--text-2);
  }
  h2 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 24px;
    margin: 0 0 14px;
  }
  h3 {
    font-family: var(--font-pixel);
    font-weight: 500;
    font-size: 17px;
    margin: 0;
    color: var(--aqua);
  }
  .card {
    display: grid;
    gap: 10px;
    padding: 16px;
    border: 2px solid rgba(79, 227, 209, 0.18);
    background: rgba(11, 15, 20, 0.86);
    backdrop-filter: blur(6px);
    min-width: 0;
  }
  .states {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 18px;
  }
  .row,
  .loaders,
  .icons {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: flex-end;
  }
  .loaders {
    gap: 24px;
    margin-bottom: 16px;
  }
  figure {
    margin: 0;
    display: grid;
    gap: 8px;
  }
  figcaption {
    color: var(--text-2);
    font-size: 13px;
  }
  .icons {
    padding: 20px;
    background: #3a4652;
    gap: 24px;
    width: fit-content;
  }
  .icons img {
    image-rendering: pixelated;
  }
  .icons figcaption {
    color: #c9d3dc;
  }
  .toast {
    margin: 0;
    color: var(--success);
    font-size: 14px;
    min-height: 1.5em;
  }
  button {
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
  button.ghost {
    background: transparent;
    color: var(--aqua);
    box-shadow: inset 0 0 0 2px var(--aqua);
  }
  button:focus-visible {
    background: var(--orange);
    color: #000;
    outline: none;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
