// Sensor metadata: labels, display units, unit conversion and Airthings air-quality thresholds.
import type { Unit } from "./api";

export type Level = "good" | "fair" | "poor";

interface SensorMeta {
  label: string;
  /** Display unit per unit system. Values are always stored in metric. */
  unit: { metric: string; imperial: string };
  decimals: { metric: number; imperial: number };
  toImperial?: (metric: number) => number;
  /** Level for a metric value, if the sensor has air-quality thresholds. */
  level?: (metric: number) => Level;
}

const below = (fair: number, poor: number) => (v: number): Level =>
  v < fair ? "good" : v < poor ? "fair" : "poor";

const same = (unit: string, decimals = 0) => ({
  unit: { metric: unit, imperial: unit },
  decimals: { metric: decimals, imperial: decimals },
});

// Thresholds follow the levels Airthings publishes for its devices.
export const SENSORS: Record<string, SensorMeta> = {
  radonShortTermAvg: {
    label: "Radon",
    unit: { metric: "Bq/m³", imperial: "pCi/L" },
    decimals: { metric: 0, imperial: 1 },
    toImperial: (bq) => bq / 37,
    level: below(100, 150),
  },
  co2: { label: "CO₂", ...same("ppm"), level: below(800, 1000) },
  voc: { label: "VOC", ...same("ppb"), level: below(250, 2000) },
  pm25: { label: "PM2.5", ...same("µg/m³"), level: below(10, 25) },
  pm1: { label: "PM1", ...same("µg/m³"), level: below(10, 25) },
  humidity: {
    label: "Humidity",
    ...same("%"),
    level: (v) => (v >= 30 && v < 60 ? "good" : v >= 25 && v < 70 ? "fair" : "poor"),
  },
  temp: {
    label: "Temperature",
    unit: { metric: "°C", imperial: "°F" },
    decimals: { metric: 1, imperial: 1 },
    toImperial: (c) => (c * 9) / 5 + 32,
    level: (v) => (v >= 18 && v < 25 ? "good" : "fair"),
  },
  pressure: {
    label: "Pressure",
    unit: { metric: "hPa", imperial: "inHg" },
    decimals: { metric: 0, imperial: 2 },
    toImperial: (hpa) => hpa * 0.02953,
  },
  mold: { label: "Mold risk", ...same(""), level: (v) => (v < 3 ? "good" : v < 5 ? "fair" : "poor") },
  virusRisk: { label: "Virus risk", ...same(""), level: (v) => (v < 3 ? "good" : v < 8 ? "fair" : "poor") },
  light: { label: "Light", ...same("lux") },
  lux: { label: "Light", ...same("lux") },
  sound: { label: "Sound", ...same("dBA") },
  soundLevelA: { label: "Sound", ...same("dBA") },
  noise: { label: "Sound", ...same("dBA") },
  battery: { label: "Battery", ...same("%") },
};

/** Preferred tile order; unknown sensors follow alphabetically. */
const ORDER = ["radonShortTermAvg", "pm25", "pm1", "co2", "voc", "humidity", "temp", "pressure", "mold", "virusRisk"];

export function sortSensors<T>(items: T[], key: (item: T) => string): T[] {
  const rank = (t: string) => {
    const i = ORDER.indexOf(t);
    return i === -1 ? ORDER.length : i;
  };
  return [...items].sort((a, b) => rank(key(a)) - rank(key(b)) || key(a).localeCompare(key(b)));
}

export function sensorLabel(type: string): string {
  return SENSORS[type]?.label ?? type.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/^./, (c) => c.toUpperCase());
}

export function displayUnit(type: string, unit: Unit, rawUnit = ""): string {
  return SENSORS[type]?.unit[unit] ?? rawUnit;
}

export function convert(type: string, metricValue: number, unit: Unit): number {
  const fn = SENSORS[type]?.toImperial;
  return unit === "imperial" && fn ? fn(metricValue) : metricValue;
}

export function formatValue(type: string, metricValue: number, unit: Unit): string {
  const decimals = SENSORS[type]?.decimals[unit] ?? 1;
  return convert(type, metricValue, unit).toFixed(decimals);
}

export function levelOf(type: string, metricValue: number): Level | null {
  return SENSORS[type]?.level?.(metricValue) ?? null;
}

export const LEVEL_LABEL: Record<Level, string> = { good: "Good", fair: "Fair", poor: "Poor" };

/** The worst level among a device's readings, for the card header. */
export function worstLevel(sensors: { sensorType: string; value: number }[]): Level | null {
  const order: Level[] = ["good", "fair", "poor"];
  let worst: Level | null = null;
  for (const s of sensors) {
    const l = levelOf(s.sensorType, s.value);
    if (l && (worst === null || order.indexOf(l) > order.indexOf(worst))) worst = l;
  }
  return worst;
}

const DEVICE_NAMES: Record<string, string> = {
  WAVE: "Wave",
  WAVE_MINI: "Wave Mini",
  WAVE_PLUS: "Wave Plus",
  WAVE_RADON: "Wave Radon",
  WAVE_GEN2: "Wave Radon",
  WAVE_ENHANCE: "Wave Enhance",
  VIEW_PLUS: "View Plus",
  VIEW_POLLUTION: "View Pollution",
  VIEW_RADON: "View Radon",
  VIEW_CO2: "View CO₂",
  CORENTIUM_HOME_2: "Corentium Home 2",
  HOME: "Corentium Home",
  AP_1: "Renew",
  HUB: "Hub",
};

export function deviceTypeName(type: string): string {
  return DEVICE_NAMES[type] ?? type.toLowerCase().replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}
