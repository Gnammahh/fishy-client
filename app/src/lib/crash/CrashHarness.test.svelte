<script lang="ts">
  // Test only: the crash boundary around a screen that throws.
  import CrashBoundary from "./CrashBoundary.svelte";
  import Boom from "./Boom.test.svelte";

  let { onreload }: { onreload?: () => void } = $props();
</script>

<CrashBoundary {onreload}><Boom /></CrashBoundary>
