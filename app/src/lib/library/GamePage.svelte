<script lang="ts">
  import type { Snippet } from "svelte";
  import { untrack } from "svelte";
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import PixelSprite from "$lib/pixel/PixelSprite.svelte";
  import { FISH_MOUTH_CLOSED, FISH_PALETTE } from "$lib/pixel/sprites";
  import type { Game } from "./library";
  import {
    filterAchievements,
    formatRarity,
    formatUnlocked,
    loadAchievements,
    type AchievementFilter,
    type Achievements,
  } from "./achievements";

  /** A game's page (spec §6q). The buttons come from the Library, which keeps their logic. */
  let { game, actions, lines, onback }: { game: Game; actions: Snippet<[Game]>; lines: string[]; onback: () => void } = $props();

  const LOAD_ERROR = "Couldn't load achievements from Epic.";
  type Load = { status: "loading" } | { status: "error"; message: string } | { status: "ready"; data: Achievements };
  let load = $state<Load>({ status: "loading" });
  let filter = $state<AchievementFilter>("all");
  /** Only the newest request may show: a late reply for another game is dropped. */
  let request = 0;
  let shownFor = "";

  async function fetchFor(app: string) {
    const mine = ++request;
    load = { status: "loading" };
    try {
      const data = await loadAchievements(app);
      if (mine === request) load = { status: "ready", data };
    } catch (e) {
      if (mine === request) load = { status: "error", message: typeof e === "string" && e ? e : LOAD_ERROR };
    }
  }

  // A new game: the filter goes back to All and its list is asked for (a refreshed copy of
  // the same game doesn't ask again).
  $effect(() => {
    const app = game.app_name;
    if (app === shownFor) return;
    shownFor = app;
    untrack(() => {
      filter = "all";
      fetchFor(app);
    });
  });

  const shown = $derived(load.status === "ready" ? filterAchievements(load.data.items, filter) : []);
  const percent = $derived(load.status === "ready" && load.data.total > 0 ? (load.data.unlocked / load.data.total) * 100 : 0);
  const FILTERS: [AchievementFilter, string][] = [
    ["all", "All"],
    ["unlocked", "Unlocked"],
    ["locked", "Locked"],
  ];
</script>

<section class="game-page">
  <button class="back" onclick={onback}>← Library</button>

  <div class="top">
    <div class="cover">
      {#if game.cover_url}
        <img src={game.cover_url} alt="{game.title} cover art" referrerpolicy="no-referrer" />
      {:else}
        <div class="placeholder" role="img" aria-label="No cover art for {game.title}">
          <PixelSprite grid={FISH_MOUTH_CLOSED} palette={FISH_PALETTE} scale={4} />
        </div>
      {/if}
    </div>
    <div class="info">
      <h2>{game.title}</h2>
      {@render actions(game)}
      {#each lines as line (line)}<p class="line">{line}</p>{/each}
    </div>
  </div>

  <section class="achievements" aria-labelledby="achievements-heading">
    <h3 id="achievements-heading">Achievements</h3>
    {#if load.status === "loading"}
      <div class="center" role="status">
        <FishLoader size="small" />
        <p>Loading achievements…</p>
      </div>
    {:else if load.status === "error"}
      <div class="center">
        <p class="error" role="alert">{load.message}</p>
        <PixelButton onclick={() => fetchFor(game.app_name)}>Try again</PixelButton>
      </div>
    {:else if load.data.total === 0}
      <p class="none">This game has no Epic achievements.</p>
    {:else}
      <div class="progress">
        <p class="count">{load.data.unlocked} / {load.data.total} unlocked</p>
        <div class="bar" aria-hidden="true"><div class="fill" style="width: {percent}%"></div></div>
      </div>
      <div class="tabs" role="tablist" aria-label="Which achievements">
        {#each FILTERS as [value, label] (value)}
          <button role="tab" aria-selected={filter === value} class:active={filter === value} onclick={() => (filter = value)}>{label}</button>
        {/each}
      </div>
      {#if shown.length === 0}
        <p class="none">{filter === "unlocked" ? "None unlocked yet." : "You have them all!"}</p>
      {:else}
        <ul class="list">
          {#each shown as a (a.name)}
            <li class:locked={!a.unlocked}>
              {#if a.icon_url}
                <img class="icon" src={a.icon_url} alt="" width="48" height="48" loading="lazy" referrerpolicy="no-referrer" />
              {:else}
                <div class="icon"></div>
              {/if}
              <div class="text">
                <span class="name">{a.title}</span>
                {#if a.description}<span class="desc">{a.description}</span>{/if}
              </div>
              <span class="when">{a.unlocked ? formatUnlocked(a.unlocked_at) : (formatRarity(a.rarity) ?? "")}</span>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>
</section>

<style>
  .game-page {
    display: flex;
    flex-direction: column;
    gap: 18px;
    width: 100%;
  }
  .back {
    align-self: flex-start;
    font-family: var(--font-pixel);
    font-size: 13px;
    color: var(--aqua);
    background: none;
    border: none;
    padding: 2px 0;
    cursor: pointer;
  }
  .back:hover {
    text-decoration: underline;
  }
  .back:focus-visible,
  .tabs button:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: 2px;
  }
  .top {
    display: flex;
    gap: 20px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .cover {
    width: 200px;
    flex: none;
  }
  .cover img,
  .placeholder {
    width: 100%;
    aspect-ratio: 3 / 4;
    object-fit: cover;
    border: 2px solid rgba(79, 227, 209, 0.18);
    background: #0b0f14;
    display: block;
  }
  .placeholder {
    display: grid;
    place-items: center;
  }
  .info {
    display: grid;
    gap: 10px;
    min-width: 0;
    flex: 1;
    align-content: start;
  }
  h2 {
    margin: 0;
    font-family: var(--font-pixel);
    font-size: 22px;
    overflow-wrap: anywhere;
  }
  .line {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .achievements {
    display: grid;
    gap: 12px;
  }
  h3 {
    margin: 0;
    font-family: var(--font-pixel);
    font-size: 16px;
  }
  .center {
    display: grid;
    justify-items: start;
    gap: 8px;
  }
  .error {
    color: var(--error);
  }
  .none {
    color: var(--text-2);
    margin: 0;
  }
  .progress {
    display: grid;
    gap: 6px;
    max-width: 420px;
  }
  .count {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    height: 10px;
    border: 2px solid rgba(79, 227, 209, 0.5);
    background: #0b0f14;
  }
  .fill {
    height: 100%;
    background: var(--aqua);
  }
  .tabs {
    display: flex;
    gap: 6px;
  }
  .tabs button {
    font: inherit;
    font-size: 14px;
    color: var(--text-2);
    background: transparent;
    border: 2px solid rgba(79, 227, 209, 0.25);
    padding: 6px 12px;
    cursor: pointer;
  }
  .tabs button.active {
    color: #000;
    background: var(--aqua);
    border-color: var(--aqua);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    border: 2px solid rgba(79, 227, 209, 0.18);
    background: rgba(11, 15, 20, 0.6);
  }
  .icon {
    width: 48px;
    height: 48px;
    flex: none;
    object-fit: cover;
    background: #0b0f14;
  }
  .text {
    display: grid;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .desc {
    font-size: 13px;
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .when {
    font-size: 12px;
    color: var(--text-2);
    text-align: right;
    flex: none;
    max-width: 40%;
  }
  /* Locked: greyed (spec §6q). */
  .locked .icon {
    filter: grayscale(1) brightness(0.6);
  }
  .locked .name,
  .locked .desc {
    opacity: 0.6;
  }
</style>
