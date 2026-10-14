<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, errorMessage, type AuthInfo, type Settings } from "$lib/api";

  let {
    settings,
    auth,
    onsaved,
    onsignedout,
    onback,
  }: {
    settings: Settings;
    auth: AuthInfo;
    onsaved: (s: Settings) => void;
    onsignedout: () => void;
    onback: () => void;
  } = $props();

  // Edit a copy; changes apply on Save.
  let draft = $state<Settings>(untrack(() => structuredClone($state.snapshot(settings))));
  let message = $state("");
  let dataDir = $state("");
  let confirmDelete = $state(false);

  onMount(async () => {
    dataDir = await api.dataDir();
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    try {
      const saved = await api.saveSettings(draft);
      draft = saved;
      onsaved(saved);
      message = "Settings saved.";
    } catch (err) {
      message = errorMessage(err);
    }
  }

  async function signOut() {
    await api.signOut();
    onsignedout();
  }

  async function deleteData() {
    try {
      await api.clearLocalData();
      message = "Local history deleted.";
    } catch (err) {
      message = errorMessage(err);
    } finally {
      confirmDelete = false;
    }
  }
</script>

<div class="settings">
  <header>
    <button class="link" onclick={onback}>← All devices</button>
    <h1>Settings</h1>
  </header>

  <form onsubmit={save}>
    <section class="card">
      <h2>Display</h2>
      <label>
        Units
        <select bind:value={draft.unit}>
          <option value="metric">Metric (°C, Bq/m³, hPa)</option>
          <option value="imperial">Imperial (°F, pCi/L, inHg)</option>
        </select>
      </label>
      <label>
        Theme
        <select bind:value={draft.theme}>
          <option value="system">Same as system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
    </section>

    <section class="card">
      <h2>Recording</h2>
      <label>
        Check for new readings every (minutes)
        <input type="number" min="2" max="120" bind:value={draft.pollIntervalMinutes} />
        <span class="hint">Airthings allows 120 requests per hour and most devices update every 5 minutes.</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.closeToTray} />
        Keep recording in the tray when the window is closed
      </label>
    </section>

    <details class="card advanced">
      <summary>Advanced sign-in options</summary>
      <p class="hint">
        Use a different Airthings OAuth client (authorization code flow). Register this redirect URI with it:
        <code>{auth.redirectUri}</code>
      </p>
      <label>
        OAuth client ID
        <input type="text" bind:value={draft.oauthClientId} spellcheck="false" placeholder="Built-in client" />
      </label>
      <label>
        OAuth client secret (leave empty for public clients)
        <input type="password" bind:value={draft.oauthClientSecret} autocomplete="off" />
      </label>
      <label>
        Redirect port
        <input type="number" min="1024" max="65535" bind:value={draft.redirectPort} />
      </label>
      <label>
        API client scope
        <input type="text" bind:value={draft.scope} spellcheck="false" />
      </label>
    </details>

    <div class="save">
      <button class="primary" type="submit">Save</button>
      {#if message}<span class="muted">{message}</span>{/if}
    </div>
  </form>

  <section class="card">
    <h2>Account</h2>
    <p class="muted">
      Signed in {auth.method === "oauth" ? "with your Airthings account" : "with an API client"}. Signing out keeps
      your recorded history.
    </p>
    <div><button onclick={signOut}>Sign out</button></div>
  </section>

  <section class="card">
    <h2>Local data</h2>
    <p class="muted">History is stored in <code>{dataDir}</code></p>
    <div class="row">
      <button onclick={() => api.openDataDir()}>Open folder</button>
      {#if confirmDelete}
        <button class="danger" onclick={deleteData}>Delete all history</button>
        <button onclick={() => (confirmDelete = false)}>Cancel</button>
      {:else}
        <button class="danger" onclick={() => (confirmDelete = true)}>Delete local data…</button>
      {/if}
    </div>
  </section>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 640px;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    align-items: flex-start;
  }
  h1 {
    font-size: 1.4rem;
  }
  h2 {
    font-size: 1rem;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  section,
  details {
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  summary {
    cursor: pointer;
    font-weight: 600;
  }
  details[open] summary {
    margin-bottom: 0.25rem;
  }
  details > * + * {
    margin-top: 0.75rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.88rem;
    color: var(--text-2);
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 0.5rem;
    color: var(--text);
  }
  .hint {
    font-size: 0.8rem;
    color: var(--text-3);
    margin: 0;
  }
  code {
    word-break: break-all;
  }
  p {
    margin: 0;
  }
  .save,
  .row {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    flex-wrap: wrap;
  }
</style>
