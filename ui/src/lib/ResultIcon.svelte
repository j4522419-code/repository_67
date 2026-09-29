<script lang="ts">
  // A result's icon: the app's own icon, a built-in symbol for things like
  // the calculator, or else the first letter of its title on a tile.
  let {
    src,
    glyph,
    title,
    thumbnail = false,
  }: { src: string | null; glyph: string | null; title: string; thumbnail?: boolean } = $props();

  // Outlines on a 24×24 grid.
  const GLYPHS: Record<string, string[]> = {
    calculator: [
      "M7 3h10a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
      "M8.5 6.5h7v3h-7z",
      "M9 13h.01M12 13h.01M15 13h.01M9 17h.01M12 17h.01M15 17h.01",
    ],
    web: [
      "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z",
      "M3 12h18",
      "M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9s1.3-6.4 3.8-9z",
    ],
    lock: [
      "M7 11h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2z",
      "M8 11V8a4 4 0 0 1 8 0v3",
    ],
    sleep: ["M20 14.5A8.5 8.5 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"],
    restart: ["M20 12a8 8 0 1 1-2.35-5.65", "M20 4v4.5h-4.5"],
    shutdown: ["M12 3v8", "M6.35 6.85a8 8 0 1 0 11.3 0"],
    signout: [
      "M10 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h4",
      "M15 16l4-4-4-4",
      "M19 12H9",
    ],
    emptybin: ["M4 7h16", "M9 7V4h6v3", "M6 7l1 13h10l1-13", "M10 11v6M14 11v6"],
    run: [
      "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
      "M7 9l3 3-3 3",
      "M12 15h5",
    ],
    folder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
    file: [
      "M7 3h7l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
      "M14 3v5h5",
      "M9 13h6M9 17h6",
    ],
    clipboard: [
      "M9 3h6a1 1 0 0 1 1 1v2H8V4a1 1 0 0 1 1-1z",
      "M8 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2",
    ],
    snippet: [
      "M6 4h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
      "M8 9h8M8 13h8M8 17h4",
    ],
    note: [
      "M5 4h14a1 1 0 0 1 1 1v10l-5 5H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z",
      "M20 15h-4a1 1 0 0 0-1 1v4",
      "M8 9h8M8 12h5",
    ],
    add: ["M12 5v14M5 12h14"],
    pause: ["M9 5v14M15 5v14"],
    play: ["M8 5l11 7-11 7z"],
  };

  let failed = $state(false);
  $effect(() => {
    void src;
    failed = false;
  });

  const paths = $derived(glyph ? GLYPHS[glyph] : undefined);
</script>

{#if src && !failed}
  <img {src} alt="" class:thumbnail onerror={() => (failed = true)} />
{:else if paths}
  <span class="tile" aria-hidden="true">
    <svg viewBox="0 0 24 24">
      {#each paths as d (d)}<path {d} />{/each}
    </svg>
  </span>
{:else}
  <span class="tile" aria-hidden="true">{title.charAt(0).toUpperCase()}</span>
{/if}

<style>
  img,
  .tile {
    flex: none;
    width: 32px;
    height: 32px;
  }

  img {
    object-fit: contain;
  }

  img.thumbnail {
    object-fit: cover;
    border-radius: 5px;
  }

  .tile {
    display: grid;
    place-items: center;
    border-radius: 7px;
    background: var(--subtle-fill-strong);
    color: var(--text-secondary);
    font-weight: 600;
    font-size: 15px;
  }

  svg {
    width: 20px;
    height: 20px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
