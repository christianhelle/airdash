//! Bottom bar: when the last update happened, the next one, API budget, and refresh.

use egui::{RichText, Ui};

use crate::api::RateLimit;
use crate::format::{time_ago, time_until};

/// How often the dashboard refreshes on its own.
pub const REFRESH_INTERVAL_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub refreshing: bool,
    /// Epoch seconds of the last successful refresh.
    pub last_refresh: Option<i64>,
    /// Epoch seconds of the last refresh, successful or not. The next one is due an interval after it.
    pub last_attempt: Option<i64>,
    pub last_error: Option<String>,
    pub rate_limit: Option<RateLimit>,
}

impl Status {
    /// Epoch seconds when the next automatic refresh is due, or `None` if it is due now.
    pub fn next_refresh(&self) -> Option<i64> {
        self.last_attempt.map(|t| t + REFRESH_INTERVAL_SECS)
    }

    pub fn refresh_due(&self, now: i64) -> bool {
        !self.refreshing && self.next_refresh().is_none_or(|next| now >= next)
    }
}

pub fn state_text(status: &Status, now: i64) -> String {
    let mut text = if status.refreshing {
        "Updating…".to_owned()
    } else if let Some(last) = status.last_refresh {
        format!("Updated {}", time_ago(last, now))
    } else {
        "Not updated yet".to_owned()
    };
    if let (false, Some(next)) = (status.refreshing, status.next_refresh()) {
        text.push_str(&format!(" · next {}", time_until(next, now)));
    }
    text
}

/// True when the user asked to refresh now.
pub fn ui(ui: &mut Ui, status: &Status) -> bool {
    let now = chrono::Utc::now().timestamp();
    let mut refresh = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(state_text(status, now)).size(13.0));
        if let Some(error) = &status.last_error {
            ui.add(egui::Label::new(RichText::new(error).size(13.0).color(ui.visuals().error_fg_color)).truncate())
                .on_hover_text(error);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            refresh = ui.add_enabled(!status.refreshing, egui::Link::new("Refresh")).clicked();
            if let Some((remaining, limit)) = status.rate_limit.as_ref().and_then(|r| Some((r.remaining?, r.limit?))) {
                ui.label(RichText::new(format!("API {remaining}/{limit}")).size(13.0).weak())
                    .on_hover_text("Airthings API requests left this hour");
            }
        });
    });
    refresh
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_791_979_200;

    #[test]
    fn describes_the_refresh_state() {
        let idle = Status { last_refresh: Some(NOW - 120), last_attempt: Some(NOW - 120), ..Default::default() };
        assert_eq!(state_text(&idle, NOW), "Updated 2 min ago · next in 3 min");
        let refreshing = Status { refreshing: true, ..Default::default() };
        assert_eq!(state_text(&refreshing, NOW), "Updating…");
        assert_eq!(state_text(&Status::default(), NOW), "Not updated yet");
    }

    #[test]
    fn refreshes_every_five_minutes() {
        assert!(Status::default().refresh_due(NOW));
        let recent = Status { last_attempt: Some(NOW - 299), ..Default::default() };
        assert!(!recent.refresh_due(NOW));
        let old = Status { last_attempt: Some(NOW - 300), ..Default::default() };
        assert!(old.refresh_due(NOW));
        let busy = Status { refreshing: true, ..old };
        assert!(!busy.refresh_due(NOW));
    }
}
