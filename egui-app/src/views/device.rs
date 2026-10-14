//! One device: history charts for every sensor over a chosen time range.

use std::collections::HashMap;

use airdash_core::api::{Device, Unit};
use airdash_core::store::SensorInfo;
use egui::{RichText, Ui};

use crate::backend::Backend;
use crate::chart;
use crate::sensors::{device_type_name, display_unit, sensor_label, sort_sensors};
use crate::widgets;

const MAX_POINTS: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Day,
    Week,
    Month,
    Year,
    All,
}

impl Range {
    pub const ALL: [Range; 5] = [Range::Day, Range::Week, Range::Month, Range::Year, Range::All];

    pub fn id(self) -> &'static str {
        match self {
            Range::Day => "24h",
            Range::Week => "7d",
            Range::Month => "30d",
            Range::Year => "1y",
            Range::All => "all",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Range::Day => "24 hours",
            Range::Week => "7 days",
            Range::Month => "30 days",
            Range::Year => "1 year",
            Range::All => "All",
        }
    }

    /// `(from, to)` in epoch seconds, ending at `now`.
    pub fn bounds(self, now: i64) -> (i64, i64) {
        let days = match self {
            Range::Day => 1,
            Range::Week => 7,
            Range::Month => 30,
            Range::Year => 365,
            Range::All => return (0, now),
        };
        (now - days * 86_400, now)
    }
}

pub struct DeviceView {
    pub serial: String,
    range: Range,
    sensors: Vec<SensorInfo>,
    series: HashMap<String, Vec<(i64, f64)>>,
    loaded: bool,
    message: String,
    /// The readings version the charts were loaded for.
    version: Option<u64>,
}

impl DeviceView {
    pub fn new(serial: String) -> Self {
        DeviceView {
            serial,
            range: Range::Day,
            sensors: Vec::new(),
            series: HashMap::new(),
            loaded: false,
            message: String::new(),
            version: None,
        }
    }

    fn load(&mut self, backend: &Backend) {
        let (from, to) = self.range.bounds(chrono::Utc::now().timestamp());
        let store = &backend.store;
        let result = store.sensor_types(&self.serial).and_then(|mut types| {
            sort_sensors(&mut types, |s| &s.sensor_type);
            let series = types
                .iter()
                .map(|s| Ok((s.sensor_type.clone(), store.series(&self.serial, &s.sensor_type, from, to, MAX_POINTS)?)))
                .collect::<airdash_core::error::Result<HashMap<_, _>>>()?;
            Ok((types, series))
        });
        match result {
            Ok((sensors, series)) => {
                self.sensors = sensors;
                self.series = series;
            }
            Err(e) => self.message = e.to_string(),
        }
        self.loaded = true;
    }

    /// `version` changes whenever new readings arrive. Returns true when the user goes back.
    pub fn ui(&mut self, ui: &mut Ui, backend: &Backend, device: &Device, unit: Unit, version: u64) -> bool {
        if self.version != Some(version) {
            self.version = Some(version);
            self.load(backend);
        }

        let back = ui.link("⬅ All devices").clicked();
        ui.label(RichText::new(if device.name.is_empty() { &device.serial_number } else { &device.name }).size(22.0).strong());
        ui.weak(format!("{} · {}", device_type_name(&device.device_type), device.serial_number));
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            for range in Range::ALL {
                if ui.selectable_label(self.range == range, range.label()).clicked() && self.range != range {
                    self.range = range;
                    self.load(backend);
                }
            }
        });
        if !self.message.is_empty() {
            ui.weak(&self.message);
        }
        ui.add_space(8.0);

        if self.loaded && self.sensors.is_empty() {
            widgets::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.heading("No history yet");
                ui.weak(
                    "Airthings only shares the latest values, so AirDash builds history by recording every update \
                     while it runs. Charts fill in over time. Keep AirDash running in the tray to keep recording.",
                );
            });
        }

        widgets::grid(ui, &self.sensors, 420.0, 16.0, |ui, sensor| {
            widgets::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(sensor_label(&sensor.sensor_type)).strong());
                    ui.weak(display_unit(&sensor.sensor_type, unit, &sensor.unit));
                });
                let points = self.series.get(&sensor.sensor_type).map(Vec::as_slice).unwrap_or_default();
                if points.is_empty() {
                    ui.weak("No readings in this range.");
                } else {
                    let id = format!("{}-{}", self.serial, sensor.sensor_type);
                    chart::show(ui, &id, points, &sensor.sensor_type, unit, &sensor.unit);
                }
            });
        });
        back
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_end_now() {
        assert_eq!(Range::Day.bounds(1_000_000), (1_000_000 - 86_400, 1_000_000));
        assert_eq!(Range::Year.bounds(100_000_000), (100_000_000 - 365 * 86_400, 100_000_000));
        assert_eq!(Range::All.bounds(5), (0, 5));
    }
}
