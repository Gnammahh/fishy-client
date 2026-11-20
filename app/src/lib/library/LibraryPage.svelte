<script lang="ts">
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import PixelSprite from "$lib/pixel/PixelSprite.svelte";
  import { FISH_MOUTH_CLOSED, FISH_PALETTE } from "$lib/pixel/sprites";
  import { library, loadLibrary, refreshLibrary, filterGames } from "./library";

  let { name, offline = false }: { name: string; offline?: boolean } = $props();
  let query = $state("");
  const shown = $derived(filterGames($library.games, query));

  // Load on open, and again whenever we come back online (offline → online).
  let loadedOffline: boolean | undefined;
  $effect(() => {
    if (offline !== loadedOffline) {
      loadedOffline = offline;
      loadLibrary(offline);
    }
  });
</script>

<section class="library">
  <header>
    <div>
      <h2>Library</h2>
      <p class="who">Signed in as <strong>{name}</strong>.</p>
    </div>
    {#if $library.status === "ready" && $library.games.length > 0}
      <div class="tools">
        <label class="sr-only" for="library-search">Search your library</label>
        <input id="library-search" type="search" placeholder="Search your library" bind:value={query} autocomplete="off" />
        <span class="count">{$library.games.length} {$library.games.length === 1 ? "game" : "games"}</span>
      </div>
    {/if}
  </header>

  {#if $library.status === "ready" && $library.error}
    <div class="banner" role="alert">
      <span>{$library.error}</span>
      <PixelButton variant="ghost" onclick={refreshLibrary}>Try again</PixelButton>
    </div>
  {:else if $library.updating}
    <p class="updating" role="status">Updating…</p>
  {/if}

  {#if $library.status === "loading"}
    <div class="center" role="status">
      <FishLoader size="medium" />
      <p>Fetching your library…</p>
    </div>
  {:else if $library.status === "offline"}
    <div class="center">
      <p>Your library shows up here once Fishy Client can reach Epic.</p>
    </div>
  {:else if $library.status === "error"}
    <div class="center">
      <p class="error" role="alert">{$library.error}</p>
      <PixelButton onclick={refreshLibrary}>Try again</PixelButton>
    </div>
  {:else if $library.games.length === 0}
    <div class="center">
      <PixelSprite grid={FISH_MOUTH_CLOSED} palette={FISH_PALETTE} scale={4} vflip />
      <p>No fish in this pond yet. Games you own on Epic show up here.</p>
    </div>
  {:else if shown.length === 0}
    <p class="none">No games match "{query.trim()}".</p>
  {:else}
    <ul class="grid">
      {#each shown as game (game.catalog_item_id)}
        <li class="card">
          {#if game.cover_url}
            <img src={game.cover_url} alt="{game.title} cover art" loading="lazy" referrerpolicy="no-referrer" />
          {:else}
            <div class="placeholder" role="img" aria-label="No cover art for {game.title}">
              <PixelSprite grid={FISH_MOUTH_CLOSED} palette={FISH_PALETTE} scale={3} />
            </div>
          {/if}
          <span class="title">{game.title}</span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .library {
    display: flex;
    flex-direction: column;
    gap: 18px;
    width: 100%;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: 14px;
  }
  h2 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 28px;
    margin: 0;
  }
  .who {
    margin: 4px 0 0;
    color: var(--text-2);
  }
  strong {
    color: var(--aqua);
    font-weight: 600;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  input[type="search"] {
    width: 240px;
    max-width: 50vw;
    background: rgba(0, 0, 0, 0.6);
    color: var(--text);
    border: 2px solid rgba(79, 227, 209, 0.35);
    padding: 8px 10px;
    font: inherit;
    font-size: 14px;
  }
  input[type="search"]:focus-visible {
    outline: none;
    border-color: var(--orange);
  }
  .count,
  .updating {
    color: var(--text-2);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    margin: 0;
  }
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    padding: 10px 14px;
    border: 2px solid var(--orange);
    background: rgba(11, 15, 20, 0.9);
    color: var(--orange);
  }
  .center {
    display: grid;
    justify-items: center;
    gap: 14px;
    padding: 48px 0;
    text-align: center;
    color: var(--text-2);
  }
  .center p {
    margin: 0;
  }
  .error {
    color: var(--error);
  }
  .none {
    color: var(--text-2);
    margin: 0;
  }
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 20px;
  }
  .card {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .card img,
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
  .title {
    font-size: 14px;
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
