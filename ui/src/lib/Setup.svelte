<script lang="ts">
  // Every setting, on one screen: on first run (just the essentials, and
  // how to use Grandium), after installing, and from the tray's
  // "Settings…". Works from the keyboard: Tab moves between sections,
  // arrow keys pick, Enter saves (or presses the button that has focus).
  import { onMount } from "svelte";
  import { backend, type SetupChoices, type SetupOptions } from "./backend";
  import { applyTheme, type Theme } from "./theme";

  let {
    firstRun,
    takenHotkey,
    onDone,
  }: {
    firstRun: boolean;
    /** The hotkey another app has, if Grandium couldn't get it. */
    takenHotkey: string | null;
    onDone: () => void;
  } = $props();

  let options = $state<SetupOptions | null>(null);
  /** "" stands for Windows' default browser. */
  let browser = $state("");
  let searchEngine = $state("g");
  let hotkey = $state("Alt+Space");
  let theme = $state<Theme>("system");
  /** Radio buttons hold text, so "yes" or "no". */
  let startWithWindows = $state("yes");
  let clipboardMaxItems = $state(500);
  let clipboardMaxDays = $state(30);
  let extraFolders = $state<string[]>([]);
  let hidden = $state<{ key: string; name: string }[]>([]);
  let error = $state<string | null>(null);
  let form = $state<HTMLFormElement>();

  onMount(async () => {
    try {
      options = await backend.setupOptions();
    } catch (e) {
      error = `Couldn't load the settings: ${e}`;
      return;
    }
    const current = options.current;
    browser = current.browser ?? "";
    searchEngine = current.searchEngine;
    hotkey = current.hotkey;
    theme = current.theme;
    startWithWindows = current.startWithWindows ? "yes" : "no";
    clipboardMaxItems = current.clipboardMaxItems;
    clipboardMaxDays = current.clipboardMaxDays;
    extraFolders = current.extraFolders;
    hidden = current.hidden;
    queueMicrotask(() => form?.querySelector<HTMLInputElement>("input:checked")?.focus());
  });

  // Show a theme as soon as it's picked; leaving without saving puts the
  // saved one back.
  $effect(() => applyTheme(theme));

  function choices(): SetupChoices {
    return {
      browser: browser || null,
      searchEngine,
      hotkey,
      theme,
      startWithWindows: startWithWindows === "yes",
      clipboardMaxItems: Number(clipboardMaxItems),
      clipboardMaxDays: Number(clipboardMaxDays),
      extraFolders,
      hidden,
    };
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    error = null;
    try {
      await backend.saveSetup(choices());
    } catch (e) {
      error = String(e);
      return;
    }
    onDone();
  }

  async function addFolder() {
    error = null;
    try {
      const folder = await backend.pickFolder();
      const known = extraFolders.some((f) => f.toLowerCase() === folder?.toLowerCase());
      if (folder && !known) extraFolders = [...extraFolders, folder];
    } catch (e) {
      error = `Couldn't add the folder: ${e}`;
    }
  }

  function removeFolder(folder: string) {
    extraFolders = extraFolders.filter((f) => f !== folder);
  }

  function onKeydown(event: KeyboardEvent) {
    const onButton = event.target instanceof HTMLButtonElement && event.target.type === "button";
    if (event.key === "Enter" && !onButton) {
      // Browsers don't always submit a form on Enter from an option button.
      event.preventDefault();
      form?.requestSubmit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      // First run: come back to setup next time. Otherwise: never mind.
      if (firstRun) backend.hide();
      else onDone();
    }
  }

  const dayLabel = (days: number) => (days === 1 ? "1 day" : days === 365 ? "1 year" : `${days} days`);
</script>

<svelte:window onkeydown={onKeydown} />

<form class="setup" bind:this={form} onsubmit={save}>
  <header>
    <h1>{firstRun ? "Welcome to Grandium" : "Settings"}</h1>
    <p class="intro">
      {firstRun ? "Pick a few things first. " : ""}You can change these any time from the tray icon →
      Settings.
    </p>
  </header>

  <div class="sections">
    {#if firstRun}
      <ul class="tips" aria-label="How to use Grandium">
        <li>Press <kbd>{hotkey}</kbd> anywhere, then type to find apps, files, notes and anything you copied.</li>
        <li><kbd>/</kbd> lists every command, like <kbd>/files</kbd>, <kbd>/clip</kbd> and <kbd>/note</kbd>.</li>
        <li><kbd>;</kbd> pastes a snippet, <kbd>=</kbd> does math, and <kbd>Tab</kbd> shows more actions.</li>
      </ul>
    {/if}

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

      <fieldset>
        <legend>Start Grandium when you sign in to Windows</legend>
        <div class="choices">
          <label><input type="radio" name="startup" value="yes" bind:group={startWithWindows} />Yes</label>
          <label><input type="radio" name="startup" value="no" bind:group={startWithWindows} />No</label>
        </div>
        <p class="note">It waits in the tray, so {hotkey} works right away.</p>
      </fieldset>

      {#if !firstRun || takenHotkey}
        <fieldset>
          <legend>Open Grandium with</legend>
          <div class="choices">
            {#each options.hotkeys as choice (choice)}
              <label><input type="radio" name="hotkey" value={choice} bind:group={hotkey} />{choice}</label>
            {/each}
          </div>
          {#if takenHotkey}
            <p class="note warning">
              Another app is using {takenHotkey}.{hotkey === takenHotkey ? " Pick a different one." : ""}
            </p>
          {/if}
        </fieldset>
      {/if}

      {#if !firstRun}
        <fieldset>
          <legend>Look</legend>
          <div class="choices">
            {#each options.themes as choice (choice.id)}
              <label><input type="radio" name="theme" value={choice.id} bind:group={theme} />{choice.name}</label>
            {/each}
          </div>
        </fieldset>

        <fieldset>
          <legend>Clipboard history keeps</legend>
          <div class="choices">
            {#each options.clipboardItems as count (count)}
              <label>
                <input type="radio" name="items" value={count} bind:group={clipboardMaxItems} />{count.toLocaleString()} items
              </label>
            {/each}
          </div>
          <div class="choices">
            {#each options.clipboardDays as days (days)}
              <label>
                <input type="radio" name="days" value={days} bind:group={clipboardMaxDays} />for {dayLabel(days)}
              </label>
            {/each}
          </div>
          <p class="note">Pinned items are always kept.</p>
        </fieldset>

        <fieldset>
          <legend>File search looks in</legend>
          <p class="note first">{options.userFolders.join(", ")}{extraFolders.length ? ", and:" : ""}</p>
          {#each extraFolders as folder (folder)}
            <div class="folder">
              <span class="path">{folder}</span>
              <button type="button" onclick={() => removeFolder(folder)}>Remove</button>
            </div>
          {/each}
          <button type="button" class="add" onclick={addFolder}>Add a folder…</button>
        </fieldset>

        <fieldset>
          <legend>Hidden from Grandium</legend>
          {#each hidden as item (item.key)}
            <div class="folder">
              <span class="path">{item.name}</span>
              <button type="button" onclick={() => (hidden = hidden.filter((h) => h.key !== item.key))}>
                Show again
              </button>
            </div>
          {:else}
            <p class="note first">
              Nothing yet. Right-click something in the results (or press Tab) and pick “Hide from
              Grandium”.
            </p>
          {/each}
        </fieldset>
      {/if}
    {/if}
  </div>

  {#if error}
    <div class="error" role="alert">{error}</div>
  {/if}

  <footer>
    <span><kbd>Tab</kbd> next · <kbd>←</kbd><kbd>→</kbd> choose · <kbd>Enter</kbd> save</span>
    <button type="submit" class="save">{firstRun ? "Start using Grandium" : "Save"}</button>
  </footer>
</form>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    color: var(--text);
  }

  header {
    padding: 18px 20px 12px;
  }

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }

  .intro {
    margin: 6px 0 0;
    font-size: 13px;
    color: var(--text-secondary);
  }

  /* The window stops growing at 720px; past that, settings scroll. */
  .sections {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-height: 540px;
    overflow-y: auto;
    padding: 2px 20px 16px;
  }

  .tips {
    margin: 0;
    padding: 10px 14px 10px 30px;
    border-radius: 8px;
    background: var(--accent-fill);
    font-size: 13px;
    line-height: 1.7;
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

  .choices + .choices {
    margin-top: 6px;
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

  label:has(input:focus-visible),
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .note {
    margin: 8px 0 0;
    font-size: 12.5px;
    color: var(--text-secondary);
  }

  .note.first {
    margin: 0 0 8px;
  }

  .note.warning {
    color: var(--text);
  }

  .folder {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
    padding: 5px 6px 5px 12px;
    border: 1px solid var(--divider);
    border-radius: 6px;
    background: var(--subtle-fill);
    font-size: 13px;
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button {
    padding: 4px 10px;
    border: 1px solid var(--divider);
    border-radius: 6px;
    background: var(--subtle-fill-strong);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
  }

  .add {
    align-self: flex-start;
  }

  .error {
    margin: 0 20px 10px;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--error-bg);
    font-size: 13px;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 20px;
    border-top: 1px solid var(--divider);
    font-size: 12px;
    color: var(--text-secondary);
  }

  .save {
    padding: 7px 14px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--on-accent);
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
