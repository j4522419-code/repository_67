<script lang="ts">
  // Choosing the browser and search engine: on first run, and later
  // from the tray's "Settings…". Works from the keyboard: Tab moves between
  // sections, arrow keys pick, Enter saves.
  import { onMount } from "svelte";
  import { backend, type SetupOptions } from "./backend";

  let { firstRun, onDone }: { firstRun: boolean; onDone: () => void } = $props();

  let options = $state<SetupOptions | null>(null);
  /** "" stands for Windows' default browser. */
  let browser = $state("");
  let searchEngine = $state("g");
  let form = $state<HTMLFormElement>();

  onMount(async () => {
    options = await backend.setupOptions();
    browser = options.current.browser ?? "";
    searchEngine = options.current.searchEngine;
    queueMicrotask(() => form?.querySelector<HTMLInputElement>("input:checked")?.focus());
  });

  async function save(event: SubmitEvent) {
    event.preventDefault();
    await backend.saveSetup({ browser: browser || null, searchEngine });
    onDone();
  }

  function onKeydown(event: KeyboardEvent) {
    // Browsers don't always submit a form on Enter from an option button.
    if (event.key === "Enter") {
      event.preventDefault();
      form?.requestSubmit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      // First run: come back to setup next time. Otherwise: never mind.
      if (firstRun) backend.hide();
      else onDone();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<form class="setup" bind:this={form} onsubmit={save}>
  <h1>{firstRun ? "Welcome to Grandium" : "Settings"}</h1>
  <p class="intro">
    {firstRun ? "Pick a few things first. " : ""}You can change these any time from the tray icon →
    Settings.
  </p>

  {#if options}
    <fieldset>
      <legend>Open web searches and links in</legend>
      <div class="choices">
        <label><input type="radio" name="browser" value="" bind:group={browser} />Windows default</label>
        {#each options.browsers as choice (choice.id)}
          <label><input type="radio" name="browser" value={choice.id} bind:group={browser} />{choice.name}</label>
        {/each}
      </div>
    </fieldset>

    <fieldset>
      <legend>Search the web with</legend>
      <div class="choices">
        {#each options.searchEngines as choice (choice.id)}
          <label>
            <input type="radio" name="engine" value={choice.id} bind:group={searchEngine} />{choice.name}
          </label>
        {/each}
      </div>
    </fieldset>
  {/if}

  <footer>
    <span><kbd>Tab</kbd> next · <kbd>←</kbd><kbd>→</kbd> choose · <kbd>Enter</kbd> save</span>
    <button type="submit">{firstRun ? "Start using Grandium" : "Save"}</button>
  </footer>
</form>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 20px 0;
    color: var(--text);
  }

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }

  .intro {
    margin: -8px 0 0;
    font-size: 13px;
    color: var(--text-secondary);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    padding: 0;
    margin-bottom: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  label {
    position: relative;
    padding: 6px 12px;
    border: 1px solid var(--divider);
    border-radius: 999px;
    background: var(--subtle-fill);
    font-size: 13.5px;
    cursor: default;
  }

  label:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-fill);
  }

  label:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 4px -20px 0;
    padding: 10px 20px;
    border-top: 1px solid var(--divider);
    font-size: 12px;
    color: var(--text-secondary);
  }

  button {
    padding: 7px 14px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--on-accent);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
  }

  kbd {
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--divider);
    background: var(--subtle-fill);
    font: inherit;
  }

  kbd + kbd {
    margin-left: 2px;
  }
</style>
