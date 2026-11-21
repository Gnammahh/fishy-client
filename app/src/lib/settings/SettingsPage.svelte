<script lang="ts">
  import { settings, saveSettings, type Settings } from "./settings";

  let error = $state("");

  async function toggle(key: keyof Settings, e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    const wanted = box.checked;
    error = (await saveSettings({ [key]: wanted })) ?? "";
    if (error) box.checked = !wanted; // show what's really saved
  }
</script>

<section class="settings">
  <h2>Settings</h2>

  <h3>Updates</h3>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="auto-name"
      aria-describedby="auto-hint"
      checked={$settings.auto_update}
      onchange={(e) => toggle("auto_update", e)}
    />
    <span>
      <span class="name" id="auto-name">Update games automatically</span>
      <span class="hint" id="auto-hint">When your Library finds a new version of an installed game, it starts downloading by itself.</span>
    </span>
  </label>
  <label class="row">
    <input
      type="checkbox"
      role="switch"
      aria-labelledby="warn-name"
      aria-describedby="warn-hint"
      checked={$settings.warn_outdated_play}
      onchange={(e) => toggle("warn_outdated_play", e)}
    />
    <span>
      <span class="name" id="warn-name">Warn before playing a game with an update waiting</span>
      <span class="hint" id="warn-hint">Some online games won't start until they're updated.</span>
    </span>
  </label>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 640px;
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
    font-size: 16px;
    margin: 8px 0 0;
    color: var(--aqua);
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    cursor: pointer;
  }
  .row > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-weight: 600;
  }
  .hint {
    font-size: 13px;
    opacity: 0.75;
  }
  input {
    width: 18px;
    height: 18px;
    margin-top: 2px;
    accent-color: var(--orange);
    flex-shrink: 0;
  }
  .error {
    color: var(--error);
  }
</style>
