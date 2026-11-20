<script lang="ts">
  import icon from "../../../src-tauri/icons/32x32.png";

  export type Page = "library" | "downloads" | "settings" | "account";

  let { page, onselect }: { page: Page; onselect: (p: Page) => void } = $props();

  const ITEMS: { id: Page; label: string }[] = [
    { id: "library", label: "Library" },
    { id: "downloads", label: "Downloads" },
    { id: "settings", label: "Settings" },
    { id: "account", label: "Account" },
  ];
</script>

<nav class="side" aria-label="Sections">
  <div class="brand"><img src={icon} width="24" height="24" alt="" /><span>Fishy Client</span></div>
  {#each ITEMS as item (item.id)}
    <button type="button" aria-current={page === item.id ? "page" : undefined} onclick={() => onselect(item.id)}>
      {item.label}
    </button>
  {/each}
</nav>

<style>
  .side {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 0;
    border-right: 2px solid rgba(79, 227, 209, 0.18);
    background: rgba(11, 15, 20, 0.86);
    backdrop-filter: blur(6px);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 18px 18px;
    font-family: var(--font-pixel);
    color: var(--aqua);
    font-size: 17px;
  }
  .brand img {
    image-rendering: pixelated;
  }
  button {
    font-family: var(--font-pixel);
    font-size: 16px;
    text-align: left;
    padding: 10px 18px;
    color: var(--text-2);
    background: none;
    border: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }
  button::before {
    content: "";
    width: 6px;
    height: 6px;
    background: currentColor;
    opacity: 0.5;
  }
  button[aria-current="page"] {
    color: var(--aqua);
    background: rgba(79, 227, 209, 0.07);
  }
  button[aria-current="page"]::before {
    opacity: 1;
  }
  button:hover {
    color: var(--text);
  }
  button:focus-visible {
    outline: 2px solid var(--orange);
    outline-offset: -2px;
  }
</style>
