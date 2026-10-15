//! AirDash keeps sign-in credentials in the OS keychain.

use std::path::PathBuf;

use airthings_client::auth::Credentials;
use airthings_client::error::{Error, Result};
use airthings_client::store::SecretStore;

const SERVICE: &str = "AirDash";
const ACCOUNT: &str = "airthings-credentials";

/// The OS keychain (Windows Credential Manager, macOS Keychain, Secret Service on Linux).
/// Falls back to a file in the app data folder when no keychain is available or the
/// secret is too large for it (Windows limits entries to 2560 bytes).
pub struct KeychainStore {
    fallback_path: PathBuf,
}

impl KeychainStore {
    pub fn new(fallback_path: PathBuf) -> Self {
        KeychainStore { fallback_path }
    }

    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, ACCOUNT).ok()
    }

    fn save_fallback(&self, json: &str) -> Result<()> {
        if let Some(dir) = self.fallback_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Other(e.to_string()))?;
        }
        std::fs::write(&self.fallback_path, json).map_err(|e| Error::Other(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.fallback_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

impl SecretStore for KeychainStore {
    fn load(&self) -> Option<Credentials> {
        let from_keychain = Self::entry().and_then(|e| e.get_password().ok());
        let json = from_keychain.or_else(|| std::fs::read_to_string(&self.fallback_path).ok())?;
        serde_json::from_str(&json).ok()
    }

    fn save(&self, credentials: &Credentials) -> Result<()> {
        let json = serde_json::to_string(credentials).map_err(|e| Error::Other(e.to_string()))?;
        if let Some(entry) = Self::entry()
            && entry.set_password(&json).is_ok()
        {
            let _ = std::fs::remove_file(&self.fallback_path);
            return Ok(());
        }
        self.save_fallback(&json)
    }

    fn clear(&self) {
        if let Some(entry) = Self::entry() {
            let _ = entry.delete_credential();
        }
        let _ = std::fs::remove_file(&self.fallback_path);
    }
}
