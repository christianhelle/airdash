//! Units, theme, recording, advanced sign-in options, account and local data.

use airdash_core::api::Unit;
use airdash_core::settings::{MIN_POLL_MINUTES, Settings};
use egui::{RichText, Ui};

use crate::backend::{AuthInfo, Backend, SignInMethod, Task};
use crate::widgets;

pub enum Action {
    Back,
    Saved(Settings),
    SignedOut,
}

pub struct SettingsView {
    // Edit a copy; changes apply on Save.
    draft: Settings,
    oauth_client_id: String,
    oauth_client_secret: String,
    message: String,
    confirm_delete: bool,
    sign_out: Option<Task<()>>,
}

impl SettingsView {
    pub fn new(settings: &Settings) -> Self {
        SettingsView {
            oauth_client_id: settings.oauth_client_id.clone().unwrap_or_default(),
            oauth_client_secret: settings.oauth_client_secret.clone().unwrap_or_default(),
            draft: settings.clone(),
            message: String::new(),
            confirm_delete: false,
            sign_out: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, backend: &Backend, auth: &AuthInfo) -> Option<Action> {
        if self.sign_out.as_ref().and_then(Task::ready).is_some() {
            return Some(Action::SignedOut);
        }
        let mut action = None;
        ui.set_max_width(640.0);
        if ui.link("⬅ All devices").clicked() {
            action = Some(Action::Back);
        }
        ui.label(RichText::new("Settings").size(22.0).strong());
        ui.add_space(8.0);

        section(ui, "Display", |ui| {
            let caption = ui.label("Units");
            egui::ComboBox::from_id_salt("unit")
                .selected_text(unit_label(self.draft.unit))
                .width(260.0)
                .show_ui(ui, |ui| {
                    for unit in [Unit::Metric, Unit::Imperial] {
                        ui.selectable_value(&mut self.draft.unit, unit, unit_label(unit));
                    }
                })
                .response
                .labelled_by(caption.id);
            let caption = ui.label("Theme");
            egui::ComboBox::from_id_salt("theme")
                .selected_text(theme_label(&self.draft.theme))
                .width(260.0)
                .show_ui(ui, |ui| {
                    for theme in ["system", "light", "dark"] {
                        ui.selectable_value(&mut self.draft.theme, theme.to_owned(), theme_label(theme));
                    }
                })
                .response
                .labelled_by(caption.id);
        });

        section(ui, "Recording", |ui| {
            ui.horizontal(|ui| {
                let caption = ui.label("Check for new readings every");
                ui.add(egui::DragValue::new(&mut self.draft.poll_interval_minutes).range(MIN_POLL_MINUTES..=120).suffix(" min"))
                    .labelled_by(caption.id);
            });
            hint(ui, "Airthings allows 120 requests per hour and most devices update every 5 minutes.");
        });

        widgets::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(RichText::new("Advanced sign-in options").strong()).show(ui, |ui| {
                hint(ui, "Sign in through the browser with your own Airthings OAuth client (authorization code flow). Register this redirect URI with it:");
                ui.code(&auth.redirect_uri);
                ui.add_space(4.0);
                // Each field is labelled by its caption, so screen readers announce it.
                let caption = ui.label("OAuth client ID");
                ui.add(egui::TextEdit::singleline(&mut self.oauth_client_id).hint_text("Not set").desired_width(f32::INFINITY))
                    .labelled_by(caption.id);
                let caption = ui.label("OAuth client secret (leave empty for public clients)");
                ui.add(egui::TextEdit::singleline(&mut self.oauth_client_secret).password(true).desired_width(f32::INFINITY))
                    .labelled_by(caption.id);
                ui.horizontal(|ui| {
                    let caption = ui.label("Redirect port");
                    ui.add(egui::DragValue::new(&mut self.draft.redirect_port).range(1024..=65535)).labelled_by(caption.id);
                });
                let caption = ui.label("API client scope");
                ui.add(egui::TextEdit::singleline(&mut self.draft.scope).desired_width(f32::INFINITY)).labelled_by(caption.id);
            });
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.draft.oauth_client_id = Some(self.oauth_client_id.clone());
                self.draft.oauth_client_secret = Some(self.oauth_client_secret.clone());
                match backend.save_settings(self.draft.clone()) {
                    Ok(saved) => {
                        *self = SettingsView { message: "Settings saved.".into(), ..SettingsView::new(&saved) };
                        action = Some(Action::Saved(saved));
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            if !self.message.is_empty() {
                ui.weak(&self.message);
            }
        });
        ui.add_space(8.0);

        section(ui, "Account", |ui| {
            let how = match auth.method {
                Some(SignInMethod::OAuth) => "with your Airthings account",
                _ => "with an API client",
            };
            ui.weak(format!("Signed in {how}. Signing out keeps your recorded history."));
            if ui.add_enabled(self.sign_out.is_none(), egui::Button::new("Sign out")).clicked() {
                self.sign_out = Some(backend.sign_out());
            }
        });

        section(ui, "Local data", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.weak("History is stored in");
                ui.code(backend.data_dir.display().to_string());
            });
            ui.horizontal(|ui| {
                if ui.button("Open folder").clicked() {
                    backend.open_data_dir();
                }
                let danger = ui.visuals().error_fg_color;
                if self.confirm_delete {
                    if ui.button(RichText::new("Delete all history").color(danger)).clicked() {
                        self.message = match backend.clear_local_data() {
                            Ok(()) => "Local history deleted.".into(),
                            Err(e) => e.to_string(),
                        };
                        self.confirm_delete = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_delete = false;
                    }
                } else if ui.button(RichText::new("Delete local data…").color(danger)).clicked() {
                    self.confirm_delete = true;
                }
            });
        });
        action
    }
}

fn section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    widgets::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        ui.add_space(4.0);
        add_contents(ui);
    });
    ui.add_space(8.0);
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).weak());
}

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Metric => "Metric (°C, Bq/m³, hPa)",
        Unit::Imperial => "Imperial (°F, pCi/L, inHg)",
    }
}

fn theme_label(theme: &str) -> &'static str {
    match theme {
        "light" => "Light",
        "dark" => "Dark",
        _ => "Same as system",
    }
}
