//! The egui application: picks the view and routes backend events to it.

use crate::backend::Backend;

pub struct AirDashApp {
    backend: Backend,
}

impl AirDashApp {
    pub fn new(backend: Backend) -> Self {
        AirDashApp { backend }
    }
}

impl eframe::App for AirDashApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _ = self.backend.events();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("AirDash");
        });
    }
}
