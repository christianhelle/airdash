<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, type PollerStatus } from "$lib/api";
  import { timeAgo, timeUntil } from "$lib/format";

  let { status }: { status: PollerStatus } = $props();

  // Re-render relative times every 30 seconds.
  let now = $state(Date.now());
  const timer = setInterval(() => (now = Date.now()), 30_000);
  onDestroy(() => clearInterval(timer));
</script>

<footer class="status">
  <span class="state">
    {#if status.polling}
      Updating…
    {:else if status.paused}
      Recording paused
    {:else if status.lastPoll}
      Updated {timeAgo(status.lastPoll * 1000, now)}
    {:else}
      Not updated yet
    {/if}
    {#if !status.paused && !status.polling && status.nextPoll}
      <span class="muted">· next {timeUntil(status.nextPoll * 1000, now)}</span>
    {/if}
  </span>

  {#if status.lastError}
    <span class="error" title={status.lastError}>{status.lastError}</span>
  {/if}

  <span class="actions">
    {#if status.rateLimit?.remaining != null && status.rateLimit?.limit != null}
      <span class="muted" title="Airthings API requests left this hour">
        API {status.rateLimit.remaining}/{status.rateLimit.limit}
      </span>
    {/if}
    <button class="link" onclick={() => api.setPaused(!status.paused)}>
      {status.paused ? "Resume" : "Pause"}
    </button>
    <button class="link" onclick={() => api.refreshNow()} disabled={status.polling || status.paused}>Refresh</button>
  </span>
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.45rem 1rem;
    border-top: 1px solid var(--border);
    background: var(--surface);
    font-size: 0.82rem;
  }
  .error {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    margin-left: auto;
    display: flex;
    gap: 0.9rem;
    align-items: center;
  }
</style>
