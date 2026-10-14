//! User preferences, stored as JSON in the app config folder.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::Unit;
use crate::auth::{BUILTIN_CLIENT_ID, BUILTIN_CLIENT_SECRET, DEFAULT_REDIRECT_PORT, DEFAULT_SCOPE};
use crate::error::{Error, Result};

pub const MIN_POLL_MINUTES: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Display units. Readings are always fetched and stored in metric.
    pub unit: Unit,
    pub poll_interval_minutes: u32,
    pub close_to_tray: bool,
    /// "system", "light" or "dark".
    pub theme: String,
    /// Overrides the OAuth client compiled into the app.
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    pub redirect_port: u16,
    /// Scope requested with client credentials.
    pub scope: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            unit: Unit::Metric,
            poll_interval_minutes: 5,
            close_to_tray: true,
            theme: "system".into(),
            oauth_client_id: None,
            oauth_client_secret: None,
            redirect_port: DEFAULT_REDIRECT_PORT,
            scope: DEFAULT_SCOPE.into(),
        }
    }
}

impl Settings {
    /// Missing or unreadable files give the defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .normalized()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Other(e.to_string()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| Error::Other(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| Error::Other(e.to_string()))
    }

    /// Clamp values the UI or a hand-edited file may have set out of range.
    pub fn normalized(mut self) -> Self {
        self.poll_interval_minutes = self.poll_interval_minutes.max(MIN_POLL_MINUTES);
        if self.redirect_port == 0 {
            self.redirect_port = DEFAULT_REDIRECT_PORT;
        }
        if self.scope.trim().is_empty() {
            self.scope = DEFAULT_SCOPE.into();
        }
        let blank = |v: &Option<String>| v.as_deref().map(str::trim).unwrap_or("").is_empty();
        if blank(&self.oauth_client_id) {
            self.oauth_client_id = None;
        }
        if blank(&self.oauth_client_secret) {
            self.oauth_client_secret = None;
        }
        self
    }

    /// The OAuth client to sign in with: the user's override, else the one built into the app.
    pub fn oauth_client(&self) -> Option<(String, Option<String>)> {
        if let Some(id) = &self.oauth_client_id {
            return Some((id.clone(), self.oauth_client_secret.clone()));
        }
        BUILTIN_CLIENT_ID
            .filter(|id| !id.is_empty())
            .map(|id| (id.to_owned(), BUILTIN_CLIENT_SECRET.filter(|s| !s.is_empty()).map(str::to_owned)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = std::env::temp_dir().join(format!("airdash-settings-{}", std::process::id()));
        assert_eq!(Settings::load(&dir.join("missing.json")), Settings::default());
    }

    #[test]
    fn round_trips_through_file() {
        let dir = std::env::temp_dir().join(format!("airdash-settings-rt-{}", std::process::id()));
        let path = dir.join("settings.json");
        let settings = Settings { unit: Unit::Imperial, poll_interval_minutes: 10, ..Default::default() };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_json_fills_in_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"theme": "dark"}"#).unwrap();
        assert_eq!(settings.theme, "dark");
        assert_eq!(settings.poll_interval_minutes, 5);
    }

    #[test]
    fn normalizes_out_of_range_values() {
        let settings = Settings {
            poll_interval_minutes: 0,
            redirect_port: 0,
            scope: " ".into(),
            oauth_client_id: Some("  ".into()),
            ..Default::default()
        }
        .normalized();
        assert_eq!(settings.poll_interval_minutes, MIN_POLL_MINUTES);
        assert_eq!(settings.redirect_port, DEFAULT_REDIRECT_PORT);
        assert_eq!(settings.scope, DEFAULT_SCOPE);
        assert_eq!(settings.oauth_client_id, None);
    }

    #[test]
    fn override_client_wins_over_builtin() {
        let settings = Settings { oauth_client_id: Some("mine".into()), ..Default::default() };
        assert_eq!(settings.oauth_client(), Some(("mine".into(), None)));
    }
}
