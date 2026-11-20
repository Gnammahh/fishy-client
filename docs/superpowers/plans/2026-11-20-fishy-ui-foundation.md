# Fishy Client — UI Foundation (Milestones 0–1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri desktop app called "Fishy Client" that opens a window showing the ocean background, the fish loader, the fish download bar in every state, and the pixel fish app icon. It uses fake data and has no Epic code yet.

**Architecture:** A Cargo workspace at the repo root with one member for now, `app/src-tauri` (the Tauri shell). The UI is Svelte 5 + TypeScript in `app/src`. All fish art is defined as text pixel grids in TypeScript and drawn as SVG `<rect>`s, so there are no image files to hand-edit. Animation timing and math live in pure TS functions tested with Vitest. The Svelte components only render what those functions return.

**Tech Stack:** Rust (stable, via rustup) · Tauri 2 · Svelte 5 + TypeScript + Vite · Vitest + @testing-library/svelte · @fontsource/pixelify-sans, @fontsource/inter · pngjs (icon generation script).

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` (sections 2, 3, 4-layout, 5, 9-milestones 0–1, 10)

## Global Constraints

- App/window title: `Fishy Client`. Executable/Cargo package name: `fishy-client`. Bundle identifier: `com.fishyclient.app`.
- Window minimum size 960×600, resizable.
- Colors (exact): background `#000000`, panel `#0B0F14`, aqua `#4FE3D1`, orange `#FF9F43`, text `#E8F1F2`, secondary `#7A8A99`, error `#FF5C5C`, success `#3DDC84`, navy `#03122B`, ocean `#0A3A7A`, wave2 `#0E5AA8`, wave3 `#1B7FD1`.
- Fonts bundled locally (no CDN at runtime): Pixelify Sans for headings and buttons, Inter for body text.
- Pixel art: whole-number scaling only, `image-rendering: pixelated` / `shape-rendering: crispEdges`, frame-stepped animation (no smooth tweening of sprite frames).
- Every animation has a reduced-motion version, driven by `prefers-reduced-motion: reduce` (WebView2 reflects the Windows "Show animations" setting).
- No Epic logos or branding anywhere.
- License: GPL-3.0-or-later.
- Windows 11 is the target. Don't add Linux- or macOS-specific code.

## Review Focus

1. **Download total unknown or 0 bytes.** The progress text must show `0%` and never `NaN`, `Infinity` or `~∞`. → test in Task 5.
2. **Speed 0 (stalled or paused).** The time-left part must show `—` instead of a huge or infinite number. → test in Task 5.
3. **Progress outside 0–1** (a rounding error from the downloader gives 1.0000001 or -0). The value is clamped, so the dots eaten never go above the dot count and the fish never leaves the bar. → test in Task 5.
4. **Very wide or very narrow windows** (960 px up to a 3840 px 4K screen). The waves must cover the full width with no gap at the seam while they drift. → test in Task 3 (path width) plus a manual check in Task 6.
5. **Reduced motion turned on while the app is running.** The animations must stop without a restart. → the CSS media query handles it, and Task 6 has a manual check.

---

## File Structure

```
fishy-client/
├─ Cargo.toml                      workspace root (members: app/src-tauri)
├─ LICENSE                         GPL-3.0 text
├─ README.md                       name, one-paragraph description, dev commands
├─ assets/sprites/fish-icon.txt    16×16 icon grid (source of truth for the icon)
├─ scripts/gen-icon.mjs            grid → app/src-tauri/icons/app-icon.png (1024×1024)
└─ app/
   ├─ package.json, vite.config.ts, vitest config (in vite.config.ts)
   ├─ src-tauri/                   Tauri shell (tauri.conf.json, Cargo.toml, src/main.rs, icons/)
   └─ src/
      ├─ lib/theme.css             color variables + font faces + base styles
      ├─ lib/motion.ts             reducedMotion store
      ├─ lib/pixel/grid.ts         parseGrid, gridToRects
      ├─ lib/pixel/sprites.ts      all fish/pellet/puff sprite grids + palettes
      ├─ lib/pixel/PixelSprite.svelte
      ├─ lib/ocean/wave.ts         steppedWavePath
      ├─ lib/ocean/OceanBackground.svelte
      ├─ lib/loader/loaderFrame.ts fishLoaderFrame
      ├─ lib/loader/FishLoader.svelte
      ├─ lib/progress/progress.ts  clampProgress, dotsEaten, formatProgress
      ├─ lib/progress/FishProgressBar.svelte
      └─ routes/+page.svelte       (or the template's main page) → preview page
```
Tests sit next to the code as `*.test.ts`.

---

### Task 1: Toolchain + Tauri app skeleton (Milestone 0)

**Files:**
- Create: `Cargo.toml`, `LICENSE`, `README.md`, `app/` (from template), `app/src/lib/theme.css`, `app/src/lib/motion.ts`
- Modify: `app/src-tauri/tauri.conf.json`, `app/src-tauri/Cargo.toml`, `.gitignore`

**Interfaces:**
- Produces: CSS variables `--bg --panel --aqua --orange --text --text-2 --error --success --navy --ocean --wave2 --wave3`, plus font families `--font-pixel` ("Pixelify Sans") and `--font-body` ("Inter"). Also `reducedMotion: Readable<boolean>` from `app/src/lib/motion.ts`. Vitest is runnable with `npm test` in `app/`.

- [ ] **Step 1: Install the toolchain (user's machine).** Ask the user for approval before installing. Then run `winget install Rustlang.Rustup` and `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`, and open a new terminal afterwards.
  Verify: `cargo --version` and `rustc --version` print versions. `node --version` prints v26.x.

- [ ] **Step 2: Scaffold.** From the repo root, run `npm create tauri-app@latest app -- --template svelte-ts --manager npm --identifier com.fishyclient.app --yes`, then `cd app && npm install`. Note whether the template made `src/routes/+page.svelte` (SvelteKit) or `src/App.svelte`. Later tasks call it "the main page".

- [ ] **Step 3: Configure.** Root `Cargo.toml`: `[workspace] members = ["app/src-tauri"]`, `resolver = "2"`. In `app/src-tauri/Cargo.toml` set the package `name = "fishy-client"` and `license = "GPL-3.0-or-later"`, and update the lib name used in `main.rs` to match. In `tauri.conf.json` set `productName: "Fishy Client"`, `identifier: "com.fishyclient.app"`, and a window with `title: "Fishy Client"`, `width: 1200`, `height: 760`, `minWidth: 960`, `minHeight: 600`, `resizable: true`, `backgroundColor: "#000000"`. Add `target/` and `node_modules/` to `.gitignore`. Add the GPL-3.0 `LICENSE` text and a short `README.md`.

- [ ] **Step 4: Theme + fonts + motion store.** `npm i @fontsource/pixelify-sans @fontsource/inter`, `npm i -D vitest @testing-library/svelte jsdom`, and add `"test": "vitest run"` to package.json with `test.environment = "jsdom"` in the Vite config. `theme.css` defines the Global Constraints colors as variables, imports both fontsource packages, and sets `html, body { margin:0; background: var(--bg); color: var(--text); font-family: var(--font-body); }`. Import it from the main page or layout. `motion.ts` exports `reducedMotion`, a readable store backed by `matchMedia('(prefers-reduced-motion: reduce)')` that updates on its `change` event.

- [ ] **Step 5: Write test `app/src/lib/motion.test.ts`.** Name it `reducedMotion follows the media query`: stub `window.matchMedia` to return `matches: true`, then assert `get(reducedMotion) === true`. Fire the stored `change` listener with `matches: false` and assert it's `false`.

- [ ] **Step 6: Run** `npm test`. Expected: 1 passed.

- [ ] **Step 7: Run** `npm run tauri dev`. Expected: a black window titled "Fishy Client" opens and can't be resized below 960×600. (The first Rust build takes a few minutes.)

- [ ] **Step 8: Commit.** `git add -A && git commit -m "chore: scaffold Fishy Client Tauri app with theme and fonts"`

---

### Task 2: Pixel grid engine + sprites

**Files:**
- Create: `app/src/lib/pixel/grid.ts`, `app/src/lib/pixel/grid.test.ts`, `app/src/lib/pixel/sprites.ts`, `app/src/lib/pixel/sprites.test.ts`, `app/src/lib/pixel/PixelSprite.svelte`

**Interfaces:**
- Produces:
  - `type Grid = string[]` (each string is one row and each char is one pixel; `.` = transparent)
  - `type Palette = Record<string, string>` (char → CSS color)
  - `type Rect = { x: number; y: number; color: string }`
  - `parseGrid(text: string): Grid`: splits on newlines, trims each line, drops empty lines, and strips any leading row-number prefix matching `/^\s*\d+\s+/`
  - `gridToRects(grid: Grid, palette: Palette): Rect[]`: one rect per non-`.` char found in the palette (unknown chars are skipped), in row-major order
  - `sprites.ts` exports `FISH_SWIM: [Grid, Grid]`, `FISH_TURN: Grid`, `FISH_MOUTH_OPEN: Grid`, `FISH_MOUTH_CLOSED: Grid`, `FISH_FLIP: [Grid, Grid, Grid]`, `FISH_DEAD: Grid` (x eye, for the error state), `PELLET: Grid`, `PUFF: Grid`, `BUBBLE: Grid`, and `FISH_PALETTE`, `FISH_GRAY_PALETTE`, `PELLET_PALETTE`
  - `<PixelSprite grid palette scale={4} flip={false} title?>`: an SVG with `viewBox="0 0 W H"`, width/height = W·scale, H·scale, `shape-rendering="crispEdges"` and a 1×1 `<rect>` per pixel. `flip` mirrors it horizontally (to face left).

- [ ] **Step 1: Write tests in `grid.test.ts`**
  - `parseGrid strips row numbers and blank lines`: input `"\n 0 .#.\n 1 ##.\n"` → `[".#.", "##."]`
  - `gridToRects maps palette chars only`: `gridToRects([".#", "o?"], {"#":"#4FE3D1","o":"#FF9F43"})` deep-equals `[{x:1,y:0,color:"#4FE3D1"},{x:0,y:1,color:"#FF9F43"}]`

- [ ] **Step 2: Write tests in `sprites.test.ts`**
  - `all fish frames are 16x16`: every row of every FISH_* grid has length 16, and each grid has 16 rows
  - `fish frames face right and have an eye`: each of FISH_SWIM[0], FISH_SWIM[1], FISH_MOUTH_OPEN and FISH_MOUTH_CLOSED contains exactly one `e` char (the eye pixel), and its column is ≥ 8
  - `pellet is 4x4, bubble is 4x4, puff is 6x6`

- [ ] **Step 3: Run** `npm test`. Expected: FAIL (modules not found).

- [ ] **Step 4: Implement `grid.ts`, then draw `sprites.ts`.** The fish is an **outline only** with a hollow body: `#` = aqua `#4FE3D1` outline, `e` = eye (`#E8F1F2`), `.` = transparent. Base it on the icon grid in spec §2, with the tail pixels connected to the body. Swim frame 2 moves the tail up or down by 1 px. MOUTH_OPEN cuts a 2-px notch at the nose. TURN is a narrow, front-facing body. The FLIP frames are rotated 0°, 90° and 180°. FISH_DEAD has an `x` in place of the eye and uses FISH_GRAY_PALETTE (`#7A8A99`). PELLET is a solid orange `o` (`#FF9F43`) blob. PUFF is a 6×6 ring of orange specks. BUBBLE is a 4×4 aqua ring.

- [ ] **Step 5: Implement `PixelSprite.svelte`** as described in Interfaces.

- [ ] **Step 6: Run** `npm test`. Expected: all pass.

- [ ] **Step 7: Commit.** `git commit -am "feat(ui): pixel grid engine and fish sprites"` (`git add` the new files first)

---

### Task 3: App icon from the grid

**Files:**
- Create: `assets/sprites/fish-icon.txt`, `scripts/gen-icon.mjs`, `scripts/gen-icon.test.mjs`
- Modify: `app/package.json` (add the script `"icon": "node ../scripts/gen-icon.mjs && tauri icon src-tauri/icons/app-icon.png"`), `app/src-tauri/icons/*` (generated)

**Interfaces:**
- Consumes: the same grid format as `parseGrid`. The script has its own small parser so it doesn't need a TS build. It accepts `#` and `e` as aqua and everything else as black.
- Produces: `renderIcon(gridText: string, size: number): PNG` exported from `gen-icon.mjs` (a pngjs PNG object). Running the file directly writes `app/src-tauri/icons/app-icon.png` at 1024×1024.

- [ ] **Step 1: Write `assets/sprites/fish-icon.txt`.** Copy the spec §2 draft grid without row numbers, using the corrected connected tail from Task 2's swim frame 1, with the eye as `#` (the icon is single-color).

- [ ] **Step 2: Write test `scripts/gen-icon.test.mjs`** (Vitest can run it: `npx vitest run ../scripts` from `app/`)
  - `renderIcon is square at requested size`: `renderIcon(text, 1024)` has width === height === 1024
  - `background is pure black and outline is aqua`: pixel (0,0) is RGBA `0,0,0,255`. The pixel at the center of grid cell (row 3, col 7), i.e. `(7*64+32, 3*64+32)`, is `79,227,209,255`

- [ ] **Step 3: Run test.** Expected: FAIL.

- [ ] **Step 4: Implement `renderIcon`.** `npm i -D pngjs` in `app/`. Each grid cell is a solid `size/16` block (size must be divisible by 16, otherwise throw `Error("size must be a multiple of 16")`), with no anti-aliasing.

- [ ] **Step 5: Run test.** Expected: PASS.

- [ ] **Step 6: Run** `npm run icon` in `app/`. Expected: `src-tauri/icons/` contains `icon.ico`, `32x32.png`, `128x128.png` and `128x128@2x.png`, and `npm run tauri dev` shows the fish in the taskbar.

- [ ] **Step 7: Commit.** `git add -A && git commit -m "feat: pixel fish app icon generated from grid"`

---

### Task 4: Ocean background

**Files:**
- Create: `app/src/lib/ocean/wave.ts`, `app/src/lib/ocean/wave.test.ts`, `app/src/lib/ocean/OceanBackground.svelte`

**Interfaces:**
- Produces:
  - `steppedWavePath(opts: { width: number; height: number; baseY: number; amplitude: number; wavelength: number; step: number }): string`: an SVG path `d` that starts at `M0,{height}`, rises to the wave edge, follows a sine-shaped edge **quantized to `step`-px stairs** (every vertex x and y is a multiple of `step`, alternating horizontal and vertical segments), and closes along the bottom (`…L{width},{height} Z`). `width` must be a whole number of wavelengths, so tiles join seamlessly.
  - `<OceanBackground />`: a fixed full-window layer (`position:fixed; inset:0; z-index:-1`). The background is `linear-gradient(to bottom, #000000 0%, #000000 50%, #03122B 80%, #0A3A7A 100%)`. 3 wave layers sit in the bottom 35%, using colors `#0A3A7A` @ 45%, `#0E5AA8` @ 35% and `#1B7FD1` @ 25% opacity, step 4 px, wavelengths 320/240/180 px, and loop durations 40 s/30 s/20 s. Each layer is an SVG 2 tiles wide (path width = 2 × tile width ≥ 3840 px) animated with `transform: translateX(0 → -tileWidth)` (linear, infinite). Under `prefers-reduced-motion: reduce` there's `animation: none`.

- [ ] **Step 1: Write tests in `wave.test.ts`**
  - `path starts bottom-left and closes bottom-right`: for `{width:640,height:200,baseY:100,amplitude:20,wavelength:320,step:4}`, `d` starts with `"M0,200"` and ends with `"L640,200 Z"`
  - `all vertices snap to the step grid`: parse every number pair from `d` and assert `x % 4 === 0 && y % 4 === 0`
  - `edge stays within amplitude`: every vertex that isn't on the bottom line has `y` within `[80, 120]`
  - `edge is periodic`: the y of the vertex at x=0 equals the y at x=320 and at x=640 (seamless tiling)
  - `rejects width that is not a whole number of wavelengths`: `width:500` throws

- [ ] **Step 2: Run tests.** Expected: FAIL.

- [ ] **Step 3: Implement `steppedWavePath`.** Sample at every `step` px of x, compute `y = baseY + amplitude·sin(2π·x/wavelength)`, round to the nearest multiple of `step`, and emit `H` then `V` segments only when y changes.

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Implement `OceanBackground.svelte`** as described in Interfaces, and mount it once in the main page or layout.

- [ ] **Step 6: Check by hand.** `npm run tauri dev`: a black top fades to blue at the bottom, with three pixel-stepped wave bands drifting at different speeds. Maximize the window: there's no gap at the right edge.

- [ ] **Step 7: Commit.** `git add -A && git commit -m "feat(ui): deep-water ocean background"`

---

### Task 5: FishProgressBar

**Files:**
- Create: `app/src/lib/progress/progress.ts`, `app/src/lib/progress/progress.test.ts`, `app/src/lib/progress/FishProgressBar.svelte`

**Interfaces:**
- Consumes: `PixelSprite`, `FISH_SWIM`, `FISH_MOUTH_OPEN`, `FISH_MOUTH_CLOSED`, `FISH_DEAD`, `BUBBLE`, `PELLET`, `FISH_GRAY_PALETTE` (Task 2), and `reducedMotion` (Task 1)
- Produces:
  - `type DownloadState = 'downloading' | 'paused' | 'verifying' | 'error' | 'done'`
  - `clampProgress(p: number): number`: returns 0 for NaN, otherwise clamps to [0, 1]
  - `dotsEaten(progress: number, dotCount: number): number` = `Math.floor(clampProgress(progress) * dotCount)`
  - `formatBytes(bytes: number): string`: binary units, 1 decimal place, labelled `B`/`KB`/`MB`/`GB` (1 GB = 1024³ bytes)
  - `formatProgress(downloaded: number, total: number, bytesPerSec: number): string` → `"42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left"`. The percent is `Math.round`. Time left: under 60 s → `~N s left`, otherwise `~N min left` (rounded). If speed ≤ 0 or total ≤ 0, the time left is `—`.
  - `<FishProgressBar downloaded total bytesPerSec state on:retry on:play />`

- [ ] **Step 1: Write tests in `progress.test.ts`**
  - `formats the spec example`: `formatProgress(3.1*1024**3, 7.4*1024**3, 18.2*1024**2) === "42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left"`
  - `unknown total shows 0% and no infinity`: `formatProgress(0, 0, 0) === "0% · 0.0 B / 0.0 B · 0.0 B/s · —"` *(Review Focus 1)*
  - `stalled speed shows dash`: `formatProgress(1024**3, 2*1024**3, 0).endsWith("· —")` *(Review Focus 2)*
  - `seconds when under a minute`: `formatProgress(0, 30*1024**2, 1024**2).endsWith("~30 s left")`
  - `clampProgress handles out-of-range`: `clampProgress(1.0000001) === 1`, `clampProgress(-0.2) === 0` and `clampProgress(NaN) === 0` *(Review Focus 3)*
  - `dotsEaten never exceeds dotCount`: `dotsEaten(1.0000001, 40) === 40` and `dotsEaten(0.5, 40) === 20`

- [ ] **Step 2: Run tests.** Expected: FAIL.

- [ ] **Step 3: Implement `progress.ts`.**

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Implement `FishProgressBar.svelte`.** The track is a full-width row of `dotCount = 40` orange 2×2-px-scaled dots (scale 2). Dots at index < `dotsEaten` are hidden. The fish (scale 2, 32 px) is absolutely positioned at `left: progress·100%`, offset by half its width, and clamped inside the track. Per state:
  - **downloading:** alternates MOUTH_OPEN/MOUTH_CLOSED every 150 ms, and a crunch PUFF flashes for 150 ms whenever `dotsEaten` increases
  - **paused:** shows FISH_TURN with BUBBLE sprites rising (CSS stepped animation)
  - **verifying:** the fish swims back and forth along the eaten part (an 8 s stepped loop) with BUBBLE as the magnifier
  - **error:** shows FISH_DEAD in gray, the track border turns `#FF5C5C`, and there's a "Retry" button (Pixelify font) that fires `retry`
  - **done:** a FLIP sequence plays (3 frames × 120 ms) with a PUFF splash, then the component renders a "Play" button that fires `play`

  The `formatProgress` text sits under the bar in `--text-2`. With reduced motion there are no chomp/bubble/flip frames: the fish just sits at the progress position, and done shows the Play button immediately.

- [ ] **Step 6: Commit.** `git add -A && git commit -m "feat(ui): fish download progress bar"`

---

### Task 6: FishLoader + preview page

**Files:**
- Create: `app/src/lib/loader/loaderFrame.ts`, `app/src/lib/loader/loaderFrame.test.ts`, `app/src/lib/loader/FishLoader.svelte`
- Modify: the main page (`app/src/routes/+page.svelte` or `app/src/App.svelte`)

**Interfaces:**
- Consumes: Task 2 sprites/`PixelSprite`, `reducedMotion`, `OceanBackground`, `FishProgressBar`
- Produces:
  - `type LoaderPhase = 'fall' | 'swim' | 'eat' | 'return'`
  - `type LoaderFrame = { phase: LoaderPhase; fishX: number; facing: 'left' | 'right'; mouthOpen: boolean; pelletY: number; pelletVisible: boolean }` (x and y are 0–1 fractions of the tank)
  - `CYCLE_MS = 1200`
  - `fishLoaderFrame(elapsedMs: number, pelletX: number): LoaderFrame`, using `t = elapsedMs % 1200`:
    - **fall** `[0,400)`: pelletY goes 0→1 linearly, the fish is at 0.5 facing the pellet
    - **swim** `[400,700)`: fishX goes 0.5→pelletX, facing the pellet
    - **eat** `[700,850)`: fishX = pelletX, mouthOpen = true for `[700,775)`, the pellet is hidden from 775
    - **return** `[850,1200)`: fishX goes pelletX→0.5, facing the centre, pellet hidden
    - When pelletX === 0.5, facing stays `'right'`
  - `<FishLoader size="small|medium|large" done={false} />`: fish scale 2/4/8 (32/64/128 px). It picks a new random `pelletX` in [0.15, 0.85] each cycle and draws with `requestAnimationFrame`. While `done` is false, the sprite is FISH_SWIM (alternating every 200 ms) or MOUTH_OPEN. When `done` turns true, it finishes the current cycle, plays FISH_FLIP, then fades out over 300 ms. With reduced motion it shows a static fish plus 3 pellets fading in and out (CSS opacity only).

- [ ] **Step 1: Write tests in `loaderFrame.test.ts`**
  - `starts with pellet at top and fish centred`: `fishLoaderFrame(0, 0.8)` → `{phase:'fall', fishX:0.5, pelletY:0, pelletVisible:true, facing:'right'}`
  - `fish faces left for a pellet on the left`: `fishLoaderFrame(500, 0.2).facing === 'left'`
  - `fish reaches pellet and eats`: `fishLoaderFrame(720, 0.3)` → `phase 'eat'`, `fishX 0.3`, `mouthOpen true`
  - `pellet gone after bite`: `fishLoaderFrame(800, 0.3).pelletVisible === false`
  - `returns to centre and faces it`: `fishLoaderFrame(1199, 0.3).fishX` is close to 0.5 (±0.01) and its `facing === 'right'`
  - `cycle repeats`: `fishLoaderFrame(1200 + 100, 0.6)` deep-equals `fishLoaderFrame(100, 0.6)`

- [ ] **Step 2: Run tests.** Expected: FAIL.

- [ ] **Step 3: Implement `loaderFrame.ts`.**

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Implement `FishLoader.svelte`.** The tank is a rounded `--panel` box at 85% opacity with `backdrop-filter: blur(6px)`. Its size is 4× the fish width by 3× the fish height.

- [ ] **Step 6: Build the preview page.** The main page shows `OceanBackground`, a "Fishy Client" heading in Pixelify Sans in aqua, and three sections:
  - **(a) Loader:** all three sizes, plus a "Finish" button that sets `done`
  - **(b) Download bar:** one bar per state with fixed fake numbers, plus one live bar that fakes a 7.4 GB download at 18.2 MB/s, sped up so it finishes in 20 s
  - **(c) Icon:** `fish-icon.txt` shown with PixelSprite at scale 4 and 8 on a black square

- [ ] **Step 7: Run** `npm test` (all pass) and `npm run tauri dev`. **Check by hand:**
  - The loader fish chases each pellet and returns
  - The live bar's fish eats the dots and ends with a splash and a Play button
  - The error bar shows Retry
  - Turn on Windows Settings → Accessibility → Visual effects → "Animation effects" **Off** while the app is open: everything stops animating without a restart *(Review Focus 5)*
  - Resize to the 960×600 minimum and maximize: no layout overflow and no wave gaps *(Review Focus 4)*

- [ ] **Step 8: Commit.** `git add -A && git commit -m "feat(ui): fish loader and component preview page"`

---

## Next plans (not in this plan)
Milestone 2 (login) adds `crates/fishy-core` and `crates/fishy-cli` to the workspace and gets its own plan, written once this one is done.
