<script lang="ts">
  import { steppedWavePath } from "./wave";

  // Values match the approved design/mockup.html (css px).
  const LAYERS = [
    { color: "#0A3A7A", opacity: 0.45, wavelength: 320, amplitude: 18, base: 0.7, speed: 8 },
    { color: "#0E5AA8", opacity: 0.35, wavelength: 240, amplitude: 14, base: 0.8, speed: 12 },
    { color: "#1B7FD1", opacity: 0.25, wavelength: 180, amplitude: 10, base: 0.9, speed: 18 },
  ];
  const STEP = 4;

  let width = $state(1200);
  let height = $state(760);

  const layers = $derived(
    LAYERS.map((l) => {
      // one tile covers the window; the path is two tiles so the drift never shows a gap
      const tile = Math.max(1, Math.ceil(width / l.wavelength)) * l.wavelength;
      const h = Math.max(1, Math.round(height / STEP) * STEP);
      const d = steppedWavePath({
        width: tile * 2,
        height: h,
        baseY: Math.round((l.base * h) / STEP) * STEP,
        amplitude: l.amplitude,
        wavelength: l.wavelength,
        step: STEP,
      });
      return { ...l, tile, h, d, seconds: tile / l.speed, steps: tile / STEP };
    }),
  );
</script>

<svelte:window bind:innerWidth={width} bind:innerHeight={height} />

<div class="ocean" aria-hidden="true">
  {#each layers as l (l.color)}
    <svg
      class="layer"
      width={l.tile * 2}
      height={l.h}
      viewBox="0 0 {l.tile * 2} {l.h}"
      shape-rendering="crispEdges"
      style="--tile:{l.tile}px; animation: fishy-ocean-drift {l.seconds}s steps({l.steps}) infinite;"
    >
      <path d={l.d} fill={l.color} fill-opacity={l.opacity} />
    </svg>
  {/each}
</div>

<style>
  .ocean {
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    pointer-events: none;
    background: linear-gradient(to bottom, #000000 0%, #000000 50%, #03122b 80%, #0a3a7a 100%);
  }
  .layer {
    position: absolute;
    left: 0;
    top: 0;
    will-change: transform;
  }
  @keyframes -global-fishy-ocean-drift {
    from {
      transform: translateX(0);
    }
    to {
      transform: translateX(calc(-1 * var(--tile)));
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .layer {
      animation: none !important;
    }
  }
  /* The Reduce motion setting (spec §6f.1). */
  :global(html[data-reduce-motion]) .layer {
    animation: none !important;
  }
</style>
