//! The egui application: picks the view and routes backend events to it.

use crate::backend::{AuthInfo, Backend};
use crate::views::login::LoginView;

pub struct AirDashApp {
    backend: Backend,
    auth: AuthInfo,
    login: LoginView,
}

impl AirDashApp {
    pub fn new(backend: Backend) -> Self {
        let auth = backend.auth_info();
        AirDashApp { backend, auth, login: LoginView::default() }
    }

    fn refresh_auth(&mut self) {
        self.auth = self.backend.auth_info();
    }
}

impl eframe::App for AirDashApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _ = self.backend.events();
        egui::CentralPanel::default().show(ui, |ui| {
            if !self.auth.signed_in {
                if self.login.ui(ui, &self.backend, &self.auth) {
                    self.login = LoginView::default();
                    self.refresh_auth();
                }
                return;
            }
            ui.heading("AirDash");
        });
    }
}
