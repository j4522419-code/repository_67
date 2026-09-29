<script lang="ts">
  // Text with some ranges emphasized, e.g. the letters a search matched.
  let { text, ranges }: { text: string; ranges: [number, number][] } = $props();

  const parts = $derived.by(() => {
    const out: { text: string; match: boolean }[] = [];
    let at = 0;
    for (const [start, end] of ranges) {
      if (start > at) out.push({ text: text.slice(at, start), match: false });
      out.push({ text: text.slice(start, end), match: true });
      at = end;
    }
    if (at < text.length) out.push({ text: text.slice(at), match: false });
    return out;
  });
</script>

{#each parts as part, i (i)}{#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}

<style>
  mark {
    background: none;
    color: inherit;
    font-weight: 600;
  }
</style>
