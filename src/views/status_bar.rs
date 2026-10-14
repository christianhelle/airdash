//! Bottom bar: when the last update happened, the next one, API budget, and pause/refresh.

use airdash_core::poller::{Poller, PollerStatus};
use egui::{RichText, Ui};

use crate::format::{time_ago, time_until};

pub fn state_text(status: &PollerStatus, now: i64) -> String {
    let mut text = if status.polling {
        "Updating…".to_owned()
    } else if status.paused {
        "Recording paused".to_owned()
    } else if let Some(last) = status.last_poll {
        format!("Updated {}", time_ago(last, now))
    } else {
        "Not updated yet".to_owned()
    };
    if let (false, false, Some(next)) = (status.paused, status.polling, status.next_poll) {
        text.push_str(&format!(" · next {}", time_until(next, now)));
    }
    text
}

pub fn ui(ui: &mut Ui, status: &PollerStatus, poller: &Poller) {
    let now = chrono::Utc::now().timestamp();
    ui.horizontal(|ui| {
        ui.label(RichText::new(state_text(status, now)).size(13.0));
        if let Some(error) = &status.last_error {
            ui.add(egui::Label::new(RichText::new(error).size(13.0).color(ui.visuals().error_fg_color)).truncate())
                .on_hover_text(error);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh = ui.add_enabled(!status.polling && !status.paused, egui::Link::new("Refresh"));
            if refresh.clicked() {
                poller.refresh_now();
            }
            if ui.link(if status.paused { "Resume" } else { "Pause" }).clicked() {
                poller.set_paused(!status.paused);
            }
            if let Some((remaining, limit)) = status.rate_limit.as_ref().and_then(|r| Some((r.remaining?, r.limit?))) {
                ui.label(RichText::new(format!("API {remaining}/{limit}")).size(13.0).weak())
                    .on_hover_text("Airthings API requests left this hour");
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_791_979_200;

    #[test]
    fn describes_the_poller_state() {
        let idle = PollerStatus { last_poll: Some(NOW - 120), next_poll: Some(NOW + 180), ..Default::default() };
        assert_eq!(state_text(&idle, NOW), "Updated 2 min ago · next in 3 min");
        let paused = PollerStatus { paused: true, next_poll: Some(NOW + 180), ..Default::default() };
        assert_eq!(state_text(&paused, NOW), "Recording paused");
        let polling = PollerStatus { polling: true, ..Default::default() };
        assert_eq!(state_text(&polling, NOW), "Updating…");
        assert_eq!(state_text(&PollerStatus::default(), NOW), "Not updated yet");
    }
}
