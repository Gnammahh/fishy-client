<script lang="ts">
  import { onMount } from "svelte";
  import { reducedMotion } from "$lib/motion";
  import { drawGrid } from "$lib/pixel/canvas";
  import {
    FISH_SWIM,
    FISH_MOUTH_OPEN,
    FISH_MOUTH_CLOSED,
    PELLET,
    PUFF,
    FISH_PALETTE,
    PELLET_PALETTE,
    PUFF_PALETTE,
  } from "$lib/pixel/sprites";
  import { fishLoaderFrame, CYCLE_MS } from "./loaderFrame";

  let { size = "medium", done = false }: { size?: "small" | "medium" | "large"; done?: boolean } = $props();

  const u = $derived(size === "small" ? 2 : size === "large" ? 8 : 4);
  const W = $derived(48 * u);
  const H = $derived(32 * u);

  let canvas: HTMLCanvasElement;
  let faded = $state(false);

  // Restart the animation when `done` goes back to false.
  let restartToken = $state(0);
  let lastDone = false;
  $effect(() => {
    if (lastDone && !done) {
      faded = false;
      restartToken++;
    }
    lastDone = done;
  });

  onMount(() => {
    const ctx = canvas.getContext("2d")!;
    let raf = 0;
    let last = 0;
    let start = performance.now();
    let cycle = -1;
    let pelletX = 0.7;
    let doneAt = 0;
    let token = restartToken;

    const pickPellet = () => {
      const r = Math.random() * 0.23;
      return Math.random() < 0.5 ? 0.15 + r : 0.62 + r;
    };

    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      if (token !== restartToken) {
        token = restartToken;
        start = now;
        cycle = -1;
        doneAt = 0;
      }
      if (now - last < 70) return;
      last = now;

      const s = u;
      const w = W;
      const h = H;
      ctx.clearRect(0, 0, w, h);
      const cx = w / 2;
      const fy = Math.round((h / 2 - 7 * s) / s) * s;
      const mouthY = fy + 7 * s;

      if (doneAt) {
        const t = now - doneAt;
        if (!$reducedMotion && t < 360) {
          const f = Math.floor(t / 120);
          drawGrid(ctx, FISH_MOUTH_CLOSED, FISH_PALETTE, cx - 8 * s, fy + [-1, -3, -1][f] * s, s, { vflip: f === 1 });
        } else {
          drawGrid(ctx, FISH_MOUTH_CLOSED, FISH_PALETTE, cx - 8 * s, fy, s);
          faded = true;
        }
        return;
      }

      if ($reducedMotion) {
        drawGrid(ctx, FISH_MOUTH_CLOSED, FISH_PALETTE, cx - 8 * s, fy, s);
        for (let i = 0; i < 3; i++) {
          ctx.globalAlpha = (Math.sin(now / 700 + i * 2.1) + 1) / 2;
          drawGrid(ctx, PELLET, PELLET_PALETTE, cx - 7 * s + i * 5 * s, fy - 4 * s, s);
        }
        ctx.globalAlpha = 1;
        if (done) doneAt = now;
        return;
      }

      const elapsed = now - start;
      const k = Math.floor(elapsed / CYCLE_MS);
      if (k !== cycle) {
        cycle = k;
        if (done && k > 0) {
          doneAt = now;
          return;
        }
        pelletX = pickPellet();
      }

      const f = fishLoaderFrame(elapsed, pelletX);
      const pellet = pelletX * w;
      const dir = pelletX >= 0.5 ? 1 : -1;
      const target = pellet - dir * 7 * s; // fish centre when its mouth reaches the pellet
      const along = (f.fishX - 0.5) / (pelletX - 0.5);
      const fishCentre = cx + along * (target - cx);
      const swim = (ms: number) => (Math.floor(now / ms) % 2 ? FISH_SWIM[0] : FISH_SWIM[1]);
      const sprite =
        f.phase === "fall"
          ? swim(220)
          : f.phase === "swim"
            ? swim(120)
            : f.phase === "eat"
              ? f.mouthOpen
                ? FISH_MOUTH_OPEN
                : FISH_MOUTH_CLOSED
              : swim(200);
      const bob = f.phase === "fall" ? Math.round(Math.sin(now / 300)) * s : 0;

      drawGrid(ctx, sprite, FISH_PALETTE, Math.round((fishCentre - 8 * s) / s) * s, fy + bob, s, {
        flip: f.facing === "left",
      });
      if (f.pelletVisible) {
        const py = f.phase === "fall" ? -4 * s + (mouthY - 2 * s + 4 * s) * f.pelletY : mouthY - 2 * s;
        drawGrid(ctx, PELLET, PELLET_PALETTE, Math.round((pellet - 2 * s) / s) * s, Math.round(py / s) * s, s);
      }
      if (f.phase === "eat" && !f.mouthOpen) {
        drawGrid(ctx, PUFF, PUFF_PALETTE, Math.round((pellet - 3 * s) / s) * s, mouthY - 3 * s, s);
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

<div class="tank" style="width:{W}px">
  <canvas bind:this={canvas} width={W} height={H} class:gone={faded} aria-hidden="true"></canvas>
  {#if faded}<p class="loaded">Loaded</p>{/if}
  <span class="sr-only" role="status">{faded ? "Loaded" : "Loading"}</span>
</div>

<style>
  .tank {
    position: relative;
    max-width: 100%;
    background: rgba(11, 15, 20, 0.85);
    backdrop-filter: blur(6px);
    border: 2px solid rgba(79, 227, 209, 0.18);
  }
  canvas {
    display: block;
    max-width: 100%;
    height: auto;
    image-rendering: pixelated;
    transition: opacity 0.3s;
  }
  canvas.gone {
    opacity: 0;
  }
  .loaded {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    font-family: var(--font-pixel);
    color: var(--success);
    font-size: 18px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  @media (prefers-reduced-motion: reduce) {
    canvas {
      transition: none;
    }
  }
</style>
