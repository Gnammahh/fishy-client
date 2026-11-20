<script lang="ts">
  import FishProgressBar from "$lib/progress/FishProgressBar.svelte";
  import { formatBytes, percent } from "$lib/progress/progress";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { downloads, installed, pause, resume, cancel, type DownloadItem } from "./installs";

  const GENERIC_ERROR = "Download stopped: the connection to the download server dropped.";

  const current = $derived($downloads.find((d) => d.state === "downloading") ?? $downloads.find((d) => d.state !== "queued"));
  const queued = $derived($downloads.filter((d) => d.state === "queued"));
  const others = $derived($downloads.filter((d) => d !== current && d.state !== "queued"));

  /** app_name of the download waiting for "Delete files?" */
  let confirmCancel = $state<string | null>(null);
  let finished = $state<string[]>([]);

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

  {#each finished as title (title)}
    <p class="note" role="status">Finished: {title}. It's in your Library.</p>
  {/each}

  {#if !current && queued.length === 0}
    <p class="empty">Nothing downloading. Pick a game in your Library and press Install.</p>
  {/if}

  {#if current}
    <article class="current">
      <h3>{current.title}</h3>
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
        <PixelButton variant="ghost" onclick={() => (confirmCancel = current.app_name)}>Cancel</PixelButton>
      </div>
    </article>
  {/if}

  {#if queued.length > 0}
    <h3 class="sub">Up next</h3>
    <ul class="list">
      {#each queued as d (d.app_name)}
        <li>
          <span class="name">{d.title}</span>
          <span class="state">Queued</span>
          <button class="remove" aria-label="Remove {d.title} from the queue" onclick={() => cancel(d.app_name)}>×</button>
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

{#if confirmCancel}
  <ConfirmDialog
    message="Stop and delete the downloaded files?"
    confirmLabel="Delete files"
    onconfirm={() => {
      const appName = confirmCancel;
      confirmCancel = null;
      if (appName) cancel(appName);
    }}
    oncancel={() => (confirmCancel = null)}
  />
{/if}

<style>
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
