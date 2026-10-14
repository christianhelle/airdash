// A GUI app on Windows in every build, so launching it never opens a console window.
#![windows_subsystem = "windows"]

use std::sync::{Arc, OnceLock};

mod app;
mod backend;
mod format;
mod sensors;
mod single_instance;
mod theme;
mod tray;
mod views;
mod widgets;

/// Window and tray icon.
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> eframe::Result {
    let single_instance::Instance::Primary(instance) = single_instance::acquire() else {
        return Ok(());
    };
    // Answer other launches right away; the window can be shown once egui is up.
    let egui_ctx: Arc<OnceLock<egui::Context>> = Arc::default();
    let show_ctx = egui_ctx.clone();
    single_instance::listen(instance, move || {
        if let Some(ctx) = show_ctx.get() {
            tray::show_window(ctx);
        }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("AirDash")
        // Also names the folder eframe saves the window size and position in: the app's own data folder.
        .with_app_id(backend::APP_IDENTIFIER)
        // The size on first launch; after that the last size is restored.
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
            let _ = egui_ctx.set(cc.egui_ctx.clone());
            let backend = backend::Backend::new(cc.egui_ctx.clone())?;
            Ok(Box::new(app::AirDashApp::new(&cc.egui_ctx, backend, cc.storage)))
        }),
    )
}
