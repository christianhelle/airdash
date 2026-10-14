<script lang="ts">
  import type { SensorValue, Unit } from "$lib/api";
  import { displayUnit, formatValue, levelOf, sensorLabel } from "$lib/sensors";
  import LevelBadge from "./LevelBadge.svelte";

  let { sensor, unit }: { sensor: SensorValue; unit: Unit } = $props();
  const level = $derived(levelOf(sensor.sensorType, sensor.value));
</script>

<div class="tile {level ?? ''}">
  <div class="label">{sensorLabel(sensor.sensorType)}</div>
  <div class="value">
    {formatValue(sensor.sensorType, sensor.value, unit)}
    <span class="unit">{displayUnit(sensor.sensorType, unit, sensor.unit)}</span>
  </div>
  {#if level}<LevelBadge {level} />{/if}
</div>

<style>
  .tile {
    border: 1px solid var(--border);
    border-left: 4px solid var(--border);
    border-radius: 8px;
    padding: 0.55rem 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .tile.good {
    border-left-color: var(--good);
  }
  .tile.fair {
    border-left-color: var(--fair);
  }
  .tile.poor {
    border-left-color: var(--poor);
  }
  .label {
    font-size: 0.8rem;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .value {
    font-size: 1.35rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .unit {
    font-size: 0.8rem;
    font-weight: 400;
    color: var(--text-2);
  }
</style>
