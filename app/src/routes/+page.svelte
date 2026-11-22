<script lang="ts">
  import { onMount } from "svelte";
  import { dev } from "$app/environment";
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import LoginScreen from "$lib/auth/LoginScreen.svelte";
  import AppShell from "$lib/shell/AppShell.svelte";
  import { authState, init, loginError } from "$lib/auth/auth";

  // Dev only: ?crash=1 shows the crash page, to check how it looks.
  const crashForPreview = dev && typeof location !== "undefined" && new URLSearchParams(location.search).get("crash") === "1";
  if (crashForPreview) throw new Error("preview crash");

  onMount(async () => {
    // Dev only: in a plain browser there is no Rust side, so fake it (?mock=signedout|signedin|offline).
    if (dev && !("__TAURI_INTERNALS__" in window)) {
      const { installDevMock } = await import("$lib/auth/devMock");
      installDevMock(new URLSearchParams(location.search).get("mock") ?? "signedout");
    }
    try {
      await init();
    } catch (e) {
      authState.set({ kind: "SignedOut" });
      loginError.set(typeof e === "string" ? e : "Couldn't start the sign-in system. Please restart Fishy Client.");
    }
  });
</script>

{#if $authState.kind === "Loading"}
  <div class="boot" role="status" aria-label="Loading">
    <FishLoader size="large" />
  </div>
{:else if $authState.kind === "SignedOut"}
  <LoginScreen />
{:else}
  <AppShell state={$authState} />
{/if}

<style>
  .boot {
    min-height: 100vh;
    display: grid;
    place-items: center;
  }
</style>
