<script lang="ts">
  import "$lib/theme.css";
  import OceanBackground from "$lib/ocean/OceanBackground.svelte";
  import { onMount, type Snippet } from "svelte";
  import { syncMotionAttribute } from "$lib/motion";

  let { children }: { children: Snippet } = $props();

  // CSS-only animations (ocean, sidebar fish) follow the Reduce motion setting through this attribute.
  onMount(() => syncMotionAttribute());
</script>

<OceanBackground />
<div class="app-content">
  {@render children()}
</div>

<style>
  .app-content {
    position: relative;
    z-index: 1;
  }
</style>
