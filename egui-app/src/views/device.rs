//! One device: history charts for every sensor over a chosen time range.

use std::collections::HashMap;

use airdash_core::api::{Device, RemoteControlState, Unit};
use airdash_core::store::SensorInfo;
use egui::{RichText, Ui};

use airdash_core::error::Result;

use crate::backend::{Backend, Task};
use crate::chart;
use crate::format::safe_file_name;
use crate::sensors::{device_type_name, display_unit, sensor_label, sort_sensors};
use crate::widgets;

const MAX_POINTS: usize = 600;
/// The Airthings Renew air purifier.
const RENEW: &str = "AP_1";
const MODES: [&str; 5] = ["AUTO", "SLEEP", "BOOST", "MANUAL", "OFF"];
const DEFAULT_FAN_SPEED: u8 = 3;

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
    export: Option<Task<Result<Option<String>>>>,
    control: Option<RemoteControlState>,
    control_fetch: Option<Task<Result<Option<RemoteControlState>>>>,
    /// A mode change on its way to Airthings, and the state it sets.
    control_send: Option<(Task<Result<()>>, RemoteControlState)>,
    fan_speed: u8,
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
            export: None,
            control: None,
            control_fetch: None,
            control_send: None,
            fan_speed: DEFAULT_FAN_SPEED,
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
        if let Some(result) = self.export.as_ref().and_then(Task::ready) {
            self.export = None;
            self.message = match result {
                Ok(Some(path)) => format!("Saved {path}"),
                Ok(None) => String::new(),
                Err(e) => e.to_string(),
            };
        }
        if self.version.is_none() && device.device_type == RENEW {
            self.control_fetch = Some(backend.remote_control(&self.serial));
        }
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !self.sensors.is_empty() && self.export.is_none();
                if ui.add_enabled(enabled, egui::Button::new("Export CSV")).clicked() {
                    let (from, to) = self.range.bounds(chrono::Utc::now().timestamp());
                    let file_name = format!("{}-{}.csv", safe_file_name(&device.name), self.range.id());
                    self.export = Some(backend.export_csv(&self.serial, file_name, from, to));
                }
            });
        });
        if !self.message.is_empty() {
            ui.weak(&self.message);
        }
        ui.add_space(8.0);

        if device.device_type == RENEW {
            self.purifier_controls(ui, backend);
            ui.add_space(8.0);
        }

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

    fn purifier_controls(&mut self, ui: &mut Ui, backend: &Backend) {
        if let Some(result) = self.control_fetch.as_ref().and_then(Task::ready) {
            self.control_fetch = None;
            match result {
                Ok(control) => {
                    self.fan_speed = control.as_ref().and_then(|c| c.fan_speed).unwrap_or(DEFAULT_FAN_SPEED);
                    self.control = control;
                }
                Err(e) => self.message = e.to_string(),
            }
        }
        if let Some(result) = self.control_send.as_ref().and_then(|(task, _)| task.ready()) {
            let (_, sent) = self.control_send.take().unwrap();
            self.message = match result {
                Ok(()) => {
                    self.control = Some(sent);
                    "Sent to the purifier. It applies the change the next time it syncs.".into()
                }
                Err(e) => e.to_string(),
            };
        }

        let busy = self.control_send.is_some();
        let mut send = None;
        widgets::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Purifier mode").strong());
            ui.horizontal_wrapped(|ui| {
                for mode in MODES {
                    let active = self.control.as_ref().is_some_and(|c| c.mode == mode);
                    if ui.add_enabled(!busy, egui::Button::selectable(active, mode_label(mode))).clicked() {
                        send = Some(mode);
                    }
                }
            });
            if self.control.as_ref().is_some_and(|c| c.mode == "MANUAL") {
                let slider = ui.add_enabled(!busy, egui::Slider::new(&mut self.fan_speed, 1..=5).text("Fan speed"));
                if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                    send = Some("MANUAL");
                }
            }
        });
        if let Some(mode) = send {
            let fan_speed = (mode == "MANUAL").then_some(self.fan_speed);
            let state = RemoteControlState { mode: mode.into(), fan_speed };
            self.control_send = Some((backend.set_remote_control(&self.serial, state.clone()), state));
        }
    }
}

/// "AUTO" -> "Auto"
fn mode_label(mode: &str) -> String {
    let lower = mode.to_lowercase();
    let mut chars = lower.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_purifier_modes() {
        assert_eq!(mode_label("MANUAL"), "Manual");
        assert_eq!(mode_label("OFF"), "Off");
    }

    #[test]
    fn ranges_end_now() {
        assert_eq!(Range::Day.bounds(1_000_000), (1_000_000 - 86_400, 1_000_000));
        assert_eq!(Range::Year.bounds(100_000_000), (100_000_000 - 365 * 86_400, 100_000_000));
        assert_eq!(Range::All.bounds(5), (0, 5));
    }
}
