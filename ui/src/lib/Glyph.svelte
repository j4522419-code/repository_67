<script lang="ts">
  // One of the built-in icons, drawn in the current text color.
  import { GLYPHS } from "./glyphs";

  let { name, size = 18 }: { name: string; size?: number } = $props();
</script>

<svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
  {#each GLYPHS[name] ?? [] as d (d)}<path {d} />{/each}
</svg>

<style>
  svg {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
