<script lang="ts">
  /** The Manage box for one installed game (spec §6t). */
  import { untrack } from "svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { formatBytes } from "$lib/progress/progress";
  import { settings } from "$lib/settings/settings";
  import {
    addStartMenu,
    createShortcut,
    formatPlaytime,
    installed as installedStore,
    openGameFolder,
    removeShortcut,
    setGameOptions,
    verifying,
    verifyRepair,
    verifyStart,
    verifyStop,
    type InstalledGame,
  } from "$lib/install/installs";
  import type { Game } from "./library";

  let {
    game,
    installed,
    running,
    hasDownload,
    sharedWithEpic,
    onuninstall,
    onclose,
  }: {
    game: Game;
    installed: InstalledGame;
    running: boolean;
    hasDownload: boolean;
    sharedWithEpic: boolean;
    onuninstall: () => void;
    onclose: () => void;
  } = $props();

  const LAUNCH_MAX = 1000;
  const TOO_LONG = "Keep it under 1000 characters.";

  // The newest record (shortcuts change after Create / Remove).
  const rec = $derived($installedStore.find((g) => g.app_name === game.app_name) ?? installed);
  const shortcuts = $derived(rec.options?.shortcuts ?? []);

  // The switches start from the saved record and are saved on every change.
  const first = untrack(() => installed.options);
  let autoUpdate = $state<boolean | null>(first?.auto_update ?? null);
  let priority = $state(first?.priority ?? false);
  let launchArgs = $state(first?.launch_args ?? "");
  let launchArgsOn = $state(first?.launch_args_on ?? false);
  let savedArgs = first?.launch_args ?? "";
  let launchError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let shortcutError = $state<string | null>(null);
  let askStartMenu = $state(false);
  let verifyError = $state<string | null>(null);
  let folderError = $state<string | null>(null);
  let typing: ReturnType<typeof setTimeout> | undefined;

  const autoShown = $derived(autoUpdate ?? $settings.auto_update);
  const tooLong = (t: string) => [...t].length > LAUNCH_MAX;

  async function save() {
    if (tooLong(launchArgs)) {
      launchError = TOO_LONG;
      return;
    }
    launchError = null;
    clearTimeout(typing);
    savedArgs = launchArgs;
    saveError = await setGameOptions(game.app_name, { auto_update: autoUpdate, priority, launch_args: launchArgs, launch_args_on: launchArgsOn });
  }

  function typed(e: Event) {
    launchArgs = (e.currentTarget as HTMLTextAreaElement).value;
    clearTimeout(typing);
    if (tooLong(launchArgs)) {
      launchError = TOO_LONG;
      return;
    }
    launchError = null;
    typing = setTimeout(save, 500);
  }

  function leftBox() {
    if (tooLong(launchArgs)) launchError = TOO_LONG;
    else if (launchArgs !== savedArgs) void save();
  }

  async function create() {
    shortcutError = await createShortcut(game.app_name);
    askStartMenu = !shortcutError;
  }
  async function answerStartMenu(yes: boolean) {
    askStartMenu = false;
    if (yes) shortcutError = await addStartMenu(game.app_name);
  }
  async function remove() {
    askStartMenu = false;
    shortcutError = await removeShortcut(game.app_name);
  }

  // Verify (spec §6t): this game's progress or result, or another game's check.
  const v = $derived($verifying);
  const mine = $derived(v && v.app === game.app_name ? v : null);
  const checking = $derived(mine && "done" in mine ? mine : null);
  const result = $derived(mine && "result" in mine ? mine.result : null);
  const otherChecking = $derived(!!v && v.app !== game.app_name && "done" in v);
  const verifyReason = $derived(
    running ? "Close the game first." : hasDownload ? "Finish or cancel its download first." : otherChecking ? "Another game is being checked." : null,
  );
  const n = (x: number) => x.toLocaleString("en-US");

  async function verify() {
    verifyError = await verifyStart(game.app_name);
  }
  async function repair() {
    verifyError = await verifyRepair(game.app_name);
  }

  let box: HTMLDivElement;
  $effect(() => {
    box?.focus();
  });
  $effect(() => () => clearTimeout(typing));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div
    class="box"
    role="dialog"
    aria-modal="true"
    aria-labelledby="manage-title"
    tabindex="-1"
    bind:this={box}
    onkeydown={(e) => e.key === "Escape" && onclose()}
  >
    <header>
      <h2 id="manage-title">{game.title}</h2>
      <button class="close" aria-label="Close" onclick={onclose}>✕</button>
    </header>

    <section class="panel">
      <label class="line">
        <span class="text">
          <span class="name" id="m-auto">Auto update</span>
          <span class="hint">Updates this game by itself when a new version is out.</span>
        </span>
        <input
          type="checkbox"
          role="switch"
          aria-labelledby="m-auto"
          checked={autoShown}
          onchange={(e) => {
            autoUpdate = (e.currentTarget as HTMLInputElement).checked;
            void save();
          }}
        />
      </label>
    </section>

    <section class="panel">
      <label class="line">
        <span class="text">
          <span class="name" id="m-prio">Priority download</span>
          <span class="hint">When downloads wait in line, this game goes first.</span>
        </span>
        <input
          type="checkbox"
          role="switch"
          aria-labelledby="m-prio"
          checked={priority}
          onchange={(e) => {
            priority = (e.currentTarget as HTMLInputElement).checked;
            void save();
          }}
        />
      </label>
    </section>

    <section class="panel">
      <div class="line">
        <span class="text">
          <span class="name">Shortcut</span>
          <span class="hint">
            {#if shortcuts.length > 1}On your desktop and in the Start menu{:else if shortcuts.length === 1}On your desktop{:else}Start this game from your desktop.{/if}
          </span>
        </span>
        {#if shortcuts.length > 0}
          <PixelButton variant="ghost" onclick={remove}>Remove</PixelButton>
        {:else}
          <PixelButton variant="ghost" onclick={create}>Create</PixelButton>
        {/if}
      </div>
      {#if askStartMenu}
        <div class="ask">
          <span>Also add it to the Start menu?</span>
          <PixelButton variant="ghost" onclick={() => answerStartMenu(false)}>No</PixelButton>
          <PixelButton onclick={() => answerStartMenu(true)}>Yes</PixelButton>
        </div>
      {/if}
      {#if shortcutError}<p class="error" role="alert">{shortcutError}</p>{/if}
    </section>

    <section class="panel">
      <div class="line">
        <span class="text">
          <span class="name">Installation</span>
          <span class="hint">Size: {formatBytes(rec.install_size)}</span>
          <span class="path">{rec.install_path}</span>
        </span>
        <span class="buttons">
          <button
            class="icon"
            aria-label="Open game folder"
            title="Open game folder"
            onclick={async () => (folderError = await openGameFolder(game.app_name))}
          >
            <svg viewBox="0 0 16 16" width="18" height="18" aria-hidden="true" shape-rendering="crispEdges">
              <path d="M1 3h5l1 2h8v8H1z" fill="none" stroke="currentColor" stroke-width="1.5" />
            </svg>
          </button>
          {#if !checking}
            <PixelButton variant="ghost" disabled={!!verifyReason} title={verifyReason ?? undefined} onclick={verify}>Verify</PixelButton>
          {/if}
          <PixelButton variant="danger" disabled={running} title={running ? "Close the game first." : undefined} onclick={onuninstall}
            >Uninstall</PixelButton
          >
        </span>
      </div>
      {#if checking}
        <div class="check" role="status">
          <span>Checking files… {n(checking.done)} / {n(checking.total)}</span>
          <span class="bar"><span style="width: {checking.total ? (100 * checking.done) / checking.total : 0}%"></span></span>
          <PixelButton variant="ghost" onclick={() => verifyStop(game.app_name)}>Stop</PixelButton>
        </div>
      {:else if result?.kind === "good"}
        <p class="ok" role="status">All files are fine.</p>
      {:else if result?.kind === "bad"}
        <div class="check" role="status">
          <span>{n(result.files)} {result.files === 1 ? "file is" : "files are"} missing or damaged ({formatBytes(result.bytes)}).</span>
          <PixelButton onclick={repair}>Download them</PixelButton>
        </div>
      {:else if result?.kind === "error"}
        <p class="error" role="alert">{result.message}</p>
      {/if}
      {#if verifyError}<p class="error" role="alert">{verifyError}</p>{/if}
      {#if folderError}<p class="error" role="alert">{folderError}</p>{/if}
      {#if formatPlaytime(rec.playtime_secs)}<p class="hint">{formatPlaytime(rec.playtime_secs)}</p>{/if}
      {#if sharedWithEpic}<p class="hint">Also in Epic's launcher. Updating in one may make the other update again.</p>{/if}
    </section>

    <section class="panel">
      <label class="line">
        <span class="text">
          <span class="name" id="m-launch">Launch options</span>
          <span class="hint">Extra words added when the game starts, like -windowed.</span>
        </span>
        <input
          type="checkbox"
          role="switch"
          aria-labelledby="m-launch"
          checked={launchArgsOn}
          onchange={(e) => {
            launchArgsOn = (e.currentTarget as HTMLInputElement).checked;
            void save();
          }}
        />
      </label>
      <textarea
        aria-label="Launch options text"
        rows="2"
        spellcheck="false"
        value={launchArgs}
        oninput={typed}
        onblur={leftBox}
        class:off={!launchArgsOn}
      ></textarea>
      {#if launchError}<p class="error" role="alert">{launchError}</p>{/if}
    </section>
    {#if saveError}<p class="error" role="alert">{saveError}</p>{/if}
  </div>
</div>

<style>
  /* Starts under Fishy's title bar so its buttons stay usable (spec §6s). */
  .overlay {
    position: fixed;
    inset: var(--titlebar) 0 0 0;
    background: rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    z-index: 20;
    padding: 16px;
  }
  .box {
    background: #05080c;
    border: 2px solid var(--aqua);
    box-shadow: 6px 6px 0 rgba(79, 227, 209, 0.15);
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - var(--titlebar) - 32px);
    overflow-y: auto;
    padding: 18px 20px 20px;
    display: grid;
    gap: 10px;
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  h2 {
    margin: 0;
    font-family: var(--font-pixel);
    font-size: 20px;
    font-weight: 500;
    color: var(--aqua);
    letter-spacing: 0.04em;
  }
  .close {
    background: transparent;
    border: 0;
    color: var(--text-2);
    font-size: 16px;
    cursor: pointer;
    padding: 4px 6px;
  }
  .close:hover {
    color: var(--aqua);
  }
  .panel {
    background: var(--panel);
    border: 2px solid rgba(79, 227, 209, 0.18);
    padding: 12px 14px;
    display: grid;
    gap: 8px;
  }
  .line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  label.line {
    cursor: pointer;
  }
  .text {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .name {
    font-family: var(--font-pixel);
    font-size: 15px;
    color: var(--text);
  }
  .hint,
  .path {
    font-size: 12.5px;
    color: var(--text-2);
    margin: 0;
  }
  .path {
    overflow-wrap: anywhere;
    font-variant-numeric: tabular-nums;
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .icon {
    background: transparent;
    border: 2px solid rgba(79, 227, 209, 0.5);
    color: var(--aqua);
    padding: 5px 7px;
    display: grid;
    cursor: pointer;
  }
  .icon:hover {
    background: rgba(79, 227, 209, 0.15);
  }
  /* Square pixel switch: a block that slides, aqua when on. */
  input[role="switch"] {
    appearance: none;
    flex-shrink: 0;
    width: 40px;
    height: 20px;
    border: 2px solid var(--text-2);
    background: #000;
    position: relative;
    cursor: pointer;
    margin: 0;
  }
  input[role="switch"]::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    background: var(--text-2);
    transition: left 0.12s steps(3);
  }
  input[role="switch"]:checked {
    border-color: var(--aqua);
  }
  input[role="switch"]:checked::after {
    left: 22px;
    background: var(--aqua);
  }
  input[role="switch"]:focus-visible,
  textarea:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: 2px;
  }
  textarea {
    font-family: Consolas, "Courier New", monospace;
    font-size: 13px;
    color: var(--text);
    background: #000;
    border: 2px solid rgba(79, 227, 209, 0.3);
    padding: 8px;
    resize: vertical;
  }
  textarea.off {
    color: var(--text-2);
  }
  .ask,
  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    font-size: 13.5px;
    color: var(--text);
  }
  .ask span {
    margin-right: auto;
  }
  .bar {
    flex: 1;
    min-width: 80px;
    height: 8px;
    border: 2px solid rgba(79, 227, 209, 0.4);
    display: block;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--aqua);
  }
  .ok {
    margin: 0;
    color: var(--success);
    font-size: 13.5px;
  }
  .error {
    margin: 0;
    color: var(--error);
    font-size: 13px;
  }
</style>
