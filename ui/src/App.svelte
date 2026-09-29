<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    backend,
    type AppStatus,
    type ResultAction,
    type SearchResult,
  } from "./lib/backend";
  import Highlighted from "./lib/Highlighted.svelte";
  import ResultIcon from "./lib/ResultIcon.svelte";
  import Setup from "./lib/Setup.svelte";

  let query = $state("");
  let results = $state<SearchResult[]>([]);
  let selected = $state(0);
  /** Showing the actions of the selected result instead of the results. */
  let showActions = $state(false);
  let selectedAction = $state(0);
  /** An action waiting for Enter again, e.g. Shut down. */
  let confirming = $state<{ result: SearchResult; action: ResultAction } | null>(null);
  let error = $state<string | null>(null);
  let status = $state<AppStatus | null>(null);
  /** Showing the setup screen instead of search. */
  let showSetup = $state(false);
  let input = $state<HTMLInputElement>();
  let root = $state<HTMLElement>();
  let latestSearch = 0;

  const current = $derived<SearchResult | undefined>(results[selected]);

  // Search again whenever the text changes (and only then: `search` reads
  // other state, which mustn't re-trigger this).
  $effect(() => {
    const text = query;
    untrack(() => search(text));
  });

  // Keep the selected row visible when the list scrolls.
  $effect(() => {
    void selected;
    void results;
    queueMicrotask(() => root?.querySelector(".row.selected")?.scrollIntoView({ block: "nearest" }));
  });

  /** With `keepSelection`, stays on the same result if it's still there. */
  async function search(text: string, keepSelection = false) {
    const id = ++latestSearch;
    const previous = { id: current?.id, index: selected };
    const found = await backend.search(text);
    if (id !== latestSearch) return; // a newer search already started
    results = found;
    const same = found.findIndex((result) => result.id === previous.id);
    selected = !keepSelection ? 0 : same >= 0 ? same : Math.min(previous.index, found.length - 1);
    selected = Math.max(selected, 0);
    showActions = false;
    confirming = null;
    error = null;
  }

  /** Puts a slash command in the search box, ready for what comes after. */
  function fill(result: SearchResult) {
    if (result.fill === null) return false;
    query = result.fill;
    input?.focus();
    return true;
  }

  async function run(result: SearchResult, action: ResultAction) {
    error = null;
    if (action.id === "fill" && fill(result)) return;
    const confirmed = confirming?.result.id === result.id && confirming.action.id === action.id;
    if (action.confirm && !confirmed) {
      confirming = { result, action };
      return;
    }
    confirming = null;
    try {
      const outcome = await backend.runAction(result.id, action.id);
      if (outcome === "refresh") search(query, true);
      else query = "";
    } catch (e) {
      error = String(e);
    }
  }

  function move(delta: number) {
    confirming = null;
    if (showActions && current) {
      const count = current.actions.length;
      selectedAction = (selectedAction + delta + count) % count;
    } else if (results.length) {
      selected = (selected + delta + results.length) % results.length;
    }
  }

  function toggleActions() {
    if (!current) return;
    confirming = null;
    showActions = !showActions;
    selectedAction = 0;
  }

  /** The keys pressed, written like action shortcuts: "Ctrl+Shift+Enter". */
  function keysOf(event: KeyboardEvent) {
    const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
    return [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", key]
      .filter(Boolean)
      .join("+");
  }

  /** Enter runs the first action; other shortcuts run the matching one. */
  function actionFor(event: KeyboardEvent, result: SearchResult) {
    const keys = keysOf(event);
    if (keys === "Enter") return result.actions[0];
    return result.actions.find((action) => action.shortcut === keys);
  }

  function onKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
        event.preventDefault();
        move(event.key === "ArrowDown" ? 1 : -1);
        break;
      case "Tab":
        event.preventDefault();
        // Tab completes a slash command, like in a terminal.
        if (!(current && !showActions && fill(current))) toggleActions();
        break;
      case "Enter": {
        event.preventDefault();
        if (confirming) {
          run(confirming.result, confirming.action);
          break;
        }
        if (!current) break;
        const action = showActions ? current.actions[selectedAction] : actionFor(event, current);
        if (action) run(current, action);
        break;
      }
      case "Escape":
        event.preventDefault();
        if (confirming) confirming = null;
        else if (showActions) showActions = false;
        else if (query) query = "";
        else backend.hide();
        break;
      default: {
        if (!event.ctrlKey && !event.altKey) break;
        const action = current && !showActions ? actionFor(event, current) : undefined;
        // Also keeps browser shortcuts like Ctrl+P (print) from firing.
        if (action || ["P", "F", "R", "S"].includes(event.key.toUpperCase())) event.preventDefault();
        if (action && current) run(current, action);
      }
    }
  }

  function focusInput() {
    input?.focus();
    input?.select();
  }

  /** Also opens setup if it hasn't been done yet. */
  function refreshStatus() {
    return backend.status().then((s) => {
      status = s;
      if (s.setupNeeded) showSetup = true;
    });
  }

  function onShown() {
    focusInput();
    // The keys may have changed from the tray menu.
    refreshStatus();
  }

  async function finishSetup() {
    showSetup = false;
    await refreshStatus();
    focusInput();
  }

  onMount(() => {
    refreshStatus();
    const unlisten = backend.onShown(onShown);
    const unlistenSetup = backend.onShowSetup(() => (showSetup = true));

    // The window is sized to fit whatever is on screen.
    const resize = new ResizeObserver(() => {
      if (root) backend.setHeight(Math.ceil(root.getBoundingClientRect().height));
    });
    if (root) resize.observe(root);

    focusInput();
    return () => {
      unlisten.then((stop) => stop());
      unlistenSetup.then((stop) => stop());
      resize.disconnect();
    };
  });
</script>

<main class="launcher" bind:this={root}>
  {#if showSetup}
    <Setup firstRun={status?.setupNeeded ?? false} onDone={finishSetup} />
  {:else}
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
      placeholder="Search apps, or type / for commands"
      spellcheck="false"
      autocomplete="off"
      aria-label="Search"
      aria-controls="results"
    />
  </div>

  {#if status?.hotkeyError}
    <div class="notice warning" role="alert">
      <strong>Alt+Space is taken by another app</strong>
      (PowerToys Run or Command Palette, for example).
      {status.keys.length
        ? `You can still use the ${status.keys.join(" or ")}.`
        : "Open Grandium from its tray icon, or pick the Windows key there."}
    </div>
  {/if}
  {#if status?.windowsKeyError}
    <div class="notice warning" role="alert">
      <strong>The Windows key couldn't be set up.</strong>
      {status.windowsKeyError}
    </div>
  {/if}

  <!-- Only the keyboard moves the selection; hovering doesn't. Clicking a
       row runs it without taking focus away from the search box. -->
  <div id="results" class="results" onmousedown={(e) => e.preventDefault()} role="presentation">
    {#if showActions && current}
      <div class="heading">Actions for {current.title}</div>
      {#each current.actions as action, i (action.id)}
        <button
          type="button"
          tabindex="-1"
          class="row"
          class:selected={i === selectedAction}
          onclick={() => run(current, action)}
        >
          <span class="title">{action.label}</span>
          {#if action.shortcut}<kbd>{action.shortcut}</kbd>{/if}
        </button>
      {/each}
    {:else if results.length}
      {#each results as result, i (result.id)}
        <button
          type="button"
          tabindex="-1"
          class="row"
          class:selected={i === selected}
          onclick={() => run(result, result.actions[0])}
        >
          <ResultIcon
            src={result.icon ? backend.iconUrl(result.icon) : null}
            glyph={result.glyph}
            title={result.title}
            thumbnail={result.icon?.scheme === "clip"}
          />
          <span class="title"><Highlighted text={result.title} ranges={result.highlights} /></span>
          {#if result.subtitle}<span class="subtitle">{result.subtitle}</span>{/if}
          <span class="meta">
            {i === selected ? `${result.actions[0].label} ↵` : result.kind}
          </span>
        </button>
      {/each}
    {:else if /^\/\S+\s+$/.test(query)}
      <div class="empty">Now type after {query.trim()}…</div>
    {:else if query.trim()}
      <div class="empty">No matches for “{query.trim()}”</div>
    {/if}
  </div>

  {#if current?.preview && !showActions}
    <div class="preview">
      {#if current.preview.image}
        <img src={backend.iconUrl(current.preview.image)} alt="What was copied" />
      {:else if current.preview.text}
        <p>{current.preview.text}</p>
      {/if}
    </div>
  {/if}

  {#if confirming}
    <div class="notice warning" role="alert">
      <strong>{confirming.action.confirm}</strong>
      Press <kbd>Enter</kbd> again to confirm, or <kbd>Esc</kbd> to cancel.
    </div>
  {/if}

  {#if error}
    <div class="notice error" role="alert">{error}</div>
  {/if}

  <footer>
    {#if confirming}
      <span><kbd>↵</kbd> {confirming.action.label}</span>
    {:else if results.length}
      <span><kbd>↑</kbd><kbd>↓</kbd> select · <kbd>↵</kbd> open · <kbd>Tab</kbd> {current?.fill ? "complete" : "actions"}</span>
    {:else if status && !status.keys.length}
      <span>Grandium keeps running in the tray</span>
    {:else}
      <span>
        Press
        {#each status?.keys ?? ["Alt+Space"] as key, i (key)}{#if i > 0}&nbsp;or{" "}{/if}<kbd>{key}</kbd>{/each}
        anytime to open Grandium
      </span>
    {/if}
    <span><kbd>Esc</kbd> {confirming ? "cancel" : showActions ? "back" : "close"}</span>
  </footer>
  {/if}
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

  .results {
    display: flex;
    flex-direction: column;
    padding: 0 8px;
  }

  .results:not(:empty) {
    padding-bottom: 8px;
  }

  /* Eight rows, then it scrolls. */
  .results {
    max-height: 392px;
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .preview {
    margin: 0 12px 10px;
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--subtle-fill);
  }

  .preview p {
    margin: 0;
    max-height: 7.5em;
    overflow: hidden;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-secondary);
    user-select: text;
  }

  .preview img {
    display: block;
    max-width: 100%;
    max-height: 120px;
    margin: 0 auto;
    border-radius: 4px;
  }

  .heading {
    padding: 4px 12px 6px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .row {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 48px;
    padding: 0 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: default;
  }

  .row.selected {
    background: var(--selected-fill);
  }

  /* A long title gives way first, so the subtitle (like "2 h ago") stays
     visible; very long subtitles, like paths, are cut too. */
  .title,
  .subtitle {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    flex: 0 1 auto;
  }

  .subtitle {
    flex: 0 0 auto;
    max-width: 45%;
  }

  .title {
    font-size: 15px;
  }

  .subtitle {
    font-size: 13px;
    color: var(--text-tertiary);
  }

  .meta {
    flex: none;
    margin-left: auto;
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .row.selected .meta {
    color: var(--text-secondary);
  }

  .notice,
  .empty {
    margin: 0 12px 10px;
    padding: 10px 12px;
    border-radius: 6px;
    font-size: 13px;
    line-height: 1.45;
  }

  .notice.warning {
    background: var(--warning-bg);
  }

  .notice.error {
    background: var(--error-bg);
  }

  .empty {
    margin: 0 4px 2px;
    color: var(--text-secondary);
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

  kbd + kbd {
    margin-left: 2px;
  }

  .row kbd {
    margin-left: auto;
  }
</style>
