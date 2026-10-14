<script lang="ts">
  import { LEVEL_LABEL, type Level } from "$lib/sensors";

  let { level, compact = false }: { level: Level; compact?: boolean } = $props();
</script>

<!-- Shape and label carry the level too, so it never depends on color alone. -->
<span class="badge {level}" title={LEVEL_LABEL[level]}>
  <svg viewBox="0 0 10 10" aria-hidden="true">
    {#if level === "good"}
      <circle cx="5" cy="5" r="4" />
    {:else if level === "fair"}
      <path d="M5 1 L9.2 9 H0.8 Z" />
    {:else}
      <rect x="1.2" y="1.2" width="7.6" height="7.6" rx="1" />
    {/if}
  </svg>
  {#if !compact}<span>{LEVEL_LABEL[level]}</span>{/if}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.8rem;
    color: var(--text-2);
  }
  svg {
    width: 0.7rem;
    height: 0.7rem;
  }
  .good svg {
    fill: var(--good);
  }
  .fair svg {
    fill: var(--fair);
  }
  .poor svg {
    fill: var(--poor);
  }
</style>
