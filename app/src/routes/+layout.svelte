<script lang="ts">
  import "$lib/theme.css";
  import OceanBackground from "$lib/ocean/OceanBackground.svelte";
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
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
