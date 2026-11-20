<script lang="ts">
  import { onMount } from "svelte";
  import { reducedMotion } from "$lib/motion";
  import { drawGrid } from "$lib/pixel/canvas";
  import {
    FISH_SWIM,
    FISH_MOUTH_OPEN,
    FISH_MOUTH_CLOSED,
    FISH_DEAD,
    BUBBLE,
    PUFF,
    FISH_PALETTE,
    FISH_GRAY_PALETTE,
    BUBBLE_PALETTE,
    PUFF_PALETTE,
  } from "$lib/pixel/sprites";
  import { clampProgress, percent, statusText, type DownloadState } from "./progress";

  let {
    downloaded,
    total,
    bytesPerSec,
    state: dlState,
    onretry = undefined,
    onplay = undefined,
    autoLoop = false,
  }: {
    downloaded: number;
    total: number;
    bytesPerSec: number;
    state: DownloadState;
    onretry?: () => void;
    onplay?: () => void;
    /** Preview only: replay the "done" jump every few seconds. */
    autoLoop?: boolean;
  } = $props();

  // Geometry (css px), same as the approved mockup.
  const H = 56;
  const TRACK_Y = 34;
  const FISH_TOP = TRACK_Y - 15;
  const S = 2; // fish scale → 32 px
  const DOT_START = 38;
  const DOT_GAP = 12;
  const FRAME_MS = 80;

  let canvas: HTMLCanvasElement;
  let width = $state(320);
  let stateAt = performance.now();
  let lastState: DownloadState | undefined = undefined;
  let jumpDone = $state(false);

  const progress = $derived(total > 0 ? clampProgress(downloaded / total) : 0);
  const text = $derived(statusText(dlState, downloaded, total, bytesPerSec));
  const pct = $derived(percent(downloaded, total));
  // Screen readers hear state changes only, not every byte/speed update.
  const ANNOUNCE: Record<DownloadState, string> = {
    downloading: "",
    paused: "Download paused.",
    verifying: "Checking installed files.",
    error: "Download stopped: the connection to the download server dropped.",
    done: "Download finished.",
  };
  const announcement = $derived(ANNOUNCE[dlState]);
  const showPlay = $derived(dlState === "done" && ($reducedMotion || jumpDone));

  $effect(() => {
    if (dlState !== lastState) {
      lastState = dlState;
      stateAt = performance.now();
      jumpDone = false;
    }
  });

  onMount(() => {
    // No canvas/ResizeObserver (tests, very old webviews): keep the text, skip the drawing.
    if (typeof ResizeObserver === "undefined") return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const ro = new ResizeObserver(() => {
      width = Math.max(96, Math.floor(canvas.clientWidth));
      canvas.width = width;
    });
    ro.observe(canvas);

    let raf = 0;
    let last = 0;
    let eatenBefore = -1;
    let puffAt = -1e9;

    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      if (now - last < FRAME_MS) return;
      last = now;
      const rm = $reducedMotion;
      const W = canvas.width;
      ctx.clearRect(0, 0, W, H);

      const fishLeft = Math.round((progress * (W - 32)) / 2) * 2;
      const mouth = fishLeft + 30;
      const isError = dlState === "error";

      let eaten = 0;
      for (let x = DOT_START; x <= W - 6; x += DOT_GAP) {
        if (x < mouth) {
          eaten++;
          continue;
        }
        ctx.fillStyle = isError ? "rgba(255,92,92,.75)" : "#FF9F43";
        ctx.fillRect(x - 2, TRACK_Y - 2, 4, 4);
      }
      if (eaten > eatenBefore && eatenBefore >= 0 && dlState === "downloading") puffAt = now;
      eatenBefore = eaten;
      if (isError) {
        ctx.fillStyle = "rgba(255,92,92,.5)";
        ctx.fillRect(0, H - 2, W, 2);
      }

      const t = now - stateAt;
      if (dlState === "downloading") {
        const frames = [FISH_SWIM[0], FISH_MOUTH_OPEN, FISH_SWIM[1], FISH_MOUTH_OPEN];
        drawGrid(ctx, rm ? FISH_MOUTH_CLOSED : frames[Math.floor(now / 150) % 4], FISH_PALETTE, fishLeft, FISH_TOP, S);
        if (!rm && now - puffAt < 160) drawGrid(ctx, PUFF, PUFF_PALETTE, mouth, TRACK_Y - 6, S);
      } else if (dlState === "paused") {
        const bob = rm ? 0 : Math.round(Math.sin(now / 500)) * 2;
        drawGrid(ctx, FISH_MOUTH_CLOSED, FISH_PALETTE, fishLeft, FISH_TOP + bob, S);
        if (!rm)
          for (let i = 0; i < 3; i++) {
            const k = (now / 45 + i * 9) % 26;
            drawGrid(ctx, BUBBLE, BUBBLE_PALETTE, mouth + 2 + (i % 2) * 4, TRACK_Y - 6 - Math.round(k / 2) * 2, 1);
          }
      } else if (dlState === "verifying") {
        let x = fishLeft;
        let back = false;
        if (!rm) {
          const per = 4000;
          const ph = t % (per * 2);
          back = ph >= per;
          const f = (back ? per * 2 - ph : ph) / per;
          x = Math.round((f * fishLeft) / 2) * 2;
        }
        drawGrid(ctx, Math.floor(now / 200) % 2 ? FISH_SWIM[0] : FISH_SWIM[1], FISH_PALETTE, x, FISH_TOP, S, { flip: back });
        drawGrid(ctx, BUBBLE, BUBBLE_PALETTE, back ? x - 10 : x + 34, TRACK_Y - 10, S);
      } else if (isError) {
        drawGrid(ctx, FISH_DEAD, FISH_GRAY_PALETTE, fishLeft, FISH_TOP - 2, S, { vflip: true });
      } else {
        if (autoLoop && t > 4500) {
          stateAt = now;
          jumpDone = false;
        }
        if (!rm && t < 360) {
          const f = Math.floor(t / 120);
          drawGrid(ctx, FISH_MOUTH_CLOSED, FISH_PALETTE, fishLeft, FISH_TOP + [-6, -12, -6][f], S, { vflip: f === 1 });
          drawGrid(ctx, PUFF, PUFF_PALETTE, fishLeft + 10, TRACK_Y + 2, S);
        } else {
          drawGrid(ctx, FISH_MOUTH_CLOSED, FISH_PALETTE, fishLeft, FISH_TOP, S);
          if (!jumpDone) jumpDone = true;
        }
      }
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  });
</script>

<div class="fbar" class:is-error={dlState === "error"}>
  <div
    role="progressbar"
    aria-label="Download progress"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={pct}
    aria-valuetext={text}
  >
    <canvas bind:this={canvas} height={H} aria-hidden="true"></canvas>
  </div>
  <span class="sr-only" role="status" aria-live="polite">{announcement}</span>
  <div class="meta">
    <p class="text">{text}</p>
    <div class="actions">
      {#if dlState === "error"}
        <button class="pixel-btn" type="button" onclick={() => onretry?.()}>Retry</button>
      {:else if showPlay}
        <button class="pixel-btn play" type="button" onclick={() => onplay?.()}>Play</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .fbar {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  canvas {
    display: block;
    width: 100%;
    height: 56px;
    image-rendering: pixelated;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    min-height: 38px;
  }
  .text {
    margin: 0;
    color: var(--text-2);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  .is-error .text {
    color: var(--error);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
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
  .pixel-btn:hover {
    filter: brightness(1.12);
  }
  .pixel-btn:focus-visible {
    background: var(--orange);
    outline: none;
  }
  .pixel-btn.play {
    background: var(--success);
  }
</style>
