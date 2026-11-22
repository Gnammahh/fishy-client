<script lang="ts">
  import { onMount } from "svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import { formatBytes } from "$lib/progress/progress";
  import { refreshInstalled } from "$lib/install/installs";
  import { scanEpic, addFromEpic, repairFromEpic, type Candidate } from "./epicImport";

  /** One line per game that couldn't be added straight away (spec §6g). */
  type Line = { game: Candidate; text: string; repair: boolean; error: string | null };

  let waiting = $state<Candidate[]>([]);
  let lines = $state<Line[]>([]);
  let checking = $state<string | null>(null);
  let hidden = $state(false);

  onMount(async () => {
    waiting = await scanEpic();
  });

  const count = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

  async function addAll() {
    const todo = waiting;
    waiting = [];
    for (const game of todo) {
      checking = game.title;
      const r = await addFromEpic(game.app_name);
      if (r.kind === "missing") {
        const what = r.files === 1 ? "1 file is" : `${r.files} files are`;
        lines.push({ game, text: `${game.title}: ${what} missing or damaged (${formatBytes(r.bytes)}).`, repair: true, error: null });
      } else if (r.kind === "error") {
        lines.push({ game, text: r.message, repair: false, error: null });
        if (r.signed_out) break;
      }
    }
    checking = null;
    await refreshInstalled();
  }

  async function repair(line: Line) {
    line.error = null;
    const err = await repairFromEpic(line.game.app_name);
    if (err) line.error = err;
    else lines = lines.filter((l) => l !== line);
  }
</script>

{#if !hidden && (waiting.length > 0 || lines.length > 0 || checking)}
  <div class="note" role="status">
    {#if waiting.length > 0}
      <div class="row">
        <span>{count(waiting.length, "game", "games")} installed by Epic's launcher can be added.</span>
        <PixelButton onclick={addAll}>Add them</PixelButton>
      </div>
    {/if}
    {#if checking}<p class="row">Checking {checking}…</p>{/if}
    {#each lines as line (line.game.app_name)}
      <div class="row">
        <span>{line.text}</span>
        {#if line.repair}<PixelButton variant="ghost" onclick={() => repair(line)}>Download them</PixelButton>{/if}
      </div>
      {#if line.error}<p class="error" role="alert">{line.error}</p>{/if}
    {/each}
    {#if !checking}<button class="hide" aria-label="Hide" onclick={() => (hidden = true)}>×</button>{/if}
  </div>
{/if}

<style>
  .note {
    position: relative;
    display: grid;
    gap: 8px;
    padding: 10px 40px 10px 14px;
    border: 2px solid var(--aqua);
    background: rgba(11, 15, 20, 0.9);
    color: var(--text);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    margin: 0;
  }
  .error {
    margin: 0;
    color: var(--error);
  }
  .hide {
    position: absolute;
    top: 6px;
    right: 8px;
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 18px;
    cursor: pointer;
  }
</style>
