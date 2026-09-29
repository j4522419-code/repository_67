<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    backend,
    type AppStatus,
    type ResultAction,
    type SearchResult,
    type SnippetEditing,
  } from "./lib/backend";
  import Glyph from "./lib/Glyph.svelte";
  import Highlighted from "./lib/Highlighted.svelte";
  import ResultIcon from "./lib/ResultIcon.svelte";
  import { BUTTONS, splitScope, type Scope } from "./lib/scopes";
  import Setup from "./lib/Setup.svelte";
  import SnippetEditor from "./lib/SnippetEditor.svelte";
  import { applyTheme } from "./lib/theme";

  /** What's typed in the search box. */
  let text = $state("");
  /** A slash command shown as a pill before the text, like "Files". */
  let scope = $state<Scope | null>(null);
  const query = $derived(scope ? `/${scope.command} ${text}` : text);

  let results = $state<SearchResult[]>([]);
  /** Section titles, by the index of the result they come before. */
  let headings = $state<Map<number, string>>(new Map());
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
  /** Showing the snippet editor instead of search. */
  let editing = $state<SnippetEditing | null>(null);
  let input = $state<HTMLInputElement>();
  let root = $state<HTMLElement>();
  let latestSearch = 0;

  const current = $derived<SearchResult | undefined>(results[selected]);

  // Search again whenever the query changes (and only then: `search` reads
  // other state, which mustn't re-trigger this).
  $effect(() => {
    const q = query;
    untrack(() => search(q));
  });

  // Keep the selected row visible when the list scrolls.
  $effect(() => {
    void selected;
    void results;
    queueMicrotask(() => root?.querySelector(".row.selected")?.scrollIntoView({ block: "nearest" }));
  });

  const SECTIONS: Record<string, string> = {
    App: "Applications",
    File: "Documents",
    Folder: "Folders",
    Snippet: "Snippets",
    Note: "Notes",
    Clipboard: "Clipboard",
    System: "System",
    Run: "Run",
    Calculator: "Calculator",
    Web: "Search the web",
  };

  /** Like Spotlight: the best match first as the Top Hit, then the rest
   * grouped by kind, in the order each kind first shows up. The web search
   * always comes last. */
  function inSections(found: SearchResult[]) {
    const [top, ...rest] = found;
    const groups = new Map<string, SearchResult[]>();
    for (const result of rest) {
      groups.set(result.kind, [...(groups.get(result.kind) ?? []), result]);
    }
    const ordered = [...groups.values()].sort(
      (a, b) => Number(a[0].kind === "Web") - Number(b[0].kind === "Web"),
    );
    const sections = new Map<number, string>([[0, "Top Hit"]]);
    const flat = [top];
    for (const members of ordered) {
      sections.set(flat.length, SECTIONS[members[0].kind] ?? members[0].kind);
      flat.push(...members);
    }
    return { flat, sections };
  }

  /** Plain text searches everything, and its results come in sections. */
  function searchesEverything(q: string) {
    return !/^\s*[/;=]/.test(q);
  }

  /** With `keepSelection`, stays on the same result if it's still there. */
  async function search(q: string, keepSelection = false) {
    const id = ++latestSearch;
    const previous = { id: current?.id, index: selected };
    const found = await backend.search(q);
    if (id !== latestSearch) return; // a newer search already started
    if (searchesEverything(q) && found.length > 2) {
      const { flat, sections } = inSections(found);
      results = flat;
      headings = sections;
    } else {
      results = found;
      headings = new Map();
    }
    const same = results.findIndex((result) => result.id === previous.id);
    selected = !keepSelection ? 0 : same >= 0 ? same : Math.min(previous.index, results.length - 1);
    selected = Math.max(selected, 0);
    showActions = false;
    confirming = null;
    error = null;
  }

  /** Puts `typed` in the search box, turning a leading slash command into
   * a pill. */
  function setQuery(typed: string) {
    const split = splitScope(typed);
    scope = split?.scope ?? null;
    text = split ? split.rest : typed;
  }

  /** Typing "/files " makes a Files pill. */
  function onInput() {
    if (!scope && splitScope(text)) setQuery(text);
  }

  /** Narrows the search to one kind of thing, or back to everything. */
  function pick(button: Scope | "commands") {
    if (button === "commands") {
      scope = null;
      text = "/";
    } else {
      scope = scope?.command === button.command ? null : button;
      text = "";
    }
    input?.focus();
  }

  /** Puts a slash command in the search box, ready for what comes after. */
  function fill(result: SearchResult) {
    if (result.fill === null) return false;
    setQuery(result.fill);
    input?.focus();
    return true;
  }

  async function run(result: SearchResult, action: ResultAction) {
    error = null;
    if (action.id === "fill" && fill(result)) return;
    if (action.id === "editSnippet") {
      try {
        editing = await backend.snippetDraft(result.id);
      } catch (e) {
        error = String(e);
      }
      return;
    }
    const confirmed = confirming?.result.id === result.id && confirming.action.id === action.id;
    if (action.confirm && !confirmed) {
      confirming = { result, action };
      return;
    }
    confirming = null;
    try {
      const outcome = await backend.runAction(result.id, action.id);
      if (outcome === "refresh") search(query, true);
      else setQuery("");
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

  /** Right-clicking a row shows what can be done with it. */
  function openActions(event: MouseEvent, index: number) {
    event.preventDefault();
    selected = index;
    confirming = null;
    showActions = true;
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
        else if (text) text = "";
        else if (scope) scope = null;
        else backend.hide();
        break;
      case "Backspace":
        // Backspace at the start of the box takes the pill away.
        if (scope && input?.selectionStart === 0 && input.selectionEnd === 0) {
          event.preventDefault();
          scope = null;
        }
        break;
      default: {
        if (!event.ctrlKey && !event.altKey) break;
        // Ctrl+1 to Ctrl+6: the buttons in the search bar.
        const button = event.ctrlKey && !event.altKey && !event.shiftKey ? BUTTONS[Number(event.key) - 1] : undefined;
        if (button) {
          event.preventDefault();
          pick(button);
          break;
        }
        const action = current && !showActions ? actionFor(event, current) : undefined;
        // Also keeps browser shortcuts like Ctrl+P (print) from firing.
        if (action || ["P", "F", "R", "S", "E", "H"].includes(event.key.toUpperCase())) event.preventDefault();
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
      applyTheme(s.theme);
      if (s.setupNeeded) showSetup = true;
    });
  }

  function onShown() {
    focusInput();
    // A quick fade in, like Spotlight's. Restarting the animation needs it
    // taken off and put back after a layout.
    root?.classList.remove("appear");
    void root?.offsetWidth;
    root?.classList.add("appear");
    // Alt+Space may have been freed up, or setup reset.
    refreshStatus();
  }

  /** Back from the snippet editor: to the saved snippet, if it was saved. */
  function finishEditing(savedKeyword: string | null) {
    editing = null;
    const next = savedKeyword === null ? query : `;${savedKeyword}`;
    // Changing the text searches again; the same text needs asking.
    if (next !== query) setQuery(next);
    else search(query, true);
    queueMicrotask(focusInput);
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

    // The window is sized to fit what's on screen: at most once a frame,
    // and only when the height really changed.
    let lastHeight = 0;
    let queued = false;
    const resize = new ResizeObserver(() => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        const height = Math.ceil(root?.getBoundingClientRect().height ?? 0);
        if (height && height !== lastHeight) {
          lastHeight = height;
          backend.setHeight(height);
        }
      });
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

<main class="launcher appear" bind:this={root}>
  {#if showSetup}
    <Setup
      firstRun={status?.setupNeeded ?? false}
      takenHotkey={status?.hotkeyError ? status.hotkey : null}
      onDone={finishSetup}
    />
  {:else if editing}
    <SnippetEditor {editing} onDone={finishEditing} />
  {:else}
    <div class="bar" class:open={results.length || query.trim()}>
      <Glyph name="search" size={22} />
      {#if scope}
        <span class="pill"><Glyph name={scope.glyph} size={14} />{scope.label}</span>
      {/if}
      <input
        bind:this={input}
        bind:value={text}
        oninput={onInput}
        onkeydown={onKeydown}
        type="text"
        placeholder={scope ? `Search ${scope.label.toLowerCase()}` : "Grandium Search"}
        spellcheck="false"
        autocomplete="off"
        aria-label="Search"
        aria-controls="results"
      />
      <!-- The buttons never take focus: typing keeps going to the search box. -->
      <div class="buttons" role="toolbar" aria-label="Show everything of one kind">
        {#each BUTTONS as button, i (button === "commands" ? button : button.command)}
          {@const label = button === "commands" ? "Commands" : button.label}
          <button
            type="button"
            tabindex="-1"
            title="{label} (Ctrl+{i + 1})"
            aria-label={label}
            class:active={button === "commands" ? !scope && text === "/" : scope?.command === button.command}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => pick(button)}
          >
            <Glyph name={button === "commands" ? "commands" : button.glyph} size={17} />
          </button>
        {/each}
      </div>
    </div>

    {#if status?.hotkeyError}
      <div class="notice warning" role="alert">
        <strong>{status.hotkey} is taken by another app</strong>
        (PowerToys Run or Command Palette, for example). Pick another one in the tray icon →
        Settings, or open Grandium from its tray icon.
      </div>
    {/if}

    <!-- Only the keyboard moves the selection; hovering doesn't. Clicking a
         row runs it without taking focus away from the search box, and
         right-clicking shows its actions. -->
    <div id="results" class="results" onmousedown={(e) => e.preventDefault()} role="presentation">
      {#if showActions && current}
        <div class="heading">Actions for {current.title}</div>
        {#each current.actions as action, i (action.id)}
          <button
            type="button"
            tabindex="-1"
            class="row action"
            class:selected={i === selectedAction}
            onclick={() => run(current, action)}
          >
            <span class="title">{action.label}</span>
            {#if action.shortcut}<kbd>{action.shortcut}</kbd>{/if}
          </button>
        {/each}
      {:else if results.length}
        {#each results as result, i (result.id)}
          {#if headings.has(i)}<div class="heading">{headings.get(i)}</div>{/if}
          <button
            type="button"
            tabindex="-1"
            class="row"
            class:selected={i === selected}
            onclick={() => run(result, result.actions[0])}
            oncontextmenu={(e) => openActions(e, i)}
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
              {#if i === selected}{result.actions[0].label} <kbd>↵</kbd>{:else if !headings.size}{result.kind}{/if}
            </span>
          </button>
        {/each}
      {:else if text === "" && scope}
        <div class="empty">Nothing in {scope.label.toLowerCase()} yet</div>
      {:else if /^\/\S+\s+$/.test(query)}
        <div class="empty">Now type after {query.trim()}…</div>
      {:else if query.trim()}
        <div class="empty">No results for “{query.trim()}”</div>
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
  {/if}
</main>

<style>
  .launcher {
    display: flex;
    flex-direction: column;
    color: var(--text);
  }

  /* A quick fade and drop into place each time the launcher opens. */
  .launcher.appear {
    animation: appear 150ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  @keyframes appear {
    from {
      opacity: 0.35;
      transform: translateY(-5px) scale(0.99);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .launcher.appear {
      animation: none;
    }
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 58px;
    padding: 0 12px 0 18px;
    color: var(--text-secondary);
  }

  .bar.open {
    border-bottom: 1px solid var(--divider);
  }

  input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font-family: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
    font-size: 22px;
    font-weight: 400;
    caret-color: var(--accent);
  }

  input::placeholder {
    color: var(--text-tertiary);
  }

  .pill {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px 3px 8px;
    border-radius: 999px;
    background: var(--selection-bg);
    color: var(--selection-text);
    font-size: 13px;
    font-weight: 600;
  }

  .buttons {
    flex: none;
    display: flex;
    gap: 2px;
  }

  .buttons button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--text-tertiary);
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .buttons button:hover {
    background: var(--subtle-fill-strong);
    color: var(--text-secondary);
  }

  .buttons button.active {
    background: var(--selection-bg);
    color: var(--selection-text);
  }

  /* About nine rows, then it scrolls. */
  .results {
    display: flex;
    flex-direction: column;
    max-height: 420px;
    overflow-y: auto;
    padding: 0 8px;
    scrollbar-width: thin;
  }

  .results:not(:empty) {
    padding-top: 4px;
    padding-bottom: 8px;
  }

  .heading {
    flex: none;
    padding: 8px 10px 3px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-tertiary);
  }

  .row {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 0 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: default;
    transition:
      background-color 80ms ease,
      color 80ms ease;
  }

  .row.action {
    min-height: 36px;
  }

  /* The selected row is filled with the accent color, like on a Mac. */
  .row.selected {
    background: var(--selection-bg);
    color: var(--selection-text);
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
    font-size: 14.5px;
  }

  .subtitle {
    flex: 0 0 auto;
    max-width: 45%;
    font-size: 13px;
    color: var(--text-tertiary);
  }

  .row.selected .subtitle {
    color: var(--selection-text-secondary);
  }

  .meta {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .row.selected .meta {
    color: var(--selection-text);
  }

  .preview {
    margin: 0 12px 10px;
    padding: 10px 12px;
    border-radius: 8px;
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

  .notice,
  .empty {
    margin: 0 12px 10px;
    padding: 10px 12px;
    border-radius: 8px;
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
    margin: 4px 2px 2px;
    color: var(--text-secondary);
  }

  kbd {
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--divider);
    background: var(--subtle-fill);
    font: inherit;
    font-size: 11.5px;
  }

  .row kbd {
    margin-left: auto;
  }

  .row.selected kbd {
    border-color: var(--selection-text-secondary);
    background: transparent;
    color: var(--selection-text);
  }

  .meta kbd {
    margin-left: 0;
  }
</style>
