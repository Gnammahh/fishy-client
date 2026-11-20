# Fishy Client — UI Foundation (Milestones 0–1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri desktop app called "Fishy Client" that opens a window showing the ocean background, the fish loader, the fish download bar in every state, and the pixel fish app icon. It uses fake data and has no Epic code yet.

**Architecture:** A Cargo workspace at the repo root with one member for now, `app/src-tauri` (the Tauri shell). The UI is Svelte 5 + TypeScript in `app/src`. All fish art is defined as text pixel grids in TypeScript and drawn as SVG `<rect>`s, so there are no image files to hand-edit. Animation timing and math live in pure TS functions tested with Vitest. The Svelte components only render what those functions return.

**Tech Stack:** Rust (stable, via rustup) · Tauri 2 · Svelte 5 + TypeScript + Vite · Vitest + @testing-library/svelte · @fontsource/pixelify-sans, @fontsource/inter · pngjs (icon generation script).

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` (sections 2, 3, 4-layout, 5, 9-milestones 0–1, 10)

**Approved visual reference:** `design/mockup.html` (the user approved it on 2026-11-20). Copy its sprite grids verbatim (`FISH_A`, `FISH_B`, `OPEN`, `DEAD`, `PELLET`, `BUBBLE`, `PUFF`, `ICON_FISH`, `CUT`) and match its timings, colors and wave values. Where this plan and the mockup disagree on a visual detail, the mockup wins.

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
  - `sprites.ts` exports `FISH_SWIM: [Grid, Grid]` (mockup `FISH_A`, `FISH_B`), `FISH_MOUTH_OPEN: Grid` (mockup `OPEN`), `FISH_MOUTH_CLOSED: Grid` (= `FISH_A`), `FISH_DEAD: Grid` (x eye, for the error state), `PELLET: Grid`, `PUFF: Grid`, `BUBBLE: Grid`, `ICON_FISH: Grid` (14×10), and `FISH_PALETTE`, `FISH_GRAY_PALETTE`, `PELLET_PALETTE`. There are no separate turn or flip frames: the "happy flip" is `FISH_MOUTH_CLOSED` drawn with `vflip`, and the error fish is `FISH_DEAD` with `vflip` (belly up).
  - `<PixelSprite grid palette scale={4} flip={false} vflip={false} title?>`: an SVG with `viewBox="0 0 W H"`, width/height = W·scale, H·scale, `shape-rendering="crispEdges"` and a 1×1 `<rect>` per pixel. `flip` mirrors it horizontally (to face left) and `vflip` mirrors it vertically.

- [ ] **Step 1: Write tests in `grid.test.ts`**
  - `parseGrid strips row numbers and blank lines`: input `"\n 0 .#.\n 1 ##.\n"` → `[".#.", "##."]`
  - `gridToRects maps palette chars only`: `gridToRects([".#", "o?"], {"#":"#4FE3D1","o":"#FF9F43"})` deep-equals `[{x:1,y:0,color:"#4FE3D1"},{x:0,y:1,color:"#FF9F43"}]`

- [ ] **Step 2: Write tests in `sprites.test.ts`**
  - `all fish frames are 16x16`: every row of every FISH_* grid has length 16, and each grid has 16 rows
  - `fish frames face right and have an eye`: each of FISH_SWIM[0], FISH_SWIM[1], FISH_MOUTH_OPEN and FISH_MOUTH_CLOSED contains exactly one `e` char (the eye pixel), and its column is ≥ 8
  - `pellet is 4x4, bubble is 4x4, puff is 6x6, icon fish is 14x10`

- [ ] **Step 3: Run** `npm test`. Expected: FAIL (modules not found).

- [ ] **Step 4: Implement `grid.ts`, then fill `sprites.ts`** by copying the grids from `design/mockup.html` verbatim. Palettes are the mockup's `PAL`: `#` `#4FE3D1`, `e`/`x` `#E8F1F2`, `o` `#FF9F43`, `h` `#FFD3A1`, `b` `#9FF3EA`. FISH_GRAY_PALETTE maps `#` to `#7A8A99`.

- [ ] **Step 5: Implement `PixelSprite.svelte`** as described in Interfaces.

- [ ] **Step 6: Run** `npm test`. Expected: all pass.

- [ ] **Step 7: Commit.** `git commit -am "feat(ui): pixel grid engine and fish sprites"` (`git add` the new files first)

---

### Task 3: App icon from the grid

**Files:**
- Create: `assets/sprites/fish-icon.txt`, `scripts/gen-icon.mjs`, `scripts/gen-icon.test.mjs`
- Modify: `app/package.json` (add the script `"icon": "node ../scripts/gen-icon.mjs && tauri icon src-tauri/icons/app-icon.png"`), `app/src-tauri/icons/*` (generated)

**Interfaces:**
- Consumes: the same grid format as `parseGrid`. The script has its own small parser so it doesn't need a TS build. `#` and `e` are aqua and everything else is transparent.
- Produces: `renderIcon(gridText: string, size: number): PNG` exported from `gen-icon.mjs` (a pngjs PNG object), drawn exactly like the mockup's `paintIcon`. On a 16-cell tile it draws a black tile with corners cut by `CUT = [3,2,1,0,0,0,0,0,0,0,0,0,0,1,2,3]` cells per row on each side (cut cells are fully transparent, alpha 0), then the 14×10 fish placed at cell (1, 3). Running the file directly writes `app/src-tauri/icons/app-icon.png` at 1024×1024.

- [ ] **Step 1: Write `assets/sprites/fish-icon.txt`** containing the mockup's `ICON_FISH` grid (14 columns × 10 rows).

- [ ] **Step 2: Write test `scripts/gen-icon.test.mjs`** (Vitest can run it: `npx vitest run ../scripts` from `app/`)
  - `renderIcon is square at requested size`: `renderIcon(text, 1024)` has width === height === 1024
  - `corners are transparent`: pixel (0,0) has alpha 0, and pixel (3*64+32, 32) (row 0, col 3) is opaque black `0,0,0,255`
  - `tile is black and fish is aqua`: pixel (32, 8*64+32) (row 8, col 0, empty margin) is `0,0,0,255`. The pixel at tile cell (row 3, col 7), which is fish cell (row 0, col 6), i.e. `(7*64+32, 3*64+32)`, is `79,227,209,255`
  - `fish is centered`: the bounding box of aqua pixels at size 16 is x 1..14 and y 3..12

- [ ] **Step 3: Run test.** Expected: FAIL.

- [ ] **Step 4: Implement `renderIcon`.** `npm i -D pngjs` in `app/`. Each cell is a solid `size/16` block (size must be divisible by 16, otherwise throw `Error("size must be a multiple of 16")`), with no anti-aliasing.

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
  - `<OceanBackground />`: a fixed full-window layer (`position:fixed; inset:0; z-index:-1`). The background is `linear-gradient(to bottom, #000000 0%, #000000 50%, #03122B 80%, #0A3A7A 100%)`. 3 wave layers sit in the bottom 35%, using colors `#0A3A7A` @ 45%, `#0E5AA8` @ 35% and `#1B7FD1` @ 25% opacity, step 4 px, wavelengths 320/240/180 px, amplitudes 18/14/10 px, edges at 70%/80%/90% of the window height, and drift speeds 8/12/18 px per second (loop duration = tile width ÷ speed). These match the approved mockup. Each layer is an SVG 2 tiles wide (path width = 2 × tile width ≥ 3840 px) animated with `transform: translateX(0 → -tileWidth)` (linear, infinite). Under `prefers-reduced-motion: reduce` there's `animation: none`.

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
- Consumes: `PixelSprite`, `FISH_SWIM`, `FISH_MOUTH_OPEN`, `FISH_MOUTH_CLOSED`, `FISH_DEAD`, `BUBBLE`, `PUFF`, `FISH_GRAY_PALETTE` (Task 2), and `reducedMotion` (Task 1)
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
  - **paused:** FISH_MOUTH_CLOSED bobbing by 1 sprite pixel, with 3 small BUBBLE sprites (scale 1) rising from the mouth
  - **verifying:** the fish swims back and forth along the eaten part (4 s each way), facing its direction of travel, with a scale-2 BUBBLE ahead of it as the magnifier
  - **error:** FISH_DEAD in gray with `vflip` (belly up), the remaining dots and a bottom line turn `#FF5C5C`, and there's a "Retry" button (Pixelify font) that fires `retry`
  - **done:** the flip plays (3 frames × 120 ms: up 6 px, up 12 px with `vflip`, up 6 px) with a PUFF splash, then the component renders a green "Play" button that fires `play`
  - Status text per state, exactly as in the mockup: paused → `Paused at 42%. 3.1 GB of 7.4 GB downloaded.`, verifying → `Checking installed files, 70% done.`, error → `Download stopped: the connection to the download server dropped.`, done → `Installed. 7.4 GB on disk.`

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
  - `CYCLE_MS = 1800` (matches the approved mockup)
  - `fishLoaderFrame(elapsedMs: number, pelletX: number): LoaderFrame`, using `t = elapsedMs % 1800`. fishX is the fish's **mouth-target position** (it reaches pelletX while eating); the component converts it to sprite coordinates the way the mockup does (`target = pellet − dir·7u`).
    - **fall** `[0,540)`: pelletY goes 0→1 linearly, the fish is at 0.5 facing the pellet
    - **swim** `[540,990)`: fishX goes 0.5→pelletX with ease-in-out, facing the pellet
    - **eat** `[990,1260)`: fishX = pelletX, mouthOpen = true for `[990,1125)`, the pellet is hidden from 1125
    - **return** `[1260,1800)`: fishX goes pelletX→0.5 with ease-in-out, facing the centre, pellet hidden
    - When pelletX === 0.5, facing stays `'right'`
  - `<FishLoader size="small|medium|large" done={false} />`: fish scale 2/4/8 (32/64/128 px) in a tank of 48×32 sprite pixels. Each cycle it picks a random `pelletX` in [0.15, 0.38] ∪ [0.62, 0.85] (never right next to the centre, so the fish never swims backwards) and draws with `requestAnimationFrame`, redrawing every ~70 ms so it moves in pixel steps. While swimming, the sprite alternates FISH_SWIM frames (every 220 ms while falling, 120 ms while chasing, 200 ms while returning). There's a 1-pixel bob during the fall and a PUFF at the bite. When `done` turns true, it finishes the current cycle, plays the flip (3 frames × 120 ms: up 1, up 3 with vflip, up 1, in sprite pixels), then fades out over 300 ms and shows "Loaded". With reduced motion it shows a static fish plus 3 pellets fading in and out.

- [ ] **Step 1: Write tests in `loaderFrame.test.ts`**
  - `starts with pellet at top and fish centred`: `fishLoaderFrame(0, 0.8)` → `{phase:'fall', fishX:0.5, pelletY:0, pelletVisible:true, facing:'right'}`
  - `fish faces left for a pellet on the left`: `fishLoaderFrame(700, 0.2).facing === 'left'`
  - `fish reaches pellet and eats`: `fishLoaderFrame(1000, 0.3)` → `phase 'eat'`, `fishX 0.3`, `mouthOpen true`
  - `pellet gone after bite`: `fishLoaderFrame(1150, 0.3).pelletVisible === false`
  - `returns to centre and faces it`: `fishLoaderFrame(1799, 0.3).fishX` is close to 0.5 (±0.01) and its `facing === 'right'`
  - `cycle repeats`: `fishLoaderFrame(1800 + 100, 0.6)` deep-equals `fishLoaderFrame(100, 0.6)`

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
