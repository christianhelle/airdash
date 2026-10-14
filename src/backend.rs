//! Owns the core services and the async runtime, and hands results back to the UI thread.

use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use airdash_core::api::{self, ApiClient};
use airdash_core::auth::{self, AuthClient, Credentials};
use airdash_core::error::{Error, Result};
use airdash_core::refresh::{self, Snapshot};
use airdash_core::secrets::KeychainStore;
use airdash_core::session::Session;
use airdash_core::settings::Settings;
use airdash_core::store::Store;
use tokio::runtime::Runtime;

/// The folders the earlier Tauri version used, so existing settings, history and sign-in carry over.
pub const APP_IDENTIFIER: &str = "com.christianhelle.airdash";

/// The result of background work, picked up by the UI on a later frame.
pub struct Task<T>(Receiver<T>);

impl<T> Task<T> {
    /// The result, once. Returns `None` while the work is still running.
    pub fn ready(&self) -> Option<T> {
        self.0.try_recv().ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInMethod {
    OAuth,
    ClientCredentials,
}

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub signed_in: bool,
    pub method: Option<SignInMethod>,
    pub oauth_available: bool,
    pub redirect_uri: String,
}

pub struct Backend {
    runtime: Runtime,
    ctx: egui::Context,
    pub session: Arc<Session>,
    pub store: Arc<Store>,
    settings: Arc<RwLock<Settings>>,
    settings_path: PathBuf,
    pub data_dir: PathBuf,
}

impl Backend {
    pub fn new(ctx: egui::Context) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config_dir = dirs::config_dir().ok_or("no config folder on this system")?.join(APP_IDENTIFIER);
        let data_dir = dirs::data_dir().ok_or("no data folder on this system")?.join(APP_IDENTIFIER);
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

        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

        Ok(Backend { runtime, ctx, session, store, settings, settings_path, data_dir })
    }

    /// Run `future` on the runtime and wake the UI when it finishes.
    pub fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) -> Task<T> {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(future.await);
            ctx.request_repaint();
        });
        Task(rx)
    }

    /// Fetch the devices and their latest readings, and record them.
    pub fn refresh(&self) -> Task<Result<Snapshot>> {
        let (session, store) = (self.session.clone(), self.store.clone());
        self.spawn(async move {
            let snapshot = refresh::fetch(&session).await?;
            store.replace_devices(&snapshot.devices)?;
            store.insert_readings(&snapshot.readings)?;
            Ok(snapshot)
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn save_settings(&self, settings: Settings) -> Result<Settings> {
        let settings = settings.normalized();
        settings.save(&self.settings_path)?;
        *self.settings.write().unwrap() = settings.clone();
        Ok(settings)
    }

    pub fn auth_info(&self) -> AuthInfo {
        let settings = self.settings();
        AuthInfo {
            signed_in: self.session.is_signed_in(),
            method: self.session.credentials().map(|c| match c {
                Credentials::RefreshToken { .. } => SignInMethod::OAuth,
                Credentials::ClientCredentials { .. } => SignInMethod::ClientCredentials,
            }),
            oauth_available: settings.oauth_client().is_some(),
            redirect_uri: auth::redirect_uri(settings.redirect_port),
        }
    }

    /// Sign in through the Airthings login page in the system browser.
    pub fn sign_in_oauth(&self) -> Task<Result<()>> {
        let settings = self.settings();
        let session = self.session.clone();
        self.spawn(async move {
            let (client_id, client_secret) = settings
                .oauth_client()
                .ok_or_else(|| Error::Other("no Airthings OAuth client is set up; sign in with your API client instead".into()))?;
            session
                .sign_in_oauth(client_id, client_secret, settings.redirect_port, |url| {
                    open::that_detached(url).map_err(|e| Error::Other(format!("could not open the browser: {e}")))
                })
                .await
        })
    }

    /// Sign in with an API client the user created in Airthings "My Applications".
    pub fn sign_in_client_credentials(&self, client_id: &str, client_secret: &str) -> Task<Result<()>> {
        let (client_id, client_secret) = (client_id.trim().to_owned(), client_secret.trim().to_owned());
        let scope = self.settings().scope;
        let session = self.session.clone();
        self.spawn(async move {
            if client_id.is_empty() || client_secret.is_empty() {
                return Err(Error::Other("enter both the client ID and the client secret".into()));
            }
            session.sign_in(Credentials::ClientCredentials { client_id, client_secret, scope }).await
        })
    }

    pub fn sign_out(&self) -> Task<()> {
        let session = self.session.clone();
        self.spawn(async move { session.sign_out().await })
    }

    pub fn clear_local_data(&self) -> Result<()> {
        self.store.clear()
    }

    pub fn open_url(&self, url: &str) {
        if url.starts_with("https://") {
            let _ = open::that_detached(url);
        }
    }

    pub fn open_data_dir(&self) {
        let _ = open::that_detached(&self.data_dir);
    }
}
