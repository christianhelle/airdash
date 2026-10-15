//! Sensor metadata: labels, display units, unit conversion and Airthings air-quality thresholds.

use airthings::api::Unit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Good,
    Fair,
    Poor,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Good => "Good",
            Level::Fair => "Fair",
            Level::Poor => "Poor",
        }
    }
}

struct SensorMeta {
    label: &'static str,
    /// Display unit as (metric, imperial). Values are always stored in metric.
    unit: (&'static str, &'static str),
    decimals: (usize, usize),
    to_imperial: Option<fn(f64) -> f64>,
    /// Level for a metric value, if the sensor has air-quality thresholds.
    level: Option<fn(f64) -> Level>,
}

const fn same(label: &'static str, unit: &'static str, level: Option<fn(f64) -> Level>) -> SensorMeta {
    SensorMeta { label, unit: (unit, unit), decimals: (0, 0), to_imperial: None, level }
}

fn below(v: f64, fair: f64, poor: f64) -> Level {
    if v < fair {
        Level::Good
    } else if v < poor {
        Level::Fair
    } else {
        Level::Poor
    }
}

// Thresholds follow the levels Airthings publishes for its devices.
fn meta(sensor_type: &str) -> Option<SensorMeta> {
    Some(match sensor_type {
        "radonShortTermAvg" => SensorMeta {
            label: "Radon",
            unit: ("Bq/m³", "pCi/L"),
            decimals: (0, 1),
            to_imperial: Some(|bq| bq / 37.0),
            level: Some(|v| below(v, 100.0, 150.0)),
        },
        "co2" => same("CO₂", "ppm", Some(|v| below(v, 800.0, 1000.0))),
        "voc" => same("VOC", "ppb", Some(|v| below(v, 250.0, 2000.0))),
        "pm25" => same("PM2.5", "µg/m³", Some(|v| below(v, 10.0, 25.0))),
        "pm1" => same("PM1", "µg/m³", Some(|v| below(v, 10.0, 25.0))),
        "humidity" => same(
            "Humidity",
            "%",
            Some(|v| match v {
                v if (30.0..60.0).contains(&v) => Level::Good,
                v if (25.0..70.0).contains(&v) => Level::Fair,
                _ => Level::Poor,
            }),
        ),
        "temp" => SensorMeta {
            label: "Temperature",
            unit: ("°C", "°F"),
            decimals: (1, 1),
            to_imperial: Some(|c| c * 9.0 / 5.0 + 32.0),
            level: Some(|v| if (18.0..25.0).contains(&v) { Level::Good } else { Level::Fair }),
        },
        "pressure" => SensorMeta {
            label: "Pressure",
            unit: ("hPa", "inHg"),
            decimals: (0, 2),
            to_imperial: Some(|hpa| hpa * 0.02953),
            level: None,
        },
        "mold" => same("Mold risk", "", Some(|v| below(v, 3.0, 5.0))),
        "virusRisk" => same("Virus risk", "", Some(|v| below(v, 3.0, 8.0))),
        "light" | "lux" => same("Light", "lux", None),
        "sound" | "soundLevelA" | "noise" => same("Sound", "dBA", None),
        "battery" => same("Battery", "%", None),
        _ => return None,
    })
}

/// Preferred tile order; unknown sensors follow alphabetically.
const ORDER: [&str; 10] = ["radonShortTermAvg", "pm25", "pm1", "co2", "voc", "humidity", "temp", "pressure", "mold", "virusRisk"];

pub fn sort_sensors<T>(items: &mut [T], key: impl Fn(&T) -> &str) {
    let rank = |t: &str| ORDER.iter().position(|o| *o == t).unwrap_or(ORDER.len());
    items.sort_by(|a, b| {
        let (a, b) = (key(a), key(b));
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });
}

pub fn sensor_label(sensor_type: &str) -> String {
    if let Some(m) = meta(sensor_type) {
        return m.label.to_owned();
    }
    // "outdoorTemp" -> "Outdoor Temp"
    let mut out = String::new();
    let mut prev_lower = false;
    for c in sensor_type.chars() {
        if prev_lower && c.is_ascii_uppercase() {
            out.push(' ');
        }
        prev_lower = c.is_ascii_lowercase();
        if out.is_empty() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn display_unit(sensor_type: &str, unit: Unit, raw_unit: &str) -> String {
    match meta(sensor_type) {
        Some(m) if unit == Unit::Imperial => m.unit.1.to_owned(),
        Some(m) => m.unit.0.to_owned(),
        None => raw_unit.to_owned(),
    }
}

pub fn convert(sensor_type: &str, metric_value: f64, unit: Unit) -> f64 {
    match meta(sensor_type).and_then(|m| m.to_imperial) {
        Some(f) if unit == Unit::Imperial => f(metric_value),
        _ => metric_value,
    }
}

pub fn format_value(sensor_type: &str, metric_value: f64, unit: Unit) -> String {
    let decimals = meta(sensor_type).map(|m| if unit == Unit::Imperial { m.decimals.1 } else { m.decimals.0 }).unwrap_or(1);
    format!("{:.*}", decimals, convert(sensor_type, metric_value, unit))
}

pub fn level_of(sensor_type: &str, metric_value: f64) -> Option<Level> {
    meta(sensor_type)?.level.map(|f| f(metric_value))
}

/// The worst level among a device's readings, for the card header.
pub fn worst_level<'a>(readings: impl IntoIterator<Item = (&'a str, f64)>) -> Option<Level> {
    readings.into_iter().filter_map(|(t, v)| level_of(t, v)).max()
}

pub fn device_type_name(device_type: &str) -> String {
    let known = match device_type {
        "WAVE" => "Wave",
        "WAVE_MINI" => "Wave Mini",
        "WAVE_PLUS" => "Wave Plus",
        "WAVE_RADON" | "WAVE_GEN2" => "Wave Radon",
        "WAVE_ENHANCE" => "Wave Enhance",
        "VIEW_PLUS" => "View Plus",
        "VIEW_POLLUTION" => "View Pollution",
        "VIEW_RADON" => "View Radon",
        "VIEW_CO2" => "View CO₂",
        "CORENTIUM_HOME_2" => "Corentium Home 2",
        "HOME" => "Corentium Home",
        "AP_1" => "Renew",
        "HUB" => "Hub",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_owned();
    }
    device_type
        .to_lowercase()
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_airthings_thresholds() {
        let cases = [
            ("radonShortTermAvg", 99.0, Level::Good),
            ("radonShortTermAvg", 100.0, Level::Fair),
            ("radonShortTermAvg", 150.0, Level::Poor),
            ("co2", 799.0, Level::Good),
            ("co2", 800.0, Level::Fair),
            ("co2", 1000.0, Level::Poor),
            ("voc", 249.0, Level::Good),
            ("voc", 2000.0, Level::Poor),
            ("pm25", 9.0, Level::Good),
            ("pm25", 25.0, Level::Poor),
            ("humidity", 45.0, Level::Good),
            ("humidity", 27.0, Level::Fair),
            ("humidity", 65.0, Level::Fair),
            ("humidity", 20.0, Level::Poor),
            ("humidity", 75.0, Level::Poor),
            ("temp", 21.0, Level::Good),
            ("temp", 16.0, Level::Fair),
        ];
        for (sensor, value, level) in cases {
            assert_eq!(level_of(sensor, value), Some(level), "{sensor} at {value}");
        }
    }

    #[test]
    fn no_level_without_thresholds() {
        assert_eq!(level_of("pressure", 1013.0), None);
        assert_eq!(level_of("somethingNew", 1.0), None);
    }

    #[test]
    fn converts_to_imperial() {
        assert_eq!(convert("temp", 20.0, Unit::Imperial), 68.0);
        assert_eq!(convert("radonShortTermAvg", 37.0, Unit::Imperial), 1.0);
        assert!((convert("pressure", 1000.0, Unit::Imperial) - 29.53).abs() < 0.001);
    }

    #[test]
    fn leaves_metric_and_unitless_sensors_alone() {
        assert_eq!(convert("temp", 20.0, Unit::Metric), 20.0);
        assert_eq!(convert("co2", 600.0, Unit::Imperial), 600.0);
    }

    #[test]
    fn formats_with_sensor_specific_precision() {
        assert_eq!(format_value("radonShortTermAvg", 50.0, Unit::Metric), "50");
        assert_eq!(format_value("radonShortTermAvg", 50.0, Unit::Imperial), "1.4");
        assert_eq!(format_value("temp", 21.27, Unit::Metric), "21.3");
    }

    #[test]
    fn picks_the_display_unit() {
        assert_eq!(display_unit("temp", Unit::Imperial, ""), "°F");
        assert_eq!(display_unit("unknown", Unit::Metric, "raw"), "raw");
    }

    #[test]
    fn labels_known_and_unknown_sensors() {
        assert_eq!(sensor_label("co2"), "CO₂");
        assert_eq!(sensor_label("outdoorTemp"), "Outdoor Temp");
    }

    #[test]
    fn names_device_types() {
        assert_eq!(device_type_name("WAVE_PLUS"), "Wave Plus");
        assert_eq!(device_type_name("SPACE_CO2_MINI"), "Space Co2 Mini");
    }

    #[test]
    fn worst_level_across_readings() {
        assert_eq!(worst_level([("co2", 500.0), ("radonShortTermAvg", 120.0)]), Some(Level::Fair));
        assert_eq!(worst_level([("pressure", 1000.0)]), None);
    }

    #[test]
    fn sorts_radon_first_and_unknown_sensors_last() {
        let mut sensors = ["temp", "zeta", "co2", "radonShortTermAvg", "alpha"];
        sort_sensors(&mut sensors, |s| s);
        assert_eq!(sensors, ["radonShortTermAvg", "co2", "temp", "alpha", "zeta"]);
    }
}
