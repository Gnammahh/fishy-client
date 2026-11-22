<script lang="ts">
  import type { Snippet } from "svelte";
  import CrashScreen from "./CrashScreen.svelte";
  import { reportUiError } from "./crash";

  // Catches a bug in any screen below it and shows the crash page instead of a blank window.
  let { children, onreload }: { children: Snippet; onreload?: () => void } = $props();
</script>

<svelte:boundary onerror={reportUiError}>
  {@render children()}
  {#snippet failed()}
    <CrashScreen {onreload} />
  {/snippet}
</svelte:boundary>
