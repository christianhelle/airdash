//! Where a session keeps its sign-in credentials between runs.

use std::sync::Mutex;

use crate::auth::Credentials;
use crate::error::Result;

pub trait SecretStore: Send + Sync {
    fn load(&self) -> Option<Credentials>;
    fn save(&self, credentials: &Credentials) -> Result<()>;
    fn clear(&self);
}

/// Keeps credentials for the lifetime of the process only.
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<Credentials>>);

impl SecretStore for MemoryStore {
    fn load(&self) -> Option<Credentials> {
        self.0.lock().unwrap().clone()
    }

    fn save(&self, credentials: &Credentials) -> Result<()> {
        *self.0.lock().unwrap() = Some(credentials.clone());
        Ok(())
    }

    fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }
}
