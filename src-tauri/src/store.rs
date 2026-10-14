//! Local SQLite history. The Consumer API only serves the latest values, so every poll is recorded here.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::api::{Device, DeviceSensors, SensorValue};
use crate::error::Result;

/// Battery level is stored as a pseudo-sensor so it can be charted like the others.
pub const BATTERY_SENSOR: &str = "battery";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS devices (
    serial      TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    type        TEXT NOT NULL,
    home        TEXT,
    sensors     TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS readings (
    serial       TEXT NOT NULL,
    recorded     INTEGER NOT NULL,
    sensor_type  TEXT NOT NULL,
    value        REAL NOT NULL,
    unit         TEXT NOT NULL,
    PRIMARY KEY (serial, sensor_type, recorded)
) WITHOUT ROWID;
";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorInfo {
    pub sensor_type: String,
    pub unit: String,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn: Mutex::new(conn) })
    }

    /// Replace the device list with what the account currently has. Readings are kept.
    pub fn replace_devices(&self, devices: &[Device]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM devices", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO devices (serial, name, type, home, sensors, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let now = Utc::now().timestamp();
            for d in devices {
                let sensors = serde_json::to_string(&d.sensors).unwrap_or_else(|_| "[]".into());
                insert.execute(params![d.serial_number, d.name, d.device_type, d.home, sensors, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT serial, name, type, home, sensors FROM devices ORDER BY home, name")?;
        let rows = stmt.query_map([], |row| {
            let sensors: String = row.get(4)?;
            Ok(Device {
                serial_number: row.get(0)?,
                name: row.get(1)?,
                device_type: row.get(2)?,
                home: row.get(3)?,
                sensors: serde_json::from_str(&sensors).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Record a poll. Returns how many new readings were stored; repeated polls of the same sample add nothing.
    pub fn insert_readings(&self, results: &[DeviceSensors]) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut inserted = 0;
        {
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO readings (serial, recorded, sensor_type, value, unit) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for r in results {
                let Some(recorded) = r.recorded.as_deref().and_then(parse_timestamp) else { continue };
                for s in &r.sensors {
                    inserted += insert.execute(params![r.serial_number, recorded, s.sensor_type, s.value, s.unit])?;
                }
                if let Some(battery) = r.battery_percentage {
                    inserted += insert.execute(params![r.serial_number, recorded, BATTERY_SENSOR, battery as f64, "pct"])?;
                }
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// The most recent stored sample of every device, shaped like the API response.
    pub fn latest(&self) -> Result<Vec<DeviceSensors>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT r.serial, r.recorded, r.sensor_type, r.value, r.unit
             FROM readings r
             JOIN (SELECT serial, MAX(recorded) AS recorded FROM readings GROUP BY serial) m
               ON r.serial = m.serial AND r.recorded = m.recorded
             ORDER BY r.serial, r.sensor_type",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, f64>(3)?, row.get::<_, String>(4)?))
        })?;
        let mut out: Vec<DeviceSensors> = Vec::new();
        for row in rows {
            let (serial, recorded, sensor_type, value, unit) = row?;
            if out.last().map(|d| d.serial_number != serial).unwrap_or(true) {
                out.push(DeviceSensors {
                    serial_number: serial,
                    sensors: Vec::new(),
                    recorded: Some(format_timestamp(recorded)),
                    battery_percentage: None,
                });
            }
            let current = out.last_mut().unwrap();
            if sensor_type == BATTERY_SENSOR {
                current.battery_percentage = Some(value.round() as i32);
            } else {
                current.sensors.push(SensorValue { sensor_type, value, unit });
            }
        }
        Ok(out)
    }

    /// Sensors that have recorded history for a device.
    pub fn sensor_types(&self, serial: &str) -> Result<Vec<SensorInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT sensor_type, unit FROM readings WHERE serial = ?1 GROUP BY sensor_type ORDER BY sensor_type",
        )?;
        let rows = stmt.query_map([serial], |row| Ok(SensorInfo { sensor_type: row.get(0)?, unit: row.get(1)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Time series as `[epoch seconds, value]` pairs between `from` and `to` (inclusive).
    /// When there are more than `max_points` readings they are averaged into evenly sized buckets.
    pub fn series(&self, serial: &str, sensor_type: &str, from: i64, to: i64, max_points: usize) -> Result<Vec<(i64, f64)>> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM readings WHERE serial = ?1 AND sensor_type = ?2 AND recorded BETWEEN ?3 AND ?4",
            params![serial, sensor_type, from, to],
            |row| row.get(0),
        )?;
        // A bucket of one second groups nothing, so small ranges come back as raw readings.
        let max_points = max_points.max(1) as i64;
        let bucket = if count <= max_points { 1 } else { ((to - from) / max_points).max(1) };
        let mut stmt = conn.prepare(
            "SELECT (recorded / ?5) * ?5 + ?5 / 2 AS t, AVG(value) FROM readings
             WHERE serial = ?1 AND sensor_type = ?2 AND recorded BETWEEN ?3 AND ?4
             GROUP BY recorded / ?5 ORDER BY t",
        )?;
        let rows = stmt.query_map(params![serial, sensor_type, from, to, bucket], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// All readings of a device in a range as CSV with one row per sample and one column per sensor.
    pub fn export_csv(&self, serial: &str, from: i64, to: i64) -> Result<String> {
        let sensors: Vec<SensorInfo> = self.sensor_types(serial)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT recorded, sensor_type, value FROM readings
             WHERE serial = ?1 AND recorded BETWEEN ?2 AND ?3 ORDER BY recorded",
        )?;
        let rows = stmt.query_map(params![serial, from, to], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, f64>(2)?))
        })?;

        let mut csv = String::from("recorded");
        for s in &sensors {
            csv.push_str(&format!(",{} ({})", s.sensor_type, s.unit));
        }
        csv.push('\n');

        let mut current: Option<(i64, Vec<Option<f64>>)> = None;
        let flush = |csv: &mut String, (recorded, values): (i64, Vec<Option<f64>>)| {
            csv.push_str(&format_timestamp(recorded));
            for v in values {
                csv.push(',');
                if let Some(v) = v {
                    csv.push_str(&v.to_string());
                }
            }
            csv.push('\n');
        };
        for row in rows {
            let (recorded, sensor_type, value) = row?;
            if current.as_ref().map(|(t, _)| *t != recorded).unwrap_or(true) {
                if let Some(done) = current.take() {
                    flush(&mut csv, done);
                }
                current = Some((recorded, vec![None; sensors.len()]));
            }
            if let Some(i) = sensors.iter().position(|s| s.sensor_type == sensor_type) {
                current.as_mut().unwrap().1[i] = Some(value);
            }
        }
        if let Some(done) = current {
            flush(&mut csv, done);
        }
        Ok(csv)
    }

    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("DELETE FROM readings; DELETE FROM devices;")?;
        Ok(())
    }
}

/// Accepts RFC 3339 and the zone-less ISO form; zone-less times are treated as UTC.
fn parse_timestamp(text: &str) -> Option<i64> {
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Some(t.timestamp());
    }
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|t| t.and_utc().timestamp())
}

fn format_timestamp(epoch: i64) -> String {
    DateTime::<Utc>::from_timestamp(epoch, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(serial: &str, recorded: &str, co2: f64, battery: Option<i32>) -> DeviceSensors {
        DeviceSensors {
            serial_number: serial.into(),
            sensors: vec![SensorValue { sensor_type: "co2".into(), value: co2, unit: "ppm".into() }],
            recorded: Some(recorded.into()),
            battery_percentage: battery,
        }
    }

    fn device(serial: &str, name: &str) -> Device {
        Device { serial_number: serial.into(), home: None, name: name.into(), device_type: "WAVE_PLUS".into(), sensors: vec!["co2".into()] }
    }

    #[test]
    fn repeated_polls_of_the_same_sample_are_ignored() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.insert_readings(&[sample("A", "2026-10-14T10:00:00Z", 600.0, Some(80))]).unwrap(), 2);
        assert_eq!(store.insert_readings(&[sample("A", "2026-10-14T10:00:00Z", 600.0, Some(80))]).unwrap(), 0);
        assert_eq!(store.insert_readings(&[sample("A", "2026-10-14T10:05:00Z", 610.0, None)]).unwrap(), 1);
    }

    #[test]
    fn samples_without_timestamp_are_skipped() {
        let store = Store::open_in_memory().unwrap();
        let mut s = sample("A", "", 600.0, None);
        s.recorded = None;
        assert_eq!(store.insert_readings(&[s]).unwrap(), 0);
    }

    #[test]
    fn latest_returns_newest_sample_per_device_with_battery() {
        let store = Store::open_in_memory().unwrap();
        store.insert_readings(&[sample("A", "2026-10-14T10:00:00Z", 600.0, Some(80)), sample("B", "2026-10-14T10:00:00Z", 400.0, None)]).unwrap();
        store.insert_readings(&[sample("A", "2026-10-14T10:05:00Z", 650.0, Some(79))]).unwrap();

        let latest = store.latest().unwrap();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].serial_number, "A");
        assert_eq!(latest[0].recorded.as_deref(), Some("2026-10-14T10:05:00Z"));
        assert_eq!(latest[0].sensors[0].value, 650.0);
        assert_eq!(latest[0].battery_percentage, Some(79));
        assert_eq!(latest[1].battery_percentage, None);
    }

    #[test]
    fn series_returns_raw_points_in_range() {
        let store = Store::open_in_memory().unwrap();
        for (i, v) in [500.0, 600.0, 700.0].iter().enumerate() {
            store.insert_readings(&[sample("A", &format!("2026-10-14T10:0{i}:00Z"), *v, None)]).unwrap();
        }
        let from = parse_timestamp("2026-10-14T10:01:00Z").unwrap();
        let to = parse_timestamp("2026-10-14T10:02:00Z").unwrap();
        let series = store.series("A", "co2", from, to, 100).unwrap();
        assert_eq!(series, vec![(from, 600.0), (to, 700.0)]);
    }

    #[test]
    fn series_averages_into_buckets_when_too_many_points() {
        let store = Store::open_in_memory().unwrap();
        let base = 1_700_000_000;
        let rows: Vec<_> = (0..100)
            .map(|i| sample("A", &format_timestamp(base + i * 60), i as f64, None))
            .collect();
        store.insert_readings(&rows).unwrap();

        let series = store.series("A", "co2", base, base + 99 * 60, 10).unwrap();
        assert!(series.len() <= 11, "got {} points", series.len());
        let mean: f64 = series.iter().map(|(_, v)| v).sum::<f64>() / series.len() as f64;
        assert!((mean - 49.5).abs() < 5.0);
    }

    #[test]
    fn replace_devices_keeps_history() {
        let store = Store::open_in_memory().unwrap();
        store.replace_devices(&[device("A", "Kitchen"), device("B", "Bedroom")]).unwrap();
        store.insert_readings(&[sample("A", "2026-10-14T10:00:00Z", 600.0, None)]).unwrap();
        store.replace_devices(&[device("B", "Bedroom")]).unwrap();

        assert_eq!(store.devices().unwrap(), vec![device("B", "Bedroom")]);
        assert_eq!(store.sensor_types("A").unwrap().len(), 1);
    }

    #[test]
    fn exports_one_row_per_sample() {
        let store = Store::open_in_memory().unwrap();
        store.insert_readings(&[sample("A", "2026-10-14T10:00:00Z", 600.0, Some(80))]).unwrap();
        store.insert_readings(&[sample("A", "2026-10-14T10:05:00Z", 610.0, None)]).unwrap();

        let csv = store.export_csv("A", 0, i64::MAX).unwrap();
        assert_eq!(
            csv,
            "recorded,battery (pct),co2 (ppm)\n2026-10-14T10:00:00Z,80,600\n2026-10-14T10:05:00Z,,610\n"
        );
    }

    #[test]
    fn parses_zone_less_timestamps_as_utc() {
        assert_eq!(parse_timestamp("2026-10-14T10:00:00"), parse_timestamp("2026-10-14T10:00:00Z"));
        assert_eq!(parse_timestamp("not a date"), None);
    }
}
