<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import {
    api,
    events,
    type AuthInfo,
    type Device,
    type DeviceSensors,
    type PollerStatus,
    type Settings,
  } from "$lib/api";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import Dashboard from "$lib/views/Dashboard.svelte";
  import DeviceDetail from "$lib/views/DeviceDetail.svelte";
  import Login from "$lib/views/Login.svelte";
  import SettingsView from "$lib/views/Settings.svelte";

  type View = { name: "dashboard" } | { name: "device"; serial: string } | { name: "settings" };

  let auth = $state<AuthInfo | null>(null);
  let settings = $state<Settings | null>(null);
  let status = $state<PollerStatus | null>(null);
  let devices = $state<Device[]>([]);
  let readings = $state<DeviceSensors[]>([]);
  let loaded = $state(false);
  let view = $state<View>({ name: "dashboard" });
  // Bumped on every new batch of readings so open charts reload.
  let version = $state(0);

  const selected = $derived.by(() => {
    const v = view;
    return v.name === "device" ? devices.find((d) => d.serialNumber === v.serial) : undefined;
  });

  $effect(() => {
    const theme = settings?.theme ?? "system";
    if (theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  });

  async function loadData() {
    [devices, readings] = await Promise.all([api.devices(), api.latestReadings()]);
    loaded = true;
  }

  async function refreshAuth() {
    auth = await api.authInfo();
    status = await api.pollerStatus();
    if (auth.signedIn) await loadData();
  }

  const unlisteners: Promise<UnlistenFn>[] = [];

  onMount(async () => {
    settings = await api.getSettings();
    await refreshAuth();
    unlisteners.push(
      events.pollerStatus((s) => {
        status = s;
        // A rejected refresh token signs the user out in the background.
        if (auth?.signedIn && !s.signedIn) refreshAuth();
      }),
      events.devicesUpdated(loadData),
      events.readingsUpdated(async () => {
        await loadData();
        version++;
      }),
    );
  });

  onDestroy(() => unlisteners.forEach((u) => u.then((fn) => fn())));

  async function signedOut() {
    view = { name: "dashboard" };
    devices = [];
    readings = [];
    loaded = false;
    await refreshAuth();
  }
</script>

{#if auth && settings}
  {#if !auth.signedIn}
    <Login {auth} onsignedin={refreshAuth} />
  {:else}
    <div class="app">
      <nav>
        <button class="brand link" onclick={() => (view = { name: "dashboard" })}>AirDash</button>
        <button class="link" onclick={() => (view = { name: "settings" })} aria-current={view.name === "settings"}>
          Settings
        </button>
      </nav>

      <main>
        {#if view.name === "settings"}
          <SettingsView
            {settings}
            {auth}
            onsaved={(s) => (settings = s)}
            onsignedout={signedOut}
            onback={() => (view = { name: "dashboard" })}
          />
        {:else if view.name === "device" && selected}
          <DeviceDetail device={selected} unit={settings.unit} {version} onback={() => (view = { name: "dashboard" })} />
        {:else}
          <Dashboard
            {devices}
            {readings}
            {loaded}
            unit={settings.unit}
            onopen={(d) => (view = { name: "device", serial: d.serialNumber })}
          />
        {/if}
      </main>

      {#if status}<StatusBar {status} />{/if}
    </div>
  {/if}
{/if}

<style>
  .app {
    height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
  }
  nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .brand {
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--text);
  }
  main {
    overflow-y: auto;
    padding: 1.25rem 16px;
  }
  @media (min-width: 700px) {
    main {
      padding: 1.5rem;
    }
  }
</style>
