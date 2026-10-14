//! The command surface the UI calls through `invoke`.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::api::{Device, DeviceSensors, RemoteControlState};
use crate::auth::{self, Credentials};
use crate::error::{Error, Result};
use crate::poller::{Poller, PollerStatus};
use crate::session::Session;
use crate::settings::Settings;
use crate::store::{SensorInfo, Store};

pub struct AppState {
    pub session: Arc<Session>,
    pub store: Arc<Store>,
    pub poller: Arc<Poller>,
    pub settings: Arc<RwLock<Settings>>,
    pub settings_path: PathBuf,
    pub data_dir: PathBuf,
}

impl AppState {
    fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthInfo {
    signed_in: bool,
    /// "oauth" or "clientCredentials" when signed in.
    method: Option<&'static str>,
    oauth_available: bool,
    redirect_uri: String,
}

#[tauri::command]
pub fn auth_info(state: State<'_, AppState>) -> AuthInfo {
    let settings = state.settings();
    AuthInfo {
        signed_in: state.session.is_signed_in(),
        method: state.session.credentials().map(|c| match c {
            Credentials::RefreshToken { .. } => "oauth",
            Credentials::ClientCredentials { .. } => "clientCredentials",
        }),
        oauth_available: settings.oauth_client().is_some(),
        redirect_uri: auth::redirect_uri(settings.redirect_port),
    }
}

/// Sign in through the Airthings login page in the system browser.
#[tauri::command]
pub async fn sign_in_oauth(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let settings = state.settings();
    let (client_id, client_secret) = settings
        .oauth_client()
        .ok_or_else(|| Error::Other("this build has no Airthings OAuth client; use API client credentials instead".into()))?;
    state
        .session
        .sign_in_oauth(client_id, client_secret, settings.redirect_port, |url| {
            app.opener()
                .open_url(url, None::<&str>)
                .map_err(|e| Error::Other(format!("could not open the browser: {e}")))
        })
        .await?;
    state.poller.reset();

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
    }
    Ok(())
}

/// Sign in with an API client the user created in Airthings "My Applications".
#[tauri::command]
pub async fn sign_in_client_credentials(state: State<'_, AppState>, client_id: String, client_secret: String) -> Result<()> {
    let (client_id, client_secret) = (client_id.trim().to_owned(), client_secret.trim().to_owned());
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(Error::Other("enter both the client ID and the client secret".into()));
    }
    let scope = state.settings().scope;
    state.session.sign_in(Credentials::ClientCredentials { client_id, client_secret, scope }).await?;
    state.poller.reset();
    Ok(())
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> Result<()> {
    state.session.sign_out().await;
    state.poller.reset();
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let settings = settings.normalized();
    settings.save(&state.settings_path)?;
    *state.settings.write().unwrap() = settings.clone();
    Ok(settings)
}

#[tauri::command]
pub fn devices(state: State<'_, AppState>) -> Result<Vec<Device>> {
    state.store.devices()
}

#[tauri::command]
pub fn latest_readings(state: State<'_, AppState>) -> Result<Vec<DeviceSensors>> {
    state.store.latest()
}

#[tauri::command]
pub fn sensor_types(state: State<'_, AppState>, serial: String) -> Result<Vec<SensorInfo>> {
    state.store.sensor_types(&serial)
}

#[tauri::command]
pub fn series(state: State<'_, AppState>, serial: String, sensor_type: String, from: i64, to: i64, max_points: usize) -> Result<Vec<(i64, f64)>> {
    state.store.series(&serial, &sensor_type, from, to, max_points)
}

/// Ask where to save, then write the device's readings as CSV. Returns the path, or `None` if cancelled.
#[tauri::command]
pub async fn export_csv(app: AppHandle, state: State<'_, AppState>, serial: String, file_name: String, from: i64, to: i64) -> Result<Option<String>> {
    let csv = state.store.export_csv(&serial, from, to)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name(&file_name)
        .add_filter("CSV", &["csv"])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.ok().flatten() else { return Ok(None) };
    let path = path.into_path().map_err(|e| Error::Other(e.to_string()))?;
    std::fs::write(&path, csv).map_err(|e| Error::Other(format!("could not write {}: {e}", path.display())))?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub fn poller_status(state: State<'_, AppState>) -> PollerStatus {
    state.poller.status()
}

#[tauri::command]
pub fn refresh_now(state: State<'_, AppState>) {
    state.poller.refresh_now();
}

#[tauri::command]
pub fn set_paused(state: State<'_, AppState>, paused: bool) {
    state.poller.set_paused(paused);
}

#[tauri::command]
pub async fn remote_control(state: State<'_, AppState>, serial: String) -> Result<Option<RemoteControlState>> {
    state.session.remote_control(&serial).await
}

#[tauri::command]
pub async fn set_remote_control(state: State<'_, AppState>, serial: String, mode: RemoteControlState) -> Result<()> {
    state.session.set_remote_control(&serial, &mode).await
}

#[tauri::command]
pub fn clear_local_data(state: State<'_, AppState>) -> Result<()> {
    state.store.clear()?;
    state.poller.refresh_now();
    Ok(())
}

#[tauri::command]
pub fn data_dir(state: State<'_, AppState>) -> String {
    state.data_dir.display().to_string()
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<()> {
    if !url.starts_with("https://") {
        return Err(Error::Other("only https links can be opened".into()));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    app.opener()
        .open_path(state.data_dir.display().to_string(), None::<&str>)
        .map_err(|e| Error::Other(e.to_string()))
}
