/** One string per row, one char per pixel. "." is transparent. */
export type Grid = string[];
/** Maps a grid char to a CSS color. */
export type Palette = Record<string, string>;
export type Rect = { x: number; y: number; color: string };

/** Parse a text grid: drops blank lines and any leading row-number prefix like " 3 ". */
export function parseGrid(text: string): Grid {
  return text
    .split(/\r?\n/)
    .map((line) => line.replace(/^\s*\d+\s+/, "").trim())
    .filter((line) => line.length > 0);
}

/** One rect per pixel whose char is in the palette, in row-major order. */
export function gridToRects(grid: Grid, palette: Palette): Rect[] {
  const rects: Rect[] = [];
  grid.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) {
      const color = palette[row[x]];
      if (row[x] !== "." && color) rects.push({ x, y, color });
    }
  });
  return rects;
}
