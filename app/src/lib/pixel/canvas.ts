import type { Grid, Palette } from "./grid";

/** Draw a pixel grid onto a 2D canvas: one `s`×`s` block per palette pixel. */
export function drawGrid(
  ctx: CanvasRenderingContext2D,
  grid: Grid,
  palette: Palette,
  x: number,
  y: number,
  s: number,
  o: { flip?: boolean; vflip?: boolean } = {},
): void {
  const h = grid.length;
  const w = grid[0]?.length ?? 0;
  for (let r = 0; r < h; r++) {
    const row = grid[r];
    for (let c = 0; c < w; c++) {
      const ch = row[c];
      const color = palette[ch];
      if (ch === "." || !color) continue;
      ctx.fillStyle = color;
      const cc = o.flip ? w - 1 - c : c;
      const rr = o.vflip ? h - 1 - r : r;
      ctx.fillRect(Math.round(x + cc * s), Math.round(y + rr * s), s, s);
    }
  }
}
