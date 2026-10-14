<script lang="ts">
  import type { Device, DeviceSensors, Unit } from "$lib/api";
  import { timeAgo } from "$lib/format";
  import { deviceTypeName, sortSensors, worstLevel } from "$lib/sensors";
  import LevelBadge from "./LevelBadge.svelte";
  import SensorTile from "./SensorTile.svelte";

  let {
    device,
    reading,
    unit,
    onopen,
  }: { device: Device; reading: DeviceSensors | undefined; unit: Unit; onopen: () => void } = $props();

  const sensors = $derived(sortSensors(reading?.sensors ?? [], (s) => s.sensorType));
  const level = $derived(worstLevel(sensors));
  const recorded = $derived(reading?.recorded ? Date.parse(reading.recorded) : null);
</script>

<button class="card device" onclick={onopen}>
  <header>
    <div class="title">
      <h3>{device.name || device.serialNumber}</h3>
      <span class="muted type">{deviceTypeName(device.type)}</span>
    </div>
    {#if level}<LevelBadge {level} />{/if}
  </header>

  {#if sensors.length > 0}
    <div class="tiles">
      {#each sensors as sensor (sensor.sensorType)}
        <SensorTile {sensor} {unit} />
      {/each}
    </div>
  {:else}
    <p class="muted empty">No readings yet.</p>
  {/if}

  <footer class="muted">
    <span>{recorded ? `Updated ${timeAgo(recorded)}` : "Waiting for first reading"}</span>
    {#if reading?.batteryPercentage != null}
      <span title="Battery">Battery {reading.batteryPercentage}%</span>
    {/if}
  </footer>
</button>

<style>
  .device {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    padding: 1rem;
    text-align: left;
    width: 100%;
  }
  .device:hover:not(:disabled) {
    background: var(--surface);
    border-color: var(--accent);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.5rem;
  }
  .title {
    display: flex;
    flex-direction: column;
  }
  h3 {
    font-size: 1.05rem;
  }
  .type {
    font-size: 0.82rem;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(118px, 1fr));
    gap: 0.5rem;
  }
  .empty {
    margin: 0;
  }
  footer {
    display: flex;
    justify-content: space-between;
    font-size: 0.8rem;
  }
</style>
