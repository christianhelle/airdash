//! The last known state of the dashboard, saved as JSON whenever it changes so the next launch can show it at once.

use std::path::Path;

use serde::{Deserialize, Serialize};

use airthings::api::Device;
use airthings::error::{Error, Result};
use crate::refresh::Snapshot;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppState {
    /// The result of the last successful refresh.
    pub snapshot: Snapshot,
    /// Epoch seconds of the last successful refresh.
    pub last_refresh: Option<i64>,
    /// Serial numbers in the order the user arranged the devices in.
    pub device_order: Vec<String>,
}

impl AppState {
    /// Missing or unreadable files give an empty state.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    /// Written to a temporary file first, so a crash mid-write never leaves a broken state behind.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_vec(self).map_err(|e| Error::Other(e.to_string()))?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, json).map_err(|e| Error::Other(format!("could not save {}: {e}", temp.display())))?;
        std::fs::rename(&temp, path).map_err(|e| Error::Other(format!("could not save {}: {e}", path.display())))
    }

    /// The devices in the user's order; ones they have not arranged follow by home and name.
    pub fn devices(&self) -> Vec<Device> {
        let mut devices = self.snapshot.devices.clone();
        let position = |d: &Device| self.device_order.iter().position(|s| *s == d.serial_number);
        devices.sort_by(|a, b| match (position(a), position(b)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => (&a.home, &a.name).cmp(&(&b.home, &b.name)),
        });
        devices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(serial: &str, home: Option<&str>, name: &str) -> Device {
        Device { serial_number: serial.into(), home: home.map(Into::into), name: name.into(), device_type: "WAVE_PLUS".into(), sensors: vec![] }
    }

    fn state(devices: Vec<Device>, order: &[&str]) -> AppState {
        AppState {
            snapshot: Snapshot { devices, ..Default::default() },
            device_order: order.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn serials(state: &AppState) -> Vec<String> {
        state.devices().into_iter().map(|d| d.serial_number).collect()
    }

    #[test]
    fn devices_follow_the_saved_order_and_new_ones_come_last() {
        let state = state(vec![device("A", None, "Attic"), device("B", None, "Bedroom"), device("C", None, "Cellar")], &["C", "A"]);
        assert_eq!(serials(&state), ["C", "A", "B"]);
    }

    #[test]
    fn unarranged_devices_are_sorted_by_home_and_name() {
        let state = state(vec![device("A", Some("Home"), "Office"), device("B", Some("Cabin"), "Loft"), device("C", Some("Home"), "Hall")], &[]);
        assert_eq!(serials(&state), ["B", "C", "A"]);
    }

    #[test]
    fn missing_file_gives_an_empty_state() {
        let dir = std::env::temp_dir().join(format!("airdash-state-missing-{}", std::process::id()));
        assert_eq!(AppState::load(&dir.join("state.json")), AppState::default());
    }

    #[test]
    fn round_trips_through_file() {
        let dir = std::env::temp_dir().join(format!("airdash-state-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let saved = AppState { last_refresh: Some(1_791_979_200), ..state(vec![device("A", None, "Attic")], &["A"]) };
        saved.save(&path).unwrap();
        assert_eq!(AppState::load(&path), saved);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
