<script lang="ts">
  // Making or changing a snippet: a keyword, and the text it pastes.
  // Tab moves between the two, Ctrl+Enter saves, Esc goes back.
  import { onMount } from "svelte";
  import { backend, type SnippetEditing } from "./backend";

  let {
    editing,
    onDone,
  }: { editing: SnippetEditing; onDone: (savedKeyword: string | null) => void } = $props();

  // Copies: the draft is only a starting point.
  // svelte-ignore state_referenced_locally
  let keyword = $state(editing.draft.keyword);
  // svelte-ignore state_referenced_locally
  let text = $state(editing.draft.text);
  let error = $state<string | null>(null);
  let keywordInput = $state<HTMLInputElement>();
  let textArea = $state<HTMLTextAreaElement>();

  onMount(() => {
    // Start where there's something left to type.
    if (keyword) {
      textArea?.focus();
      textArea?.setSelectionRange(text.length, text.length);
    } else {
      keywordInput?.focus();
    }
  });

  async function save() {
    error = null;
    try {
      onDone(await backend.saveSnippet({ id: editing.draft.id, keyword, text }));
    } catch (e) {
      error = String(e);
    }
  }

  /** Puts a placeholder where the cursor is in the text. */
  function insert(placeholder: string) {
    const area = textArea;
    if (!area) return;
    const start = area.selectionStart ?? text.length;
    const end = area.selectionEnd ?? start;
    text = text.slice(0, start) + placeholder + text.slice(end);
    queueMicrotask(() => {
      area.focus();
      area.setSelectionRange(start + placeholder.length, start + placeholder.length);
    });
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && event.ctrlKey) {
      event.preventDefault();
      save();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onDone(null);
    } else if (event.key === "Enter" && event.target === keywordInput) {
      // Enter in the keyword moves on to the text, like Tab.
      event.preventDefault();
      textArea?.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form class="editor" onkeydown={onKeydown} onsubmit={(e) => (e.preventDefault(), save())}>
  <h1>{editing.draft.id === null ? "New snippet" : "Edit snippet"}</h1>

  <label class="keyword">
    <span class="label">Keyword</span>
    <span class="field">
      <span class="semicolon" aria-hidden="true">;</span>
      <input
        bind:this={keywordInput}
        bind:value={keyword}
        type="text"
        placeholder="addr"
        spellcheck="false"
        autocomplete="off"
      />
    </span>
    <span class="hint">Type <kbd>;{keyword.replace(/^;/, "") || "addr"}</kbd> in Grandium to paste it</span>
  </label>

  <label class="text">
    <span class="label">Text to paste</span>
    <textarea bind:this={textArea} bind:value={text} rows="6" spellcheck="false"></textarea>
  </label>

  <div class="placeholders">
    <span class="label">Filled in when pasted:</span>
    {#each editing.placeholders as placeholder (placeholder.text)}
      <button type="button" tabindex="-1" title={placeholder.meaning} onclick={() => insert(placeholder.text)}>
        {placeholder.text}
      </button>
    {/each}
  </div>

  {#if error}
    <div class="error" role="alert">{error}</div>
  {/if}

  <footer>
    <span><kbd>Tab</kbd> next · <kbd>Ctrl+Enter</kbd> save · <kbd>Esc</kbd> back</span>
    <button type="submit" class="save">Save</button>
  </footer>
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px 20px 0;
    color: var(--text);
  }

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .field {
    display: flex;
    align-items: center;
    width: 220px;
    border: 1px solid var(--divider);
    border-radius: 6px;
    background: var(--subtle-fill);
  }

  .semicolon {
    padding-left: 10px;
    color: var(--text-tertiary);
    font-size: 15px;
  }

  input,
  textarea {
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px 7px 2px;
  }

  textarea {
    padding: 8px 10px;
    border: 1px solid var(--divider);
    border-radius: 6px;
    background: var(--subtle-fill);
    resize: none;
    line-height: 1.4;
    user-select: text;
  }

  .field:focus-within,
  textarea:focus {
    border-color: var(--accent);
  }

  .hint {
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .placeholders {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: -4px;
  }

  .placeholders .label {
    font-weight: normal;
    font-size: 12px;
  }

  .placeholders button {
    padding: 2px 8px;
    border: 1px solid var(--divider);
    border-radius: 999px;
    background: var(--subtle-fill);
    color: var(--text-secondary);
    font: inherit;
    font-size: 12px;
  }

  .error {
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
    margin: 4px -20px 0;
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
</style>
