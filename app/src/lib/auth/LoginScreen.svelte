<script lang="ts">
  import FishLoader from "$lib/loader/FishLoader.svelte";
  import PixelButton from "$lib/ui/PixelButton.svelte";
  import icon from "../../../src-tauri/icons/128x128.png";
  import { signIn, submitCode, openBrowserLogin, signingIn, submitting, loginError } from "./auth";

  let showPaste = $state(false);
  let pasted = $state("");

  function onSubmit(e: SubmitEvent) {
    e.preventDefault();
    submitCode(pasted);
  }
</script>

<section class="login">
  <img class="logo" src={icon} width="128" height="128" alt="" />
  <h1>Fishy Client</h1>
  <p class="lede">Sign in with your Epic Games account to see your library.</p>

  {#if $signingIn && !showPaste}
    <div class="waiting">
      <FishLoader size="medium" />
      <p>Finish signing in in the Epic window.</p>
    </div>
  {:else}
    <PixelButton onclick={signIn}>Sign in with Epic</PixelButton>
  {/if}

  {#if $loginError}
    <p class="error" role="alert">{$loginError}</p>
  {/if}

  <p class="trust">Fishy Client never sees your password. You sign in on Epic's own page.</p>

  {#if !showPaste}
    <button class="link" type="button" onclick={() => (showPaste = true)}>Trouble signing in? Paste a code instead</button>
  {:else}
    <form class="paste" onsubmit={onSubmit}>
      <ol>
        <li>
          Open Epic's sign-in page in your browser.
          <PixelButton variant="ghost" onclick={openBrowserLogin}>Open sign-in page</PixelButton>
        </li>
        <li>Sign in. Epic then shows a short page of text.</li>
        <li>Copy the value after <code>authorizationCode</code>, or the whole page, and paste it here.</li>
      </ol>
      <label for="sign-in-code">Sign-in code</label>
      <textarea id="sign-in-code" rows="3" bind:value={pasted} spellcheck="false" autocomplete="off"></textarea>
      <PixelButton type="submit" disabled={$submitting || !pasted.trim()}>Sign in with code</PixelButton>
    </form>
  {/if}
</section>

<style>
  .login {
    min-height: calc(100vh - var(--titlebar));
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 48px 24px;
    text-align: center;
  }
  .logo {
    image-rendering: pixelated;
  }
  h1 {
    font-family: var(--font-pixel);
    font-weight: 600;
    font-size: 52px;
    line-height: 1;
    color: var(--aqua);
    margin: 0;
  }
  .lede {
    margin: 0;
    color: var(--text);
  }
  .waiting {
    display: grid;
    justify-items: center;
    gap: 10px;
    color: var(--text-2);
  }
  .waiting p {
    margin: 0;
  }
  .error {
    margin: 0;
    color: var(--error);
    max-width: 52ch;
  }
  .trust {
    margin: 0;
    color: var(--text-2);
    font-size: 13px;
  }
  .link {
    background: none;
    border: 0;
    color: var(--aqua);
    font: inherit;
    font-size: 14px;
    text-decoration: underline;
    text-underline-offset: 3px;
    cursor: pointer;
    padding: 4px;
  }
  .link:focus-visible {
    outline: 2px solid var(--orange);
  }
  .paste {
    display: grid;
    gap: 10px;
    width: min(100%, 460px);
    text-align: left;
    padding: 18px;
    border: 2px solid rgba(79, 227, 209, 0.18);
    background: rgba(11, 15, 20, 0.86);
  }
  .paste ol {
    margin: 0;
    padding-left: 20px;
    display: grid;
    gap: 10px;
    color: var(--text-2);
    font-size: 14px;
  }
  .paste li :global(.pixel-btn) {
    margin-top: 6px;
  }
  label {
    font-family: var(--font-pixel);
    color: var(--aqua);
  }
  textarea {
    background: #000;
    color: var(--text);
    border: 2px solid rgba(79, 227, 209, 0.35);
    padding: 8px;
    font: 13px ui-monospace, Consolas, monospace;
    resize: vertical;
  }
  textarea:focus-visible {
    outline: none;
    border-color: var(--orange);
  }
  code {
    color: var(--orange);
  }
</style>
