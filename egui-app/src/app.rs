//! The egui application: picks the view and routes backend events to it.

use airdash_core::api::{Device, DeviceSensors};
use airdash_core::poller::{PollerEvent, PollerStatus};
use airdash_core::settings::Settings;

use crate::backend::{AuthInfo, Backend};
use crate::views::{dashboard, device::DeviceView, login::LoginView, status_bar};

enum View {
    Dashboard,
    Device(DeviceView),
    Settings,
}

pub struct AirDashApp {
    backend: Backend,
    auth: AuthInfo,
    settings: Settings,
    status: PollerStatus,
    devices: Vec<Device>,
    readings: Vec<DeviceSensors>,
    loaded: bool,
    /// Bumped on every new batch of readings so open charts reload.
    readings_version: u64,
    error: Option<String>,
    view: View,
    login: LoginView,
}

impl AirDashApp {
    pub fn new(backend: Backend) -> Self {
        let mut app = AirDashApp {
            auth: backend.auth_info(),
            settings: backend.settings(),
            status: backend.poller.status(),
            backend,
            devices: Vec::new(),
            readings: Vec::new(),
            loaded: false,
            readings_version: 0,
            error: None,
            view: View::Dashboard,
            login: LoginView::default(),
        };
        app.refresh_auth();
        app
    }

    fn refresh_auth(&mut self) {
        self.auth = self.backend.auth_info();
        if self.auth.signed_in {
            self.load_data();
        }
    }

    fn load_data(&mut self) {
        let store = &self.backend.store;
        match store.devices().and_then(|d| Ok((d, store.latest()?))) {
            Ok((devices, readings)) => {
                self.devices = devices;
                self.readings = readings;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.loaded = true;
    }

    fn handle_events(&mut self) {
        for event in self.backend.events() {
            match event {
                // A rejected refresh token signs the user out in the background.
                PollerEvent::Status(status) => {
                    let signed_out = self.auth.signed_in && !status.signed_in;
                    self.status = status;
                    if signed_out {
                        self.signed_out();
                    }
                }
                PollerEvent::DevicesUpdated => self.load_data(),
                PollerEvent::ReadingsUpdated => {
                    self.load_data();
                    self.readings_version += 1;
                }
            }
        }
    }

    fn signed_out(&mut self) {
        self.view = View::Dashboard;
        self.devices.clear();
        self.readings.clear();
        self.loaded = false;
        self.refresh_auth();
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("nav").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let brand = egui::RichText::new("AirDash").size(18.0).strong();
                if ui.add(egui::Button::new(brand).frame(false)).clicked() {
                    self.view = View::Dashboard;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(matches!(self.view, View::Settings), "Settings").clicked() {
                        self.view = View::Settings;
                    }
                });
            });
            ui.add_space(6.0);
        });
    }
}

impl eframe::App for AirDashApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_events();

        if !self.auth.signed_in {
            egui::CentralPanel::default().show(ui, |ui| {
                if self.login.ui(ui, &self.backend, &self.auth) {
                    self.login = LoginView::default();
                    self.refresh_auth();
                }
            });
            return;
        }

        self.nav(ui);
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(4.0);
            status_bar::ui(ui, &self.status, &self.backend.poller);
            ui.add_space(4.0);
        });
        // Keep relative times such as "2 min ago" current.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(30));
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let unit = self.settings.unit;
                let next = match &mut self.view {
                    View::Device(view) => match self.devices.iter().find(|d| d.serial_number == view.serial) {
                        Some(device) => view.ui(ui, &self.backend, device, unit, self.readings_version).then_some(View::Dashboard),
                        // The device was removed from the account.
                        None => Some(View::Dashboard),
                    },
                    View::Dashboard | View::Settings => dashboard::ui(ui, &self.devices, &self.readings, self.loaded, unit)
                        .map(|serial| View::Device(DeviceView::new(serial))),
                };
                if let Some(next) = next {
                    self.view = next;
                }
            });
        });
    }
}
