pub mod api;
pub mod auth;
pub mod commands;
pub mod error;
pub mod poller;
pub mod secrets;
pub mod session;
pub mod settings;
pub mod store;
mod tray;

use std::sync::{Arc, RwLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use crate::api::ApiClient;
use crate::auth::AuthClient;
use crate::commands::AppState;
use crate::poller::{Poller, PollerEvent};
use crate::secrets::KeychainStore;
use crate::session::Session;
use crate::settings::Settings;
use crate::store::Store;

fn emit(app: &AppHandle, event: PollerEvent) {
    let _ = match event {
        PollerEvent::Status(status) => app.emit("poller-status", status),
        PollerEvent::DevicesUpdated => app.emit("devices-updated", ()),
        PollerEvent::ReadingsUpdated => app.emit("readings-updated", ()),
    };
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = app.path().app_config_dir()?;
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&data_dir)?;

    let settings_path = config_dir.join("settings.json");
    let settings = Arc::new(RwLock::new(Settings::load(&settings_path)));
    let store = Arc::new(Store::open(&data_dir.join("airdash.db"))?);

    let http = reqwest::Client::builder()
        .user_agent(concat!("AirDash/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?;
    let session = Arc::new(Session::new(
        AuthClient::new(http.clone(), auth::DEFAULT_TOKEN_URL),
        ApiClient::new(http, api::DEFAULT_BASE_URL),
        Arc::new(KeychainStore::new(data_dir.join("credentials.json"))),
    ));

    let handle = app.handle().clone();
    let poller = Arc::new(Poller::new(
        session.clone(),
        store.clone(),
        settings.clone(),
        Box::new(move |event| emit(&handle, event)),
    ));
    tauri::async_runtime::spawn(poller.clone().run());

    app.manage(AppState { session, store, poller, settings, settings_path, data_dir });
    tray::create(app.handle())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main_window(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window.state::<AppState>().settings.read().unwrap().close_to_tray;
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth_info,
            commands::sign_in_oauth,
            commands::sign_in_client_credentials,
            commands::sign_out,
            commands::get_settings,
            commands::save_settings,
            commands::devices,
            commands::latest_readings,
            commands::sensor_types,
            commands::series,
            commands::export_csv,
            commands::poller_status,
            commands::refresh_now,
            commands::set_paused,
            commands::remote_control,
            commands::set_remote_control,
            commands::clear_local_data,
            commands::data_dir,
            commands::open_url,
            commands::open_data_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AirDash");
}
