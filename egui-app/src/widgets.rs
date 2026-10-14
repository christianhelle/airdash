//! Small reusable pieces: cards, level badges and sensor tiles.

use airdash_core::api::{SensorValue, Unit};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

use crate::sensors::{self, Level};

pub const GOOD: Color32 = Color32::from_rgb(0x0c, 0xa3, 0x0c);
pub const FAIR: Color32 = Color32::from_rgb(0xfa, 0xb2, 0x19);
pub const POOR: Color32 = Color32::from_rgb(0xd0, 0x3b, 0x3b);

pub fn level_color(level: Level) -> Color32 {
    match level {
        Level::Good => GOOD,
        Level::Fair => FAIR,
        Level::Poor => POOR,
    }
}

/// The frame every card on the dashboard and detail pages uses.
pub fn card(ui: &Ui) -> Frame {
    Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
}

/// Column count and width that fill `available` with columns of at least `min_width`.
pub fn columns(available: f32, min_width: f32, gap: f32) -> (usize, f32) {
    let count = (((available + gap) / (min_width + gap)).floor() as usize).max(1);
    let width = (available - gap * (count - 1) as f32) / count as f32;
    (count, width.max(0.0))
}

/// Lay `items` out in rows of equal-width cells.
pub fn grid<T>(ui: &mut Ui, items: &[T], min_width: f32, gap: f32, mut cell: impl FnMut(&mut Ui, &T)) {
    let (count, width) = columns(ui.available_width(), min_width, gap);
    ui.spacing_mut().item_spacing = vec2(gap, gap);
    for row in items.chunks(count) {
        ui.horizontal_top(|ui| {
            for item in row {
                ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(width);
                    cell(ui, item);
                });
            }
        });
    }
}

/// A colored shape plus the level name, so the level never depends on color alone.
pub fn level_badge(ui: &mut Ui, level: Level) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let size = 10.0;
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        let color = level_color(level);
        let painter = ui.painter();
        match level {
            Level::Good => painter.circle_filled(rect.center(), size / 2.0 - 0.5, color),
            Level::Fair => painter.add(egui::Shape::convex_polygon(
                vec![rect.center_top(), rect.right_bottom(), rect.left_bottom()],
                color,
                Stroke::NONE,
            )),
            Level::Poor => painter.rect_filled(rect.shrink(1.0), 1.0, color),
        };
        ui.label(RichText::new(level.label()).size(12.0).weak());
    })
    .response
    .on_hover_text(level.label());
}

/// One sensor's latest value, with a colored edge for its level.
pub fn sensor_tile(ui: &mut Ui, sensor: &SensorValue, unit: Unit) {
    let level = sensors::level_of(&sensor.sensor_type, sensor.value);
    let edge = level.map(level_color).unwrap_or(ui.visuals().widgets.noninteractive.bg_stroke.color);
    let response = Frame::new()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add(egui::Label::new(RichText::new(sensors::sensor_label(&sensor.sensor_type)).size(12.0).weak()).truncate());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(sensors::format_value(&sensor.sensor_type, sensor.value, unit)).size(20.0).strong());
                ui.label(RichText::new(sensors::display_unit(&sensor.sensor_type, unit, &sensor.unit)).size(12.0).weak());
            });
            if let Some(level) = level {
                level_badge(ui, level);
            }
        })
        .response;
    let rect = response.rect;
    let edge_rect = egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 4.0, rect.bottom()));
    ui.painter().rect_filled(edge_rect, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, edge);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_the_width_with_as_many_columns_as_fit() {
        assert_eq!(columns(1000.0, 340.0, 16.0), (2, 492.0));
        assert_eq!(columns(1100.0, 340.0, 16.0), (3, (1100.0 - 32.0) / 3.0));
        assert_eq!(columns(200.0, 340.0, 16.0), (1, 200.0));
    }
}
