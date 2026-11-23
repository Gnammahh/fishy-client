<script lang="ts">
  import "$lib/theme.css";
  import OceanBackground from "$lib/ocean/OceanBackground.svelte";
  import TitleBar from "$lib/shell/TitleBar.svelte";
  import { onMount, type Snippet } from "svelte";
  import { syncMotionAttribute, syncPausedAttribute, watchMinimized } from "$lib/motion";
  import CrashBoundary from "$lib/crash/CrashBoundary.svelte";
  import { installGlobalHandlers } from "$lib/crash/crash";

  let { children }: { children: Snippet } = $props();

  // CSS-only animations (ocean, sidebar fish) follow the Reduce motion setting through this attribute.
  onMount(() => syncMotionAttribute());
  // They also pause while minimized or while a game runs, to stay light (spec §6r).
  onMount(() => syncPausedAttribute());
  onMount(() => watchMinimized());
  // Errors that don't break a screen still go to the log (spec §6f.2).
  onMount(() => installGlobalHandlers());
</script>

<CrashBoundary>
  <OceanBackground />
  <TitleBar />
  <div class="app-content">
    {@render children()}
  </div>
</CrashBoundary>

<style>
  .app-content {
    position: relative;
    z-index: 1;
    /* below Fishy Client's own title bar (spec §6s) */
    padding-top: var(--titlebar);
  }
</style>
