<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorMessage, type AuthInfo } from "$lib/api";

  let { auth, onsignedin }: { auth: AuthInfo; onsignedin: () => void } = $props();

  const MY_APPLICATIONS = "https://consumer-api-doc.airthings.com/dashboard";

  let busy = $state(false);
  let error = $state("");
  let clientId = $state("");
  let clientSecret = $state("");
  let showCredentials = $state(untrack(() => !auth.oauthAvailable));

  async function run(action: () => Promise<void>) {
    busy = true;
    error = "";
    try {
      await action();
      onsignedin();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  const signInOAuth = () => run(api.signInOAuth);
  const signInCredentials = (e: SubmitEvent) => {
    e.preventDefault();
    run(() => api.signInClientCredentials(clientId, clientSecret));
  };
</script>

<main class="login">
  <section class="card panel">
    <h1>AirDash</h1>
    <p class="muted">Your Airthings air quality, on your desktop.</p>

    {#if auth.oauthAvailable}
      <button class="primary big" onclick={signInOAuth} disabled={busy}>
        {busy && !showCredentials ? "Waiting for the browser…" : "Sign in with Airthings"}
      </button>
      {#if busy && !showCredentials}
        <p class="muted small">Finish signing in in the browser window that opened, then come back here.</p>
      {/if}
      <button class="link toggle" onclick={() => (showCredentials = !showCredentials)}>
        {showCredentials ? "Hide" : "Use your own API client instead"}
      </button>
    {/if}

    {#if showCredentials}
      <form onsubmit={signInCredentials}>
        <p class="muted small">
          Create an API client in
          <button type="button" class="link" onclick={() => api.openUrl(MY_APPLICATIONS)}>Airthings My Applications</button>
          and paste its client ID and secret here. You only need to do this once.
        </p>
        <label>
          Client ID
          <input type="text" bind:value={clientId} autocomplete="off" spellcheck="false" required />
        </label>
        <label>
          Client secret
          <input type="password" bind:value={clientSecret} autocomplete="off" required />
        </label>
        <button class="primary" type="submit" disabled={busy}>{busy ? "Signing in…" : "Sign in"}</button>
      </form>
    {/if}

    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
  <p class="muted small footnote">
    AirDash is not made by Airthings. Readings are stored only on this computer.
  </p>
</main>

<style>
  .login {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 16px;
    gap: 1rem;
  }
  .panel {
    width: min(420px, 100%);
    padding: 2rem;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }
  h1 {
    font-size: 1.6rem;
  }
  p {
    margin: 0;
  }
  .big {
    padding: 0.7rem;
    font-size: 1rem;
  }
  .toggle {
    align-self: center;
    font-size: 0.85rem;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
    color: var(--text-2);
  }
  .small {
    font-size: 0.85rem;
  }
  .footnote {
    text-align: center;
  }
</style>
