//! History chart for one sensor.

use airdash_core::api::Unit;
use chrono::{Local, TimeZone};
use egui::{Color32, Ui};
use egui_plot::{HoverPosition, Line, Plot, Points};

use crate::format::format_date_time;
use crate::sensors::{convert, display_unit, format_value};

pub const SERIES: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
const HEIGHT: f32 = 200.0;

/// Time-axis label: clock time for short ranges, the date for longer ones.
pub fn time_label(epoch: i64, span_seconds: f64) -> String {
    let Some(t) = Local.timestamp_opt(epoch, 0).single() else { return String::new() };
    let format = if span_seconds <= 2.0 * 86_400.0 { "%H:%M" } else { "%b %-d" };
    t.format(format).to_string()
}

/// Value-axis label with at most two decimals and no trailing zeros.
pub fn value_label(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.into() }
}

/// Value range to show: the data's own, widened around flat series so the line sits mid-chart.
pub fn y_bounds(values: impl IntoIterator<Item = f64>) -> Option<(f64, f64)> {
    let (min, max) = values.into_iter().fold(None, |acc: Option<(f64, f64)>, v| match acc {
        Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
        None => Some((v, v)),
    })?;
    let pad = if max <= min { 1.0 } else { 0.0 };
    Some((min - pad, max + pad))
}

/// `points` are `(epoch seconds, metric value)` pairs.
pub fn show(ui: &mut Ui, id: &str, points: &[(i64, f64)], sensor_type: &str, unit: Unit, raw_unit: &str) {
    let series: Vec<[f64; 2]> = points.iter().map(|&(t, v)| [t as f64, convert(sensor_type, v, unit)]).collect();
    let unit_label = display_unit(sensor_type, unit, raw_unit);

    let mut plot = Plot::new(id);
    if let Some((min, max)) = y_bounds(series.iter().map(|p| p[1])) {
        plot = plot.include_y(min).include_y(max);
    }
    plot.height(HEIGHT)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .show_x(false)
        .show_y(false)
        .y_axis_min_width(48.0)
        .show_grid([false, true])
        .x_axis_formatter(|mark, range| time_label(mark.value as i64, range.end() - range.start()))
        .y_axis_formatter(|mark, _| value_label(mark.value))
        .label_formatter(|position| match position {
            HoverPosition::NearDataPoint { index, .. } => {
                let (t, v) = *points.get(*index)?;
                Some(format!("{} {unit_label}\n{}", format_value(sensor_type, v, unit), format_date_time(t)))
            }
            HoverPosition::Elsewhere { .. } => None,
        })
        .show(ui, |plot| {
            if series.len() < 3 {
                plot.add(Points::new(sensor_type, series.clone()).color(SERIES).radius(3.0));
            }
            plot.add(Line::new(sensor_type, series).color(SERIES).width(2.0));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widens_flat_series() {
        assert_eq!(y_bounds([51.0, 51.0]), Some((50.0, 52.0)));
        assert_eq!(y_bounds([0.0]), Some((-1.0, 1.0)));
        assert_eq!(y_bounds([1.0, 3.0]), Some((1.0, 3.0)));
        assert_eq!(y_bounds([]), None);
    }

    #[test]
    fn trims_value_labels() {
        assert_eq!(value_label(1000.0), "1000");
        assert_eq!(value_label(29.5312), "29.53");
        assert_eq!(value_label(0.5), "0.5");
        assert_eq!(value_label(-0.001), "0");
    }

    #[test]
    fn shows_dates_for_long_ranges() {
        let t = Local.with_ymd_and_hms(2026, 10, 14, 9, 30, 0).unwrap().timestamp();
        assert_eq!(time_label(t, 3600.0), "09:30");
        assert_eq!(time_label(t, 7.0 * 86_400.0), "Oct 14");
    }
}
