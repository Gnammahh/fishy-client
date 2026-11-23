<script lang="ts">
  import FishProgressBar from "$lib/progress/FishProgressBar.svelte";
  import { formatBytes, percent } from "$lib/progress/progress";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { downloads, installed, pause, resume, cancel, startNow, type DownloadItem } from "./installs";
  import { settings } from "$lib/settings/settings";

  /** Opens the Settings page (the speed limit lives there). */
  let { onsettings }: { onsettings?: () => void } = $props();

  const GENERIC_ERROR = "Download stopped: the connection to the download server dropped. Check your internet, then press Retry.";

  // Up to two download at once (Start now → Download both, spec §6t); else the first stopped one.
  const running = $derived($downloads.filter((d) => d.state === "downloading"));
  const currents = $derived(running.length > 0 ? running : $downloads.filter((d) => d.state !== "queued").slice(0, 1));
  const queued = $derived($downloads.filter((d) => d.state === "queued"));
  const others = $derived($downloads.filter((d) => !currents.includes(d) && d.state !== "queued"));

  /** The waiting game whose Start now asks "pause the other one, or both?" */
  let startFor = $state<DownloadItem | null>(null);
  let startError = $state("");
  const TWO_RUNNING = "Two games are already downloading.";

  async function start(d: DownloadItem, mode: "pause" | "both") {
    startFor = null;
    startError = (await startNow(d.app_name, mode)) ?? "";
  }
  function askStart(d: DownloadItem) {
    // Nothing downloading: it just starts; the mode doesn't matter then.
    if (running.length === 0) void start(d, "pause");
    else startFor = d;
  }

  /** app_name of the download waiting for "Delete files?" */
  let confirmCancel = $state<string | null>(null);
  let finished = $state<string[]>([]);
  let cancelError = $state("");

  async function confirmDelete() {
    const appName = confirmCancel;
    confirmCancel = null;
    if (!appName) return;
    cancelError = (await cancel(appName)) ?? "";
  }

  // A game that leaves the list and shows up as installed has finished; say so for this session.
  let previous: DownloadItem[] = [];
  $effect(() => {
    const now = $downloads;
    const ins = $installed;
    for (const old of previous) {
      const gone = !now.some((d) => d.app_name === old.app_name);
      if (gone && ins.some((g) => g.app_name === old.app_name) && !finished.includes(old.title)) {
        finished = [...finished, old.title];
      }
    }
    // Installed may arrive after the list changed, so remember games until they're installed.
    previous = [...now, ...previous.filter((p) => !now.some((d) => d.app_name === p.app_name) && !ins.some((g) => g.app_name === p.app_name))];
  });

  const barState = (d: DownloadItem) => (d.state === "queued" ? "paused" : d.state);
</script>

<section class="downloads">
  <h2>Downloads</h2>

  {#if $settings.speed_limit_mbps !== null}
    <p class="limit">
      Limited to {$settings.speed_limit_mbps} MB/s ·
      <button class="link" onclick={() => onsettings?.()}>Change in Settings</button>
    </p>
  {/if}

  {#each finished as title (title)}
    <p class="note" role="status">Finished: {title}. It's in your Library.</p>
  {/each}

  {#if cancelError}
    <p class="error" role="alert">{cancelError}</p>
  {/if}
  {#if startError}
    <p class="error" role="alert">{startError}</p>
  {/if}

  {#if currents.length === 0 && queued.length === 0}
    <p class="empty">Nothing downloading. Pick a game in your Library and press Install.</p>
  {/if}

  {#each currents as current (current.app_name)}
    <article class="current">
      <h3>{current.title}{#if current.update}<span class="tag">Updating…</span>{/if}</h3>
      <FishProgressBar
        downloaded={current.downloaded}
        total={current.total}
        bytesPerSec={current.bytes_per_sec}
        state={barState(current)}
        onretry={() => resume(current.app_name)}
      />
      {#if current.state === "error" && current.error && current.error !== GENERIC_ERROR}
        <p class="error" role="alert">{current.error}</p>
      {/if}
      <div class="actions">
        {#if current.state === "downloading"}
          <PixelButton variant="ghost" onclick={() => pause(current.app_name)}>Pause</PixelButton>
        {:else if current.state === "paused"}
          <PixelButton onclick={() => resume(current.app_name)}>Resume</PixelButton>
        {/if}
        {#if !current.update}
          <PixelButton variant="ghost" onclick={() => (confirmCancel = current.app_name)}>Cancel</PixelButton>
        {/if}
      </div>
      {#if current.update}<p class="note small">Updates can be paused but not cancelled.</p>{/if}
    </article>
  {/each}

  {#if queued.length > 0}
    <h3 class="sub">Up next</h3>
    <ul class="list">
      {#each queued as d (d.app_name)}
        <li>
          <span class="name">{d.title}</span>
          <span class="state">Queued</span>
          <PixelButton
            variant="ghost"
            label="Start {d.title} now"
            disabled={running.length >= 2}
            title={running.length >= 2 ? TWO_RUNNING : undefined}
            onclick={() => askStart(d)}>Start now</PixelButton
          >
          {#if !d.update}
            <button class="remove" aria-label="Remove {d.title} from the queue" onclick={() => (confirmCancel = d.app_name)}>×</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if others.length > 0}
    <h3 class="sub">Paused</h3>
    <ul class="list">
      {#each others as d (d.app_name)}
        <li>
          <span class="name">{d.title}</span>
          <span class="state">{d.state === "error" ? "Stopped" : `Paused at ${percent(d.downloaded, d.total)}% · ${formatBytes(d.downloaded)}`}</span>
          <PixelButton variant="ghost" onclick={() => resume(d.app_name)}>{d.state === "error" ? "Retry" : "Resume"}</PixelButton>
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if startFor}
  {@const d = startFor}
  {@const other = running[0]}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onclick={(e) => e.target === e.currentTarget && (startFor = null)}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="ask" role="dialog" aria-modal="true" aria-label="Start {d.title} now" tabindex="-1" onkeydown={(e) => e.key === "Escape" && (startFor = null)}>
      <p>{other ? `${other.title} is downloading.` : ""} Start {d.title} now?</p>
      <div class="ask-actions">
        <PixelButton variant="ghost" onclick={() => (startFor = null)}>Not now</PixelButton>
        {#if other}<PixelButton variant="ghost" onclick={() => start(d, "both")}>Download both</PixelButton>{/if}
        <PixelButton onclick={() => start(d, "pause")}>{other ? `Pause ${other.title}` : "Start"}</PixelButton>
      </div>
      <p class="note small">Downloading both shares your internet speed, so each one takes longer.</p>
    </div>
  </div>
{/if}

{#if confirmCancel}
  <ConfirmDialog
    message="Stop and delete the downloaded files?"
    confirmLabel="Delete files"
    onconfirm={confirmDelete}
    oncancel={() => (confirmCancel = null)}
  />
{/if}

<style>
  /* Start now's question; starts under Fishy's title bar (spec §6s). */
  .overlay {
    position: fixed;
    inset: var(--titlebar) 0 0 0;
    background: rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    z-index: 20;
    padding: 16px;
  }
  .ask {
    background: #05080c;
    border: 2px solid var(--aqua);
    padding: 18px 20px;
    display: grid;
    gap: 14px;
    max-width: min(480px, calc(100vw - 32px));
    outline: none;
    max-height: calc(100vh - var(--titlebar) - 32px);
    overflow-y: auto;
  }
  .ask p {
    margin: 0;
    color: var(--text);
  }
  .ask .note {
    color: var(--text-2);
  }
  .ask-actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 10px;
  }
  .downloads {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
    max-width: 820px;
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
    margin: 0;
  }
  .tag {
    margin-left: 10px;
    font-size: 13px;
    color: var(--orange);
  }
  .small {
    font-size: 13px;
    margin: 0;
  }
  .sub {
    font-size: 15px;
    color: var(--text-2);
    margin-top: 8px;
  }
  .current {
    display: grid;
    gap: 12px;
    padding: 18px;
    border: 2px solid rgba(79, 227, 209, 0.25);
    background: rgba(11, 15, 20, 0.85);
  }
  .actions {
    display: flex;
    gap: 10px;
  }
  .empty,
  .note {
    margin: 0;
    color: var(--text-2);
  }
  .note {
    color: var(--aqua);
  }
  .error {
    margin: 0;
    color: var(--error);
  }
  .limit {
    margin: 0;
    font-size: 14px;
    color: var(--orange);
  }
  .link {
    font: inherit;
    color: var(--aqua);
    background: none;
    border: none;
    padding: 0;
    text-decoration: underline;
    cursor: pointer;
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
    gap: 14px;
    padding: 10px 14px;
    background: rgba(11, 15, 20, 0.85);
    border: 2px solid rgba(79, 227, 209, 0.15);
  }
  .name {
    flex: 1;
    font-weight: 600;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .state {
    color: var(--text-2);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .remove {
    font: inherit;
    font-size: 18px;
    line-height: 1;
    color: var(--text-2);
    background: transparent;
    border: 2px solid rgba(79, 227, 209, 0.3);
    width: 30px;
    height: 30px;
    cursor: pointer;
  }
  .remove:hover,
  .remove:focus-visible {
    color: var(--orange);
    border-color: var(--orange);
    outline: none;
  }
</style>
