<script lang="ts">
  import { api, errorMessage, type Device, type RemoteControlState, type SensorInfo, type Unit } from "$lib/api";
  import Chart from "$lib/components/Chart.svelte";
  import { safeFileName } from "$lib/format";
  import { deviceTypeName, displayUnit, sensorLabel, sortSensors } from "$lib/sensors";

  let { device, unit, version, onback }: { device: Device; unit: Unit; version: number; onback: () => void } =
    $props();

  const RANGES = [
    { id: "24h", label: "24 hours", seconds: 86_400 },
    { id: "7d", label: "7 days", seconds: 7 * 86_400 },
    { id: "30d", label: "30 days", seconds: 30 * 86_400 },
    { id: "1y", label: "1 year", seconds: 365 * 86_400 },
    { id: "all", label: "All", seconds: null },
  ] as const;
  const MAX_POINTS = 600;

  let range = $state<(typeof RANGES)[number]>(RANGES[0]);
  let sensors = $state<SensorInfo[]>([]);
  let series = $state<Record<string, [number, number][]>>({});
  let loaded = $state(false);
  let message = $state("");

  function bounds(): [number, number] {
    const to = Math.floor(Date.now() / 1000);
    return [range.seconds === null ? 0 : to - range.seconds, to];
  }

  async function load() {
    try {
      const [from, to] = bounds();
      const types = sortSensors(await api.sensorTypes(device.serialNumber), (s) => s.sensorType);
      const data = await Promise.all(
        types.map((s) => api.series(device.serialNumber, s.sensorType, from, to, MAX_POINTS)),
      );
      sensors = types;
      series = Object.fromEntries(types.map((s, i) => [s.sensorType, data[i]]));
    } catch (e) {
      message = errorMessage(e);
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    // Reload when the range changes or new readings arrive.
    void range;
    void version;
    load();
  });

  async function exportCsv() {
    const [from, to] = bounds();
    try {
      const path = await api.exportCsv(device.serialNumber, `${safeFileName(device.name)}-${range.id}.csv`, from, to);
      message = path ? `Saved ${path}` : "";
    } catch (e) {
      message = errorMessage(e);
    }
  }

  // Renew air purifier controls.
  const isRenew = $derived(device.type === "AP_1");
  let control = $state<RemoteControlState | null>(null);
  let controlBusy = $state(false);

  $effect(() => {
    if (isRenew) {
      api.remoteControl(device.serialNumber).then((c) => (control = c)).catch((e) => (message = errorMessage(e)));
    }
  });

  async function setMode(mode: RemoteControlState["mode"], fanSpeed?: number) {
    controlBusy = true;
    try {
      const next: RemoteControlState = mode === "MANUAL" ? { mode, fanSpeed: fanSpeed ?? control?.fanSpeed ?? 3 } : { mode };
      await api.setRemoteControl(device.serialNumber, next);
      control = next;
      message = "Sent to the purifier. It applies the change the next time it syncs.";
    } catch (e) {
      message = errorMessage(e);
    } finally {
      controlBusy = false;
    }
  }
</script>

<div class="detail">
  <header>
    <button class="link" onclick={onback}>← All devices</button>
    <div class="title">
      <h1>{device.name || device.serialNumber}</h1>
      <span class="muted">{deviceTypeName(device.type)} · {device.serialNumber}</span>
    </div>
  </header>

  <div class="toolbar">
    <div class="ranges" role="group" aria-label="Time range">
      {#each RANGES as r (r.id)}
        <button class:active={r.id === range.id} aria-pressed={r.id === range.id} onclick={() => (range = r)}>{r.label}</button>
      {/each}
    </div>
    <button onclick={exportCsv} disabled={sensors.length === 0}>Export CSV</button>
  </div>

  {#if message}<p class="muted message">{message}</p>{/if}

  {#if isRenew}
    <section class="card controls">
      <h2>Purifier mode</h2>
      <div class="modes">
        {#each ["AUTO", "SLEEP", "BOOST", "MANUAL", "OFF"] as const as mode (mode)}
          <button class:active={control?.mode === mode} disabled={controlBusy} onclick={() => setMode(mode)}>
            {mode.charAt(0) + mode.slice(1).toLowerCase()}
          </button>
        {/each}
      </div>
      {#if control?.mode === "MANUAL"}
        <label>
          Fan speed {control.fanSpeed}
          <input
            type="range"
            min="1"
            max="5"
            value={control.fanSpeed ?? 3}
            disabled={controlBusy}
            onchange={(e) => setMode("MANUAL", Number(e.currentTarget.value))}
          />
        </label>
      {/if}
    </section>
  {/if}

  {#if loaded && sensors.length === 0}
    <div class="card empty">
      <h2>No history yet</h2>
      <p class="muted">
        Airthings only shares the latest values, so AirDash builds history by recording every update while it runs.
        Charts fill in over time. Keep AirDash running in the tray to keep recording.
      </p>
    </div>
  {/if}

  <div class="charts">
    {#each sensors as s (s.sensorType)}
      {@const points = series[s.sensorType] ?? []}
      <section class="card chart">
        <h2>
          {sensorLabel(s.sensorType)}
          <span class="muted unit">{displayUnit(s.sensorType, unit, s.unit)}</span>
        </h2>
        {#if points.length > 0}
          <Chart {points} sensorType={s.sensorType} {unit} rawUnit={s.unit} />
        {:else}
          <p class="muted">No readings in this range.</p>
        {/if}
      </section>
    {/each}
  </div>
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: 1rem;
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
  .title {
    display: flex;
    flex-direction: column;
  }
  .toolbar {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .ranges,
  .modes {
    display: flex;
    gap: 0.25rem;
    flex-wrap: wrap;
  }
  button.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .message {
    margin: 0;
    font-size: 0.85rem;
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(420px, 1fr));
    gap: 1rem;
  }
  @media (max-width: 480px) {
    .charts {
      grid-template-columns: 1fr;
    }
  }
  .chart,
  .controls,
  .empty {
    padding: 1rem;
    min-width: 0;
  }
  .chart h2,
  .controls h2 {
    font-size: 0.95rem;
    margin-bottom: 0.5rem;
  }
  .unit {
    font-weight: 400;
    font-size: 0.85rem;
  }
  .controls label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin-top: 0.75rem;
    max-width: 260px;
    font-size: 0.85rem;
  }
  .empty p {
    margin: 0.5rem 0 0;
  }
</style>
