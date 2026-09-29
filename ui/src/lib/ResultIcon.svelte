<script lang="ts">
  // A result's icon, or its first letter on a tile when there's no icon.
  let { src, title }: { src: string | null; title: string } = $props();

  let failed = $state(false);
  $effect(() => {
    void src;
    failed = false;
  });
</script>

{#if src && !failed}
  <img {src} alt="" onerror={() => (failed = true)} />
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

  .tile {
    display: grid;
    place-items: center;
    border-radius: 7px;
    background: var(--subtle-fill-strong);
    color: var(--text-secondary);
    font-weight: 600;
    font-size: 15px;
  }
</style>
