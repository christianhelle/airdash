use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Airthings returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
    #[error("rate limited by Airthings, retry in {0} seconds")]
    RateLimited(u64),
    #[error("not signed in")]
    Unauthorized,
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Tauri commands return errors to the UI as plain strings.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
