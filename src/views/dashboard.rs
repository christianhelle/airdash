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

/// `serials` with `dragged` moved next to `target`: just before it, or just after it when `after` is set.
pub fn reorder(serials: &[String], dragged: &str, target: &str, after: bool) -> Vec<String> {
    if dragged == target || !serials.iter().any(|s| s == dragged) {
        return serials.to_vec();
    }
    let mut order: Vec<String> = serials.iter().filter(|s| *s != dragged).cloned().collect();
    let at = order.iter().position(|s| s == target).map(|i| if after { i + 1 } else { i }).unwrap_or(order.len());
    order.insert(at, dragged.to_owned());
    order
}

/// Accepts RFC 3339 and the zone-less ISO form the API sends; zone-less times are UTC.
pub fn parse_recorded(recorded: Option<&str>) -> Option<i64> {
    let recorded = recorded?;
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(recorded) {
        return Some(t.timestamp());
    }
    chrono::NaiveDateTime::parse_from_str(recorded, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|t| t.and_utc().timestamp())
}

/// What is being dragged. Devices can only move within their own home.
struct DraggedDevice {
    serial: String,
    home: String,
}

/// Gives the full new order of serial numbers when the user dragged a device to a new place.
pub fn ui(ui: &mut Ui, devices: &[Device], readings: &[DeviceSensors], loaded: bool, unit: Unit) -> Option<Vec<String>> {
    let mut reordered = None;
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
    for (home, home_devices) in &groups {
        if show_headings {
            ui.label(RichText::new(home).size(17.0).strong());
        }
        widgets::grid(ui, home_devices, 340.0, 16.0, |ui, device| {
            let reading = readings.iter().find(|r| r.serial_number == device.serial_number);
            if let Some(Dropped { serial, after }) = device_card(ui, device, home, reading, unit, now) {
                let serials: Vec<String> = devices.iter().map(|d| d.serial_number.clone()).collect();
                reordered = Some(reorder(&serials, &serial, &device.serial_number, after));
            }
        });
        ui.add_space(12.0);
    }
    reordered
}

/// Another device was dropped on a card, to go before or after it.
struct Dropped {
    serial: String,
    after: bool,
}

fn device_card(ui: &mut Ui, device: &Device, home: &str, reading: Option<&DeviceSensors>, unit: Unit, now: i64) -> Option<Dropped> {
    let mut sensors: Vec<_> = reading.map(|r| r.sensors.iter().collect()).unwrap_or_default();
    sort_sensors(&mut sensors, |s| &s.sensor_type);
    let level = worst_level(sensors.iter().map(|s| (s.sensor_type.as_str(), s.value)));
    let recorded = reading.and_then(|r| parse_recorded(r.recorded.as_deref()));
    let name = if device.name.is_empty() { &device.serial_number } else { &device.name };

    let response = ui
        .scope_builder(UiBuilder::new().sense(Sense::drag()), |ui| {
            // Selectable text would swallow drags meant for the card.
            ui.style_mut().interaction.selectable_labels = false;
            widgets::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
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

    // Screen readers announce the card by the device's name.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, name));
    response.dnd_set_drag_payload(DraggedDevice { serial: device.serial_number.clone(), home: home.to_owned() });
    let accent = ui.visuals().hyperlink_color;
    let rect = response.rect;

    if response.dragged() {
        // Fade the card being moved and carry its name along with the pointer.
        ui.painter().rect_filled(rect, 10.0, ui.visuals().panel_fill.gamma_multiply(0.6));
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, response.id));
            let text_color = ui.visuals().text_color();
            let galley = painter.layout_no_wrap(name.to_owned(), egui::FontId::proportional(15.0), text_color);
            let size = galley.size() + egui::vec2(20.0, 12.0);
            // Keep the label inside the window near the right and bottom edges.
            let screen = ui.ctx().content_rect();
            let min = (pointer + egui::vec2(12.0, 8.0)).min(screen.max - size);
            let label = egui::Rect::from_min_size(min, size);
            painter.rect(label, 8.0, ui.visuals().window_fill, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
            painter.galley(label.min + egui::vec2(10.0, 6.0), galley, text_color);
        }
        return None;
    }

    // Only another device from the same home can be dropped here.
    let droppable = |d: &DraggedDevice| d.home == home && d.serial != device.serial_number;
    let after = ui.ctx().pointer_interact_pos().is_some_and(|p| p.x > rect.center().x);
    if response.dnd_hover_payload::<DraggedDevice>().is_some_and(|d| droppable(&d)) {
        // Show where the device will land: a bar on the near side of this card.
        let clip = ui.clip_rect();
        let x = if after { (rect.right() + 8.0).min(clip.right() - 2.0) } else { (rect.left() - 8.0).max(clip.left() + 2.0) };
        ui.painter().line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(3.0, accent));
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if let Some(dragged) = response.dnd_release_payload::<DraggedDevice>().filter(|d| droppable(d)) {
        return Some(Dropped { serial: dragged.serial.clone(), after });
    }
    response.on_hover_cursor(egui::CursorIcon::Grab);
    None
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

    fn serials(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn moves_a_device_before_or_after_another() {
        let order = serials(&["A", "B", "C", "D"]);
        assert_eq!(reorder(&order, "D", "B", false), serials(&["A", "D", "B", "C"]));
        assert_eq!(reorder(&order, "A", "C", true), serials(&["B", "C", "A", "D"]));
        assert_eq!(reorder(&order, "B", "D", true), serials(&["A", "C", "D", "B"]));
    }

    #[test]
    fn dropping_on_itself_or_unknown_devices_changes_nothing() {
        let order = serials(&["A", "B"]);
        assert_eq!(reorder(&order, "A", "A", true), order);
        assert_eq!(reorder(&order, "X", "A", false), order);
    }

    #[test]
    fn parses_recorded_timestamps() {
        assert_eq!(parse_recorded(Some("2026-10-14T10:00:00Z")), Some(1_791_972_000));
        assert_eq!(parse_recorded(Some("2026-10-14T10:00:00")), Some(1_791_972_000));
        assert_eq!(parse_recorded(Some("garbage")), None);
        assert_eq!(parse_recorded(None), None);
    }
}
