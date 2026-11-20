<script lang="ts">
  import { gridToRects, type Grid, type Palette } from "./grid";

  let {
    grid,
    palette,
    scale = 4,
    flip = false,
    vflip = false,
    title = undefined,
  }: {
    grid: Grid;
    palette: Palette;
    scale?: number;
    flip?: boolean;
    vflip?: boolean;
    title?: string;
  } = $props();

  const w = $derived(grid[0]?.length ?? 0);
  const h = $derived(grid.length);
  const rects = $derived(gridToRects(grid, palette));
  const transform = $derived(
    flip || vflip ? `translate(${flip ? w : 0} ${vflip ? h : 0}) scale(${flip ? -1 : 1} ${vflip ? -1 : 1})` : undefined,
  );
</script>

<svg
  viewBox="0 0 {w} {h}"
  width={w * scale}
  height={h * scale}
  shape-rendering="crispEdges"
  role={title ? "img" : undefined}
  aria-hidden={title ? undefined : "true"}
>
  {#if title}<title>{title}</title>{/if}
  <g {transform}>
    {#each rects as r (r.x + "," + r.y)}
      <rect x={r.x} y={r.y} width="1" height="1" fill={r.color} />
    {/each}
  </g>
</svg>

<style>
  svg {
    display: block;
    image-rendering: pixelated;
  }
</style>
