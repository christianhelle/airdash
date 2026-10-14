//! Every device on the account, grouped by home, with its latest values.

use airdash_core::api::{Device, DeviceSensors, Unit};
use egui::{RichText, Sense, Ui, UiBuilder};

use crate::format::time_ago;
use crate::sensors::{device_type_name, sort_sensors, worst_level};
use crate::widgets;

const DEFAULT_HOME: &str = "Devices";

/// Devices grouped by home, keeping the store's order.
pub fn group_by_home(devices: &[Device]) -> Vec<(String, Vec<&Device>)> {
    let mut groups: Vec<(String, Vec<&Device>)> = Vec::new();
    for device in devices {
        let home = device.home.as_deref().map(str::trim).filter(|h| !h.is_empty()).unwrap_or(DEFAULT_HOME);
        match groups.iter_mut().find(|(h, _)| h == home) {
            Some((_, list)) => list.push(device),
            None => groups.push((home.to_owned(), vec![device])),
        }
    }
    groups
}

pub fn parse_recorded(recorded: Option<&str>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(recorded?).ok().map(|t| t.timestamp())
}

/// Returns the serial number of a device the user opened.
pub fn ui(ui: &mut Ui, devices: &[Device], readings: &[DeviceSensors], loaded: bool, unit: Unit) -> Option<String> {
    let mut opened = None;
    if devices.is_empty() {
        widgets::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                if loaded {
                    ui.heading("No devices yet");
                    ui.weak(
                        "Devices registered to your Airthings account appear here after the first update. \
                         Use Refresh in the status bar to check now.",
                    );
                } else {
                    ui.weak("Loading…");
                }
            });
        });
        return None;
    }

    let groups = group_by_home(devices);
    let show_headings = groups.len() > 1 || groups[0].0 != DEFAULT_HOME;
    let now = chrono::Utc::now().timestamp();
    for (home, devices) in &groups {
        if show_headings {
            ui.label(RichText::new(home).size(17.0).strong());
        }
        widgets::grid(ui, devices, 340.0, 16.0, |ui, device| {
            let reading = readings.iter().find(|r| r.serial_number == device.serial_number);
            if device_card(ui, device, reading, unit, now) {
                opened = Some(device.serial_number.clone());
            }
        });
        ui.add_space(12.0);
    }
    opened
}

/// Returns true when clicked.
fn device_card(ui: &mut Ui, device: &Device, reading: Option<&DeviceSensors>, unit: Unit, now: i64) -> bool {
    let mut sensors: Vec<_> = reading.map(|r| r.sensors.iter().collect()).unwrap_or_default();
    sort_sensors(&mut sensors, |s| &s.sensor_type);
    let level = worst_level(sensors.iter().map(|s| (s.sensor_type.as_str(), s.value)));
    let recorded = reading.and_then(|r| parse_recorded(r.recorded.as_deref()));

    let response = ui
        .scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
            widgets::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        let name = if device.name.is_empty() { &device.serial_number } else { &device.name };
                        ui.label(RichText::new(name).size(16.0).strong());
                        ui.weak(device_type_name(&device.device_type));
                    });
                    if let Some(level) = level {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| widgets::level_badge(ui, level));
                    }
                });
                ui.add_space(6.0);
                if sensors.is_empty() {
                    ui.weak("No readings yet.");
                } else {
                    widgets::grid(ui, &sensors, 118.0, 8.0, |ui, sensor| widgets::sensor_tile(ui, sensor, unit));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let updated = match recorded {
                        Some(t) => format!("Updated {}", time_ago(t, now)),
                        None => "Waiting for first reading".into(),
                    };
                    ui.label(RichText::new(updated).size(12.0).weak());
                    if let Some(battery) = reading.and_then(|r| r.battery_percentage) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("Battery {battery}%")).size(12.0).weak());
                        });
                    }
                });
            });
        })
        .response;
    if response.hovered() {
        ui.painter().rect_stroke(
            response.rect,
            10.0,
            egui::Stroke::new(1.0, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(serial: &str, home: Option<&str>) -> Device {
        Device { serial_number: serial.into(), home: home.map(Into::into), name: serial.into(), device_type: "WAVE_PLUS".into(), sensors: vec![] }
    }

    #[test]
    fn groups_devices_by_home_in_order() {
        let devices = [device("A", Some("Cabin")), device("B", None), device("C", Some("Cabin")), device("D", Some(" "))];
        let groups = group_by_home(&devices);
        let names: Vec<_> = groups.iter().map(|(h, d)| (h.as_str(), d.iter().map(|d| d.serial_number.as_str()).collect::<Vec<_>>())).collect();
        assert_eq!(names, vec![("Cabin", vec!["A", "C"]), ("Devices", vec!["B", "D"])]);
    }

    #[test]
    fn parses_recorded_timestamps() {
        assert_eq!(parse_recorded(Some("2026-10-14T10:00:00Z")), Some(1_791_972_000));
        assert_eq!(parse_recorded(Some("garbage")), None);
        assert_eq!(parse_recorded(None), None);
    }
}
