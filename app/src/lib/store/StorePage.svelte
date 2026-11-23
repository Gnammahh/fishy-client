<script lang="ts">
  import { onMount } from "svelte";
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import PixelSprite from "$lib/pixel/PixelSprite.svelte";
  import { FISH_MOUTH_CLOSED, FISH_PALETTE } from "$lib/pixel/sprites";
  import { message } from "$lib/install/installs";
  import { loadFreeGames, openStore, shortDate, type FreeGame, type FreeGames } from "./store";

  let games = $state<FreeGames | null>(null);
  let error = $state("");
  let openError = $state("");

  async function load(force: boolean) {
    error = "";
    games = null;
    try {
      games = await loadFreeGames(force);
    } catch (e) {
      error = message(e);
    }
  }

  async function open(url?: string) {
    openError = "";
    try {
      await openStore(url);
    } catch (e) {
      openError = message(e);
    }
  }

  onMount(() => {
    load(false);
  });
</script>

{#snippet card(g: FreeGame, soon: boolean)}
  <li class="card">
    <div class="cover">
      {#if g.image}
        <img src={g.image} alt="{g.title} cover art" loading="lazy" referrerpolicy="no-referrer" />
      {:else}
        <div class="placeholder" role="img" aria-label="No cover art for {g.title}">
          <PixelSprite grid={FISH_MOUTH_CLOSED} palette={FISH_PALETTE} scale={3} />
        </div>
      {/if}
    </div>
    <span class="title">{g.title}</span>
    <span class="when">{soon ? "Free from" : "Free until"} {shortDate(g.date)}</span>
    {#if g.owned}
      <span class="owned">In your Library</span>
    {:else}
      <PixelButton variant={soon ? "ghost" : "primary"} label="{soon ? 'View' : 'Get'} {g.title} on Epic" onclick={() => open(g.url)}
        >{soon ? "View on Epic" : "Get on Epic"}</PixelButton
      >
    {/if}
  </li>
{/snippet}

<section class="store">
  <div class="head">
    <h2>Store</h2>
    <PixelButton onclick={() => open()}>Open Epic's store</PixelButton>
  </div>
  {#if openError}<p class="error" role="alert">{openError}</p>{/if}

  {#if error}
    <p class="error" role="alert">{error}</p>
    <div><PixelButton variant="ghost" onclick={() => load(true)}>Try again</PixelButton></div>
  {:else if !games}
    <div class="loading" role="status"><FishLoader size="small" /><span>Looking for free games…</span></div>
  {:else}
    <h3>Free now</h3>
    {#if games.now.length === 0}<p class="empty">Nothing right now.</p>{:else}
      <ul class="grid">{#each games.now as g (g.url + g.title)}{@render card(g, false)}{/each}</ul>
    {/if}
    <h3>Free next</h3>
    {#if games.next.length === 0}<p class="empty">Nothing right now.</p>{:else}
      <ul class="grid">{#each games.next as g (g.url + g.title)}{@render card(g, true)}{/each}</ul>
    {/if}
    <p class="hint">Fishy Client opens Epic's page; you get the game there yourself.</p>
  {/if}
</section>

<style>
  .store {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h2 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 28px;
    margin: 0;
  }
  h3 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 18px;
    margin: 8px 0 0;
  }
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 20px;
  }
  .card {
    display: grid;
    gap: 6px;
    align-content: start;
    min-width: 0;
  }
  .cover img,
  .placeholder {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    display: grid;
    place-items: center;
    background: var(--panel);
    border: 2px solid rgba(79, 227, 209, 0.25);
  }
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    font-size: 13px;
    color: var(--orange);
  }
  .owned {
    font-size: 13px;
    color: var(--aqua);
  }
  .empty,
  .hint {
    margin: 0;
    color: var(--text-2);
  }
  .hint {
    font-size: 13px;
  }
  .error {
    margin: 0;
    color: var(--error);
  }
  .loading {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-2);
  }
</style>
