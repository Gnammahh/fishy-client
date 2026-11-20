import type { Grid, Palette } from "./grid";

// All grids are copied verbatim from the approved design/mockup.html.
// Fish frames are 16x16 and face right. "#" outline, "e" eye, "x" dead eye.

const FISH_A: Grid = [
  "................",
  "........##......",
  ".......#####....",
  ".....##.....##..",
  "#...#.........#.",
  "##.#...........#",
  "#.##........e..#",
  ".#.#...........#",
  "#.##...........#",
  "##.#....##.....#",
  "#...#.........#.",
  ".....##.....##..",
  ".......#####....",
  "................",
  "................",
  "................",
];

const FISH_B: Grid = FISH_A.map((row, r) => {
  const tail: Record<number, string> = {
    4: ".#..#.........#.",
    5: ".###...........#",
    6: ".#.#........e..#",
    7: "..##...........#",
    8: ".#.#...........#",
    9: ".###....##.....#",
    10: ".#..#.........#.",
  };
  return tail[r] ?? row;
});

export const FISH_MOUTH_CLOSED: Grid = FISH_A;
export const FISH_SWIM: [Grid, Grid] = [FISH_A, FISH_B];

export const FISH_MOUTH_OPEN: Grid = FISH_A.map((row, r) =>
  r === 7 ? ".#.#.........#.." : r === 8 ? "#.##.........#.." : row,
);

export const FISH_DEAD: Grid = FISH_A.map((row, r) =>
  r === 5 ? "##.#.......x.x.#" : r === 6 ? "#.##........x..#" : r === 7 ? ".#.#.......x.x.#" : row,
);

export const PELLET: Grid = [".oo.", "ohoo", "oooo", ".oo."];
export const BUBBLE: Grid = [".bb.", "b..b", "b..b", ".bb."];
export const PUFF: Grid = ["o....o", "..o...", "o...o.", ".o....", "...o.o", "o.o..."];

/** 14x10 fish for the app icon, placed at cell (1,3) on a 16x16 tile. */
export const ICON_FISH: Grid = [
  "......#####...",
  "....##.....##.",
  "#..#.........#",
  "##.#......e..#",
  ".###.........#",
  ".###.........#",
  "##.#.........#",
  "#..#.........#",
  "....##.....##.",
  "......#####...",
];

export const FISH_PALETTE: Palette = { "#": "#4FE3D1", e: "#E8F1F2", x: "#E8F1F2" };
export const FISH_GRAY_PALETTE: Palette = { "#": "#7A8A99", e: "#E8F1F2", x: "#E8F1F2" };
export const PELLET_PALETTE: Palette = { o: "#FF9F43", h: "#FFD3A1" };
export const BUBBLE_PALETTE: Palette = { b: "#9FF3EA" };
export const PUFF_PALETTE: Palette = { o: "#FF9F43" };
