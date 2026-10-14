// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod chart;
mod format;
mod sensors;
mod theme;
mod views;
mod widgets;

/// Shared with the Tauri app.
pub const ICON_PNG: &[u8] = include_bytes!("../../src-tauri/icons/128x128.png");

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("AirDash")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([480.0, 360.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "AirDash",
        options,
        Box::new(|cc| {
            let backend = backend::Backend::new(cc.egui_ctx.clone())?;
            Ok(Box::new(app::AirDashApp::new(&cc.egui_ctx, backend)))
        }),
    )
}
