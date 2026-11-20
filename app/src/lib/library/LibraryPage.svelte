<script lang="ts">
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import PixelSprite from "$lib/pixel/PixelSprite.svelte";
  import { FISH_MOUTH_CLOSED, FISH_PALETTE } from "$lib/pixel/sprites";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import InstallDialog from "$lib/install/InstallDialog.svelte";
  import { downloads, installed, statusFor, uninstall, type GameStatus } from "$lib/install/installs";
  import { library, loadLibrary, refreshLibrary, filterGames, type Game } from "./library";

  let { name, offline = false }: { name: string; offline?: boolean } = $props();
  let query = $state("");
  let tab = $state<"all" | "installed">("all");
  let installing = $state<Game | null>(null);
  let menuFor = $state<string | null>(null);
  let confirmUninstall = $state<Game | null>(null);
  let cardError = $state<{ app: string; message: string } | null>(null);

  const status = (g: Game): GameStatus => statusFor(g.app_name, $downloads, $installed);
  const installedCount = $derived($library.games.filter((g) => $installed.some((i) => i.app_name === g.app_name)).length);
  const shown = $derived(
    filterGames($library.games, query).filter((g) => tab === "all" || $installed.some((i) => i.app_name === g.app_name)),
  );
  const LABELS: Partial<Record<GameStatus, string>> = {
    downloading: "Downloading…",
    queued: "Queued",
    paused: "Paused",
    error: "Download stopped",
  };

  async function doUninstall(game: Game) {
    confirmUninstall = null;
    cardError = null;
    const err = await uninstall(game.app_name);
    if (err) cardError = { app: game.app_name, message: err };
  }

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
  {:else}
    <div class="tabs" role="tablist" aria-label="Which games">
      <button role="tab" aria-selected={tab === "all"} class:active={tab === "all"} onclick={() => (tab = "all")}>All</button>
      <button role="tab" aria-selected={tab === "installed"} class:active={tab === "installed"} onclick={() => (tab = "installed")}>
        Installed ({installedCount})
      </button>
    </div>
    {#if shown.length === 0}
      <p class="none">{query.trim() ? `No games match "${query.trim()}".` : "No games installed yet."}</p>
    {:else}
      <ul class="grid">
        {#each shown as game (game.catalog_item_id)}
          {@const s = status(game)}
          <li class="card">
            <div class="cover">
              {#if game.cover_url}
                <img src={game.cover_url} alt="{game.title} cover art" loading="lazy" referrerpolicy="no-referrer" />
              {:else}
                <div class="placeholder" role="img" aria-label="No cover art for {game.title}">
                  <PixelSprite grid={FISH_MOUTH_CLOSED} palette={FISH_PALETTE} scale={3} />
                </div>
              {/if}
              {#if s === "installed"}<span class="badge">Installed</span>{/if}
            </div>
            <span class="title">{game.title}</span>
            <div class="row">
              {#if s === "none"}
                <button class="install" aria-label="Install {game.title}" onclick={() => (installing = game)}>Install</button>
              {:else if s === "installed"}
                <div class="more">
                  <button
                    class="dots"
                    aria-label="More options for {game.title}"
                    aria-haspopup="menu"
                    aria-expanded={menuFor === game.app_name}
                    onclick={() => (menuFor = menuFor === game.app_name ? null : game.app_name)}>⋯</button
                  >
                  {#if menuFor === game.app_name}
                    <div class="menu" role="menu">
                      <button
                        role="menuitem"
                        onclick={() => {
                          menuFor = null;
                          confirmUninstall = game;
                        }}>Uninstall</button
                      >
                    </div>
                  {/if}
                </div>
              {:else}
                <span class="state" class:bad={s === "error"}>{LABELS[s]}</span>
              {/if}
            </div>
            {#if cardError?.app === game.app_name}<p class="error small" role="alert">{cardError.message}</p>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

{#if installing}
  <InstallDialog game={installing} onclose={() => (installing = null)} />
{/if}
{#if confirmUninstall}
  {@const g = confirmUninstall}
  <ConfirmDialog
    message="Uninstall {g.title}? Its game files will be deleted."
    confirmLabel="Uninstall"
    onconfirm={() => doUninstall(g)}
    oncancel={() => (confirmUninstall = null)}
  />
{/if}

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
  .tabs button:focus-visible,
  .row button:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: 2px;
  }
  .cover {
    position: relative;
  }
  .badge {
    position: absolute;
    left: 6px;
    top: 6px;
    background: var(--aqua);
    color: #000;
    font-family: var(--font-pixel);
    font-size: 12px;
    padding: 3px 6px;
  }
  .row {
    display: flex;
    align-items: center;
    min-height: 30px;
  }
  .install,
  .dots {
    font-family: var(--font-pixel);
    font-size: 13px;
    color: var(--aqua);
    background: transparent;
    border: 2px solid var(--aqua);
    padding: 5px 10px;
    cursor: pointer;
  }
  .install:hover,
  .dots:hover {
    background: rgba(79, 227, 209, 0.15);
  }
  .more {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 5;
    background: #0b0f14;
    border: 2px solid rgba(79, 227, 209, 0.5);
    display: grid;
    min-width: 130px;
  }
  .menu button {
    font: inherit;
    font-size: 14px;
    color: var(--text);
    background: transparent;
    border: 0;
    padding: 8px 12px;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: rgba(79, 227, 209, 0.15);
    outline: none;
  }
  .state {
    font-size: 13px;
    color: var(--orange);
  }
  .state.bad {
    color: var(--error);
  }
  .small {
    font-size: 13px;
    margin: 0;
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
