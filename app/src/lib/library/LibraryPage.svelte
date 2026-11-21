<script lang="ts">
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import PixelSprite from "$lib/pixel/PixelSprite.svelte";
  import { FISH_MOUTH_CLOSED, FISH_PALETTE } from "$lib/pixel/sprites";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import InstallDialog from "$lib/install/InstallDialog.svelte";
  import OutdatedPlayDialog from "$lib/install/OutdatedPlayDialog.svelte";
  import {
    downloads,
    hasUpdate,
    installed,
    play,
    refreshInstalled,
    startUpdate,
    statusFor,
    uninstall,
    type GameStatus,
    type PlayError,
  } from "$lib/install/installs";
  import { settings, saveSettings } from "$lib/settings/settings";
  import { retry as recheckSignIn } from "$lib/auth/auth";
  import { library, loadLibrary, refreshLibrary, filterGames, type Game } from "./library";

  let { name, offline = false }: { name: string; offline?: boolean } = $props();
  let query = $state("");
  let tab = $state<"all" | "installed" | "elsewhere">("all");
  let installing = $state<Game | null>(null);
  let menuFor = $state<string | null>(null);
  let confirmUninstall = $state<Game | null>(null);
  let cardError = $state<{ app: string; message: string } | null>(null);
  let starting = $state<string[]>([]);
  /** The Refresh button: Epic's library (with the update check) and the installed list again. */
  async function refreshAll() {
    await Promise.all([refreshLibrary(), refreshInstalled()]);
  }
  const refreshing = $derived($library.updating || $library.status === "loading");

  /** The game whose Play is waiting for the "update waiting" answer (spec §6e). */
  let outdatedFor = $state<Game | null>(null);
  let playError = $state<({ app: string } & PlayError) | null>(null);
  /** How long the button says "Starting…" (the game isn't tracked after that, spec §6d). */
  const STARTING_MS = 5000;

  const status = (g: Game): GameStatus => statusFor(g.app_name, $downloads, $installed);
  const installedCount = $derived($library.games.filter((g) => $installed.some((i) => i.app_name === g.app_name)).length);
  // Games Epic has no Windows download for (mostly phone games) live in their own tab.
  const windowsGames = $derived($library.games.filter((g) => g.installable));
  const elsewhere = $derived($library.games.filter((g) => !g.installable));
  const shown = $derived(
    filterGames(tab === "elsewhere" ? elsewhere : windowsGames, query).filter(
      (g) => tab !== "installed" || $installed.some((i) => i.app_name === g.app_name),
    ),
  );
  // If a refresh empties the "Not on Windows" tab while it's open, go back to All.
  $effect(() => {
    if (tab === "elsewhere" && elsewhere.length === 0) tab = "all";
  });
  const outdated = (g: Game) => hasUpdate(g.app_name, $installed, $library.latestVersions);
  const jobOf = (g: Game) => $downloads.find((d) => d.app_name === g.app_name);
  const UPDATE_LABELS: Partial<Record<GameStatus, string>> = {
    downloading: "Updating…",
    queued: "Updating…",
    paused: "Update paused",
    error: "Update stopped",
  };
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

  /** Play, but first ask when an update is waiting (unless the user turned that off). */
  function askPlay(game: Game) {
    if (outdated(game) && $settings.warn_outdated_play) outdatedFor = game;
    else doPlay(game);
  }

  async function answerOutdated(game: Game, action: "play" | "update", dontShowAgain: boolean) {
    outdatedFor = null;
    if (dontShowAgain) await saveSettings({ warn_outdated_play: false });
    if (action === "play") await doPlay(game);
    else await doUpdate(game);
  }

  async function doUpdate(game: Game) {
    cardError = null;
    const err = await startUpdate(game.app_name);
    if (err) cardError = { app: game.app_name, message: err };
  }

  async function doPlay(game: Game, offline = false) {
    if (starting.includes(game.app_name)) return;
    playError = null;
    starting = [...starting, game.app_name];
    const err = await play(game.app_name, offline);
    if (err) {
      starting = starting.filter((a) => a !== game.app_name);
      if (err.signed_out) await recheckSignIn(); // the shell then shows the login screen
      else playError = { app: game.app_name, ...err };
      return;
    }
    setTimeout(() => (starting = starting.filter((a) => a !== game.app_name)), STARTING_MS);
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
    <div class="tools">
      {#if $library.status === "ready" && $library.games.length > 0}
        <label class="sr-only" for="library-search">Search your library</label>
        <input id="library-search" type="search" placeholder="Search your library" bind:value={query} autocomplete="off" />
        <span class="count">{windowsGames.length} {windowsGames.length === 1 ? "game" : "games"}</span>
      {/if}
      {#if !offline && $library.status !== "offline"}
        <button class="refresh" aria-label="Refresh library" title="Check Epic for new games and updates" disabled={refreshing} onclick={refreshAll}
          >Refresh</button
        >
      {/if}
    </div>
  </header>

  {#if $library.status === "ready" && $library.error}
    <div class="banner" role="alert">
      <span>{$library.error}</span>
      <PixelButton variant="ghost" onclick={refreshLibrary}>Try again</PixelButton>
    </div>
  {:else if $library.updating}
    <p class="updating" role="status">Refreshing…</p>
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
      {#if elsewhere.length > 0}
        <button role="tab" aria-selected={tab === "elsewhere"} class:active={tab === "elsewhere"} onclick={() => (tab = "elsewhere")}>
          Not on Windows ({elsewhere.length})
        </button>
      {/if}
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
              {#if s === "installed"}
                <span class="badge" class:update={outdated(game)}>{outdated(game) ? "Update available" : "Installed"}</span>
              {/if}
            </div>
            <span class="title">{game.title}</span>
            <div class="row">
              {#if s === "none" && !game.installable}
                <!-- nothing to install: one explanation sits under the grid -->
              {:else if s === "none"}
                <button class="install" aria-label="Install {game.title}" onclick={() => (installing = game)}>Install</button>
              {:else if s === "installed"}
                {#if starting.includes(game.app_name)}
                  <button class="play" aria-label="Starting {game.title}" disabled>Starting…</button>
                {:else}
                  <button class="play" aria-label="Play {game.title}" onclick={() => askPlay(game)}>Play</button>
                {/if}
                {#if outdated(game)}
                  <button class="update" aria-label="Update {game.title}" onclick={() => doUpdate(game)}>Update</button>
                {/if}
                {@render moreMenu(game)}
              {:else}
                {@const updating = !!jobOf(game)?.update}
                <span class="state" class:bad={s === "error"}>{updating ? UPDATE_LABELS[s] : LABELS[s]}</span>
                {#if updating}{@render moreMenu(game)}{/if}
              {/if}
            </div>
            {#if cardError?.app === game.app_name}<p class="error small" role="alert">{cardError.message}</p>{/if}
            {#if playError?.app === game.app_name}
              <div class="play-error" role="alert">
                <p class="error small">{playError.message}</p>
                <div class="actions">
                  <button class="link" onclick={() => doPlay(game)}>Try again</button>
                  {#if playError.can_offline}<button class="link" onclick={() => doPlay(game, true)}>Play offline</button>{/if}
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if tab === "elsewhere"}
        <p class="explain">
          Epic has no Windows download for these games. They're in your Epic account, but most are phone games from Epic's mobile
          store, which shares one library with your PC games. To play one on this PC, you'd need its PC version.
        </p>
      {/if}
    {/if}
  {/if}
</section>

{#snippet moreMenu(game: Game)}
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
{/snippet}

{#if outdatedFor}
  {@const g = outdatedFor}
  <OutdatedPlayDialog
    title={g.title}
    onplay={(dontShow) => answerOutdated(g, "play", dontShow)}
    onupdate={(dontShow) => answerOutdated(g, "update", dontShow)}
    oncancel={() => (outdatedFor = null)}
  />
{/if}

{#if installing}
  <InstallDialog game={installing} onclose={() => (installing = null)} />
{/if}
{#if confirmUninstall}
  <ConfirmDialog
    message="Uninstall {confirmUninstall.title}? Its game files will be deleted."
    confirmLabel="Uninstall"
    onconfirm={() => confirmUninstall && doUninstall(confirmUninstall)}
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
  .refresh {
    font-family: var(--font-pixel);
    font-size: 13px;
    color: var(--aqua);
    background: transparent;
    border: 2px solid var(--aqua);
    padding: 5px 10px;
    cursor: pointer;
  }
  .refresh:hover:not(:disabled) {
    background: rgba(79, 227, 209, 0.15);
  }
  .refresh:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .refresh:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: 2px;
  }
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
  .explain {
    color: var(--text-2);
    font-size: 14px;
    max-width: 640px;
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
  .play,
  .update,
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
  .play:hover:not(:disabled),
  .dots:hover {
    background: rgba(79, 227, 209, 0.15);
  }
  .update {
    margin-right: 8px;
  }
  .badge.update {
    background: var(--orange);
  }
  .play {
    margin-right: 8px;
    color: var(--orange);
    border-color: var(--orange);
  }
  .play:disabled {
    opacity: 0.7;
    cursor: default;
  }
  .play-error .actions {
    display: flex;
    gap: 12px;
  }
  .link {
    font-family: var(--font-pixel);
    font-size: 12px;
    color: var(--aqua);
    background: none;
    border: none;
    padding: 2px 0;
    text-decoration: underline;
    cursor: pointer;
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
