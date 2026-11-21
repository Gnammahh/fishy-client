<script lang="ts">
  import icon from "../../../src-tauri/icons/32x32.png";
  import PixelSprite from "$lib/pixel/PixelSprite.svelte";
  import { FISH_MOUTH_CLOSED } from "$lib/pixel/sprites";

  export type Page = "library" | "downloads" | "settings";

  let { page, onselect }: { page: Page; onselect: (p: Page) => void } = $props();

  const ITEMS: { id: Page; label: string }[] = [
    { id: "library", label: "Library" },
    { id: "downloads", label: "Downloads" },
    { id: "settings", label: "Settings" },
  ];

  // A faint school of gray fish drifting up and to the right behind the menu.
  // left: start position (% of the column), scale: pixel size, dur/delay: seconds (negative delay = already mid-swim).
  const GRAY = { "#": "#7A8A99", e: "#7A8A99" };
  const SCHOOL = [
    { left: 4, scale: 2, dur: 26, delay: -3 },
    { left: 52, scale: 1, dur: 34, delay: -14 },
    { left: 22, scale: 2, dur: 30, delay: -21 },
    { left: 66, scale: 2, dur: 24, delay: -8 },
    { left: 38, scale: 1, dur: 38, delay: -30 },
    { left: 10, scale: 1, dur: 32, delay: -17 },
    { left: 58, scale: 2, dur: 28, delay: -25 },
  ];
</script>

<nav class="side" aria-label="Sections">
  <div class="school" aria-hidden="true">
    {#each SCHOOL as f, i (i)}
      <div class="drifter" style="left:{f.left}%; animation-duration:{f.dur}s; animation-delay:{f.delay}s;">
        <PixelSprite grid={FISH_MOUTH_CLOSED} palette={GRAY} scale={f.scale} />
      </div>
    {/each}
  </div>
  <div class="brand"><img src={icon} width="24" height="24" alt="" /><span>Fishy Client</span></div>
  {#each ITEMS as item (item.id)}
    <button type="button" aria-current={page === item.id ? "page" : undefined} onclick={() => onselect(item.id)}>
      {item.label}
    </button>
  {/each}
</nav>

<style>
  .side {
    /* stays in view while the page scrolls */
    position: sticky;
    top: 0;
    align-self: start;
    height: 100vh;
    box-sizing: border-box;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 0;
    border-right: 2px solid rgba(79, 227, 209, 0.18);
    background: rgba(11, 15, 20, 0.86);
    backdrop-filter: blur(6px);
  }
  .side > :not(.school) {
    position: relative;
    z-index: 1;
  }
  .school {
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
  }
  .drifter {
    position: absolute;
    bottom: -48px;
    opacity: 0.22;
    animation-name: fishy-drift;
    animation-timing-function: steps(160);
    animation-iteration-count: infinite;
  }
  /* swim diagonally up-right, nose first, from below the column to above it */
  @keyframes fishy-drift {
    from {
      transform: translate(0, 0) rotate(-35deg);
    }
    to {
      transform: translate(90px, calc(-100vh - 96px)) rotate(-35deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .drifter {
      animation: none;
      transform: translate(0, -40vh) rotate(-35deg);
    }
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 18px 18px;
    font-family: var(--font-pixel);
    color: var(--aqua);
    font-size: 17px;
  }
  .brand img {
    image-rendering: pixelated;
  }
  button {
    font-family: var(--font-pixel);
    font-size: 16px;
    text-align: left;
    padding: 10px 18px;
    color: var(--text-2);
    background: none;
    border: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }
  button::before {
    content: "";
    width: 6px;
    height: 6px;
    background: currentColor;
    opacity: 0.5;
  }
  button[aria-current="page"] {
    color: var(--aqua);
    background: rgba(79, 227, 209, 0.07);
  }
  button[aria-current="page"]::before {
    opacity: 1;
  }
  button:hover {
    color: var(--text);
  }
  button:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: -2px;
  }
</style>
