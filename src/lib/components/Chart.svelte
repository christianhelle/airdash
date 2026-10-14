<script lang="ts">
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import { onDestroy, onMount } from "svelte";
  import type { Unit } from "$lib/api";
  import { formatDateTime } from "$lib/format";
  import { convert, displayUnit, formatValue } from "$lib/sensors";

  let {
    points,
    sensorType,
    unit,
    rawUnit = "",
    height = 200,
  }: { points: [number, number][]; sensorType: string; unit: Unit; rawUnit?: string; height?: number } = $props();

  let container: HTMLDivElement;
  let tooltip: HTMLDivElement;
  let chart: uPlot | null = null;
  let themeVersion = $state(0);

  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  function build(width: number) {
    chart?.destroy();
    const xs = points.map(([t]) => t);
    const ys = points.map(([, v]) => convert(sensorType, v, unit));
    const axis = {
      stroke: css("--text-2"),
      grid: { stroke: css("--grid"), width: 1 },
      ticks: { show: false },
      font: "12px system-ui, sans-serif",
    };
    const opts: uPlot.Options = {
      width,
      height,
      legend: { show: false },
      cursor: { drag: { x: false, y: false }, points: { size: 8, fill: css("--series"), stroke: css("--surface"), width: 2 } },
      scales: { x: { time: true } },
      axes: [
        { ...axis },
        { ...axis, size: 56, values: (_u, vals) => vals.map((v) => (v == null ? "" : String(+v.toFixed(2)))) },
      ],
      series: [{}, { stroke: css("--series"), width: 2, points: { show: points.length < 3 } }],
      hooks: {
        setCursor: [
          (u) => {
            const i = u.cursor.idx;
            if (i == null || u.cursor.left == null || u.cursor.left < 0) {
              tooltip.style.display = "none";
              return;
            }
            const value = `${formatValue(sensorType, points[i][1], unit)} ${displayUnit(sensorType, unit, rawUnit)}`;
            tooltip.innerHTML = `<strong>${value}</strong><span>${formatDateTime(points[i][0] * 1000)}</span>`;
            tooltip.style.display = "flex";
            const left = Math.min(u.cursor.left + 12, width - tooltip.offsetWidth - 4);
            tooltip.style.left = `${Math.max(0, left)}px`;
            tooltip.style.top = `${Math.max(0, (u.cursor.top ?? 0) - 40)}px`;
          },
        ],
      },
    };
    chart = new uPlot(opts, [xs, ys], container);
  }

  $effect(() => {
    // Rebuild when data, units or theme change.
    void points;
    void unit;
    void themeVersion;
    if (container) build(container.clientWidth);
  });

  let resize: ResizeObserver;
  let themeObserver: MutationObserver;
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const onTheme = () => themeVersion++;

  onMount(() => {
    resize = new ResizeObserver(() => chart?.setSize({ width: container.clientWidth, height }));
    resize.observe(container);
    themeObserver = new MutationObserver(onTheme);
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    media.addEventListener("change", onTheme);
  });

  onDestroy(() => {
    resize?.disconnect();
    themeObserver?.disconnect();
    media.removeEventListener("change", onTheme);
    chart?.destroy();
  });
</script>

<div class="chart">
  <div bind:this={container}></div>
  <div class="tooltip" bind:this={tooltip}></div>
</div>

<style>
  .chart {
    position: relative;
  }
  .tooltip {
    position: absolute;
    display: none;
    flex-direction: column;
    pointer-events: none;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.3rem 0.5rem;
    font-size: 0.8rem;
    white-space: nowrap;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.15);
    z-index: 2;
  }
  .tooltip :global(span) {
    color: var(--text-2);
  }
</style>
