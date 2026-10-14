<script lang="ts">
  import type { Device, DeviceSensors, Unit } from "$lib/api";
  import DeviceCard from "$lib/components/DeviceCard.svelte";

  let {
    devices,
    readings,
    unit,
    loaded,
    onopen,
  }: {
    devices: Device[];
    readings: DeviceSensors[];
    unit: Unit;
    loaded: boolean;
    onopen: (device: Device) => void;
  } = $props();

  const byHome = $derived.by(() => {
    const groups = new Map<string, Device[]>();
    for (const d of devices) {
      const home = d.home?.trim() || "Devices";
      groups.set(home, [...(groups.get(home) ?? []), d]);
    }
    return [...groups.entries()];
  });

  const readingFor = (serial: string) => readings.find((r) => r.serialNumber === serial);
</script>

<div class="dashboard">
  {#if devices.length === 0}
    <div class="empty card">
      {#if loaded}
        <h2>No devices yet</h2>
        <p class="muted">
          Devices registered to your Airthings account appear here after the first update. Use Refresh in the status
          bar to check now.
        </p>
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    </div>
  {/if}

  {#each byHome as [home, homeDevices] (home)}
    <section>
      {#if byHome.length > 1 || home !== "Devices"}<h2>{home}</h2>{/if}
      <div class="grid">
        {#each homeDevices as device (device.serialNumber)}
          <DeviceCard {device} reading={readingFor(device.serialNumber)} {unit} onopen={() => onopen(device)} />
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .dashboard {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  h2 {
    font-size: 1.05rem;
    color: var(--text-2);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 1rem;
  }
  @media (max-width: 420px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  .empty {
    padding: 2rem;
    text-align: center;
  }
  .empty p {
    margin: 0.5rem 0 0;
  }
</style>
