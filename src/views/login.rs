//! Sign-in screen: Airthings browser sign-in, or the user's own API client.

use airdash_core::error::Result;

use crate::backend::{AuthInfo, Backend, Task};

const MY_APPLICATIONS: &str = "https://consumer-api-doc.airthings.com/dashboard";

#[derive(Default)]
pub struct LoginView {
    task: Option<Task<Result<()>>>,
    waiting_for_browser: bool,
    error: String,
    client_id: String,
    client_secret: String,
    show_credentials: Option<bool>,
}

impl LoginView {
    /// Returns true once sign-in succeeds.
    pub fn ui(&mut self, ui: &mut egui::Ui, backend: &Backend, auth: &AuthInfo) -> bool {
        let mut signed_in = false;
        if let Some(result) = self.task.as_ref().and_then(Task::ready) {
            self.task = None;
            self.waiting_for_browser = false;
            match result {
                Ok(()) => signed_in = true,
                Err(e) => self.error = e.to_string(),
            }
        }
        let busy = self.task.is_some();
        let show_credentials = self.show_credentials.get_or_insert(!auth.oauth_available);

        // Measured outside the scroll area, whose own height is unbounded.
        let top_space = (ui.available_height() * 0.18).max(16.0);
        // Scrolls so nothing is cut off in a small window.
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| ui.vertical_centered(|ui| {
            ui.add_space(top_space);
            egui::Frame::group(ui.style()).inner_margin(24.0).corner_radius(8.0).show(ui, |ui| {
                ui.set_width(380.0_f32.min(ui.available_width()));
                ui.vertical(|ui| {
                    ui.heading(egui::RichText::new("AirDash").size(26.0).strong());
                    ui.weak("Your Airthings air quality, on your desktop.");
                    ui.add_space(12.0);

                    if auth.oauth_available {
                        let label = if self.waiting_for_browser { "Waiting for the browser…" } else { "Sign in with Airthings" };
                        let button = egui::Button::new(egui::RichText::new(label).size(16.0)).min_size(egui::vec2(ui.available_width(), 36.0));
                        if ui.add_enabled(!busy, button).clicked() {
                            self.error.clear();
                            self.waiting_for_browser = true;
                            self.task = Some(backend.sign_in_oauth());
                        }
                        if self.waiting_for_browser {
                            ui.weak("Finish signing in in the browser window that opened, then come back here.");
                        }
                        ui.vertical_centered(|ui| {
                            let toggle = if *show_credentials { "Hide" } else { "Use your own API client instead" };
                            if ui.link(toggle).clicked() {
                                *show_credentials = !*show_credentials;
                            }
                        });
                    }

                    if *show_credentials {
                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.weak("Create an API client in ");
                            if ui.link("Airthings My Applications").clicked() {
                                backend.open_url(MY_APPLICATIONS);
                            }
                            ui.weak(" and paste its client ID and secret here. You only need to do this once.");
                        });
                        ui.add_space(8.0);
                        // Each field is labelled by its caption, so screen readers announce it.
                        let caption = ui.label("Client ID");
                        ui.add(egui::TextEdit::singleline(&mut self.client_id).desired_width(f32::INFINITY))
                            .labelled_by(caption.id);
                        let caption = ui.label("Client secret");
                        let secret = ui
                            .add(egui::TextEdit::singleline(&mut self.client_secret).password(true).desired_width(f32::INFINITY))
                            .labelled_by(caption.id);
                        let submitted = secret.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.add_space(8.0);
                        let label = if busy && !self.waiting_for_browser { "Signing in…" } else { "Sign in" };
                        let clicked = ui.add_enabled(!busy, egui::Button::new(label)).clicked();
                        if !busy && (clicked || submitted) {
                            self.error.clear();
                            self.task = Some(backend.sign_in_client_credentials(&self.client_id, &self.client_secret));
                        }
                    }

                    if !self.error.is_empty() {
                        ui.add_space(8.0);
                        ui.colored_label(ui.visuals().error_fg_color, &self.error);
                    }
                });
            });
            ui.add_space(12.0);
            ui.weak("AirDash is not made by Airthings. Readings are stored only on this computer.");
            ui.add_space(16.0);
        }));
        signed_in
    }
}
