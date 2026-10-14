import { describe, expect, it } from "vitest";
import { convert, deviceTypeName, displayUnit, formatValue, levelOf, sensorLabel, sortSensors, worstLevel } from "./sensors";

describe("levelOf", () => {
  it.each([
    ["radonShortTermAvg", 99, "good"],
    ["radonShortTermAvg", 100, "fair"],
    ["radonShortTermAvg", 150, "poor"],
    ["co2", 799, "good"],
    ["co2", 800, "fair"],
    ["co2", 1000, "poor"],
    ["voc", 249, "good"],
    ["voc", 2000, "poor"],
    ["pm25", 9, "good"],
    ["pm25", 25, "poor"],
    ["humidity", 45, "good"],
    ["humidity", 27, "fair"],
    ["humidity", 65, "fair"],
    ["humidity", 20, "poor"],
    ["humidity", 75, "poor"],
    ["temp", 21, "good"],
    ["temp", 16, "fair"],
  ])("%s at %d is %s", (type, value, level) => {
    expect(levelOf(type, value)).toBe(level);
  });

  it("has no level for sensors without thresholds", () => {
    expect(levelOf("pressure", 1013)).toBeNull();
    expect(levelOf("somethingNew", 1)).toBeNull();
  });
});

describe("unit conversion", () => {
  it("converts to imperial", () => {
    expect(convert("temp", 20, "imperial")).toBe(68);
    expect(convert("radonShortTermAvg", 37, "imperial")).toBe(1);
    expect(convert("pressure", 1000, "imperial")).toBeCloseTo(29.53);
  });

  it("leaves metric and unit-less sensors alone", () => {
    expect(convert("temp", 20, "metric")).toBe(20);
    expect(convert("co2", 600, "imperial")).toBe(600);
  });

  it("formats with sensor-specific precision", () => {
    expect(formatValue("radonShortTermAvg", 50, "metric")).toBe("50");
    expect(formatValue("radonShortTermAvg", 50, "imperial")).toBe("1.4");
    expect(formatValue("temp", 21.27, "metric")).toBe("21.3");
  });

  it("picks the display unit", () => {
    expect(displayUnit("temp", "imperial")).toBe("°F");
    expect(displayUnit("unknown", "metric", "raw")).toBe("raw");
  });
});

describe("labels", () => {
  it("labels known and unknown sensors", () => {
    expect(sensorLabel("co2")).toBe("CO₂");
    expect(sensorLabel("outdoorTemp")).toBe("Outdoor Temp");
  });

  it("names device types", () => {
    expect(deviceTypeName("WAVE_PLUS")).toBe("Wave Plus");
    expect(deviceTypeName("SPACE_CO2_MINI")).toBe("Space Co2 Mini");
  });
});

describe("worstLevel", () => {
  it("returns the worst level across readings", () => {
    expect(worstLevel([{ sensorType: "co2", value: 500 }, { sensorType: "radonShortTermAvg", value: 120 }])).toBe("fair");
    expect(worstLevel([{ sensorType: "pressure", value: 1000 }])).toBeNull();
  });
});

describe("sortSensors", () => {
  it("puts radon first and unknown sensors last", () => {
    const sorted = sortSensors(["temp", "zeta", "co2", "radonShortTermAvg", "alpha"], (s) => s);
    expect(sorted).toEqual(["radonShortTermAvg", "co2", "temp", "alpha", "zeta"]);
  });
});
