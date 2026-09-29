<script lang="ts">
  import { onMount } from "svelte";
  import { backend, type AppStatus } from "./lib/backend";

  let query = $state("");
  let status = $state<AppStatus | null>(null);
  let input = $state<HTMLInputElement>();
  let root = $state<HTMLElement>();

  function focusInput() {
    input?.focus();
    input?.select();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (query) query = "";
      else backend.hide();
    }
  }

  onMount(() => {
    backend.status().then((s) => (status = s));
    const unlisten = backend.onShown(focusInput);

    // The window is sized to fit whatever is on screen.
    const resize = new ResizeObserver(() => {
      if (root) backend.setHeight(Math.ceil(root.getBoundingClientRect().height));
    });
    if (root) resize.observe(root);

    focusInput();
    return () => {
      unlisten.then((stop) => stop());
      resize.disconnect();
    };
  });
</script>

<main class="launcher" bind:this={root}>
  <div class="search">
    <svg class="search-icon" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="M15.5 15.5 21 21" />
    </svg>
    <input
      bind:this={input}
      bind:value={query}
      onkeydown={onKeydown}
      type="text"
      placeholder="Search apps, files, clipboard…"
      spellcheck="false"
      autocomplete="off"
      aria-label="Search"
    />
  </div>

  {#if status?.hotkeyError}
    <div class="notice" role="alert">
      <strong>{status.hotkey} is taken by another app</strong>
      (PowerToys Run or Command Palette, for example). For now, open Grandium from its tray icon.
    </div>
  {/if}

  {#if query}
    <div class="empty">Search isn't wired up yet. App search arrives in the next update.</div>
  {/if}

  <footer>
    {#if status?.hotkeyError}
      <span>Grandium keeps running in the tray</span>
    {:else}
      <span>Press <kbd>{status?.hotkey ?? "Alt+Space"}</kbd> anytime to open Grandium</span>
    {/if}
    <span><kbd>Esc</kbd> close</span>
  </footer>
</main>

<style>
  .launcher {
    display: flex;
    flex-direction: column;
    color: var(--text);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 60px;
    padding: 0 20px;
  }

  .search-icon {
    flex: none;
    width: 20px;
    height: 20px;
    fill: none;
    stroke: var(--text-secondary);
    stroke-width: 2;
    stroke-linecap: round;
  }

  input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 20px;
    caret-color: var(--accent);
  }

  input::placeholder {
    color: var(--text-tertiary);
  }

  .notice,
  .empty {
    margin: 0 12px 10px;
    padding: 10px 12px;
    border-radius: 6px;
    font-size: 13px;
    line-height: 1.45;
  }

  .notice {
    background: var(--warning-bg);
    color: var(--text);
  }

  .empty {
    color: var(--text-secondary);
    background: var(--subtle-fill);
  }

  footer {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 20px;
    border-top: 1px solid var(--divider);
    font-size: 12px;
    color: var(--text-secondary);
  }

  kbd {
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--divider);
    background: var(--subtle-fill);
    font: inherit;
  }
</style>
