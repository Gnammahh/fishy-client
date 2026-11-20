// Generates the Fishy Client app icon from assets/sprites/fish-icon.txt.
// Same drawing as the approved mockup: a black 16x16-cell tile with pixel-stepped
// rounded corners, and the 14x10 aqua fish placed at cell (1, 3).
//
// Usage (from app/):  node ../scripts/gen-icon.mjs   -> writes src-tauri/icons/app-icon.png (1024x1024)
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(new URL("../app/package.json", import.meta.url));
const { PNG } = require("pngjs");

const CUT = [3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3];
const FISH_AT = { col: 1, row: 3 };
const BLACK = [0, 0, 0, 255];
const AQUA = [79, 227, 209, 255];

function parse(text) {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);
}

/** @returns {import("pngjs").PNG} */
export function renderIcon(gridText, size) {
  if (size % 16 !== 0) throw new Error("size must be a multiple of 16");
  const cell = size / 16;
  const fish = parse(gridText);
  const png = new PNG({ width: size, height: size }); // starts fully transparent

  const fillCell = (col, row, rgba) => {
    for (let y = row * cell; y < (row + 1) * cell; y++)
      for (let x = col * cell; x < (col + 1) * cell; x++) {
        const i = (size * y + x) * 4;
        png.data[i] = rgba[0];
        png.data[i + 1] = rgba[1];
        png.data[i + 2] = rgba[2];
        png.data[i + 3] = rgba[3];
      }
  };

  for (let row = 0; row < 16; row++)
    for (let col = CUT[row]; col < 16 - CUT[row]; col++) fillCell(col, row, BLACK);

  fish.forEach((line, r) => {
    [...line].forEach((ch, c) => {
      if (ch === "#" || ch === "e") fillCell(FISH_AT.col + c, FISH_AT.row + r, AQUA);
    });
  });
  return png;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const root = new URL("../", import.meta.url);
  const text = readFileSync(new URL("assets/sprites/fish-icon.txt", root), "utf8");
  const out = new URL("app/src-tauri/icons/app-icon.png", root);
  writeFileSync(out, PNG.sync.write(renderIcon(text, 1024)));
  console.log("wrote", fileURLToPath(out));
}
