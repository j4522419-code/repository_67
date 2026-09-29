<script lang="ts">
  // A result's icon: the app's own icon, a built-in symbol for things like
  // the calculator, or else the first letter of its title on a tile.
  let {
    src,
    glyph,
    title,
    thumbnail = false,
  }: { src: string | null; glyph: string | null; title: string; thumbnail?: boolean } = $props();

  import { GLYPHS } from "./glyphs";

  let failed = $state(false);
  $effect(() => {
    void src;
    failed = false;
  });

  const paths = $derived(glyph ? GLYPHS[glyph] : undefined);
</script>

{#if src && !failed}
  <img {src} alt="" class:thumbnail loading="lazy" decoding="async" onerror={() => (failed = true)} />
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
    width: 28px;
    height: 28px;
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
    font-size: 14px;
    transition:
      background-color 90ms ease,
      color 90ms ease;
  }

  svg {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
