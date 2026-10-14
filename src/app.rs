//! The egui application: picks the view and routes backend events to it.

use airdash_core::api::{Device, DeviceSensors};
use airdash_core::poller::{PollerEvent, PollerStatus};
use airdash_core::settings::Settings;
use egui_winit::WindowSettings;

use crate::backend::{AuthInfo, Backend};
use crate::theme;
use crate::views::settings::{self, SettingsView};
use crate::views::{dashboard, login::LoginView, status_bar};

enum View {
    Dashboard,
    Settings(Box<SettingsView>),
}

pub struct AirDashApp {
    backend: Backend,
    auth: AuthInfo,
    settings: Settings,
    status: PollerStatus,
    devices: Vec<Device>,
    readings: Vec<DeviceSensors>,
    loaded: bool,
    /// New devices or readings arrived since the dashboard data was last loaded.
    data_stale: bool,
    error: Option<String>,
    view: View,
    login: LoginView,
    /// The window's size and position as of the last frame it was not minimized, saved in place of
    /// eframe's own snapshot: a minimized window reports a 0x0 size far off screen.
    window: Option<WindowSettings>,
}

/// The key eframe saves the window's size and position under, just before it calls [`eframe::App::save`].
const WINDOW_STATE_KEY: &str = "window";

impl AirDashApp {
    pub fn new(ctx: &egui::Context, backend: Backend, storage: Option<&dyn eframe::Storage>) -> Self {
        theme::install(ctx);
        theme::apply(ctx, &backend.settings().theme);
        let mut app = AirDashApp {
            auth: backend.auth_info(),
            settings: backend.settings(),
            status: backend.poller.status(),
            backend,
            devices: Vec::new(),
            readings: Vec::new(),
            loaded: false,
            data_stale: false,
            error: None,
            view: View::Dashboard,
            login: LoginView::default(),
            window: storage.and_then(|s| eframe::get_value(s, WINDOW_STATE_KEY)),
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
                // Reloaded once on the next drawn frame, however many updates arrive while the window is hidden.
                PollerEvent::DevicesUpdated => self.data_stale = true,
                PollerEvent::ReadingsUpdated => self.data_stale = true,
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
                    let open = matches!(self.view, View::Settings(_));
                    if ui.selectable_label(open, "Settings").clicked() && !open {
                        self.view = View::Settings(Box::new(SettingsView::new(&self.settings)));
                    }
                });
            });
            ui.add_space(6.0);
        });
    }
}

impl eframe::App for AirDashApp {
    // Only the window's size and position are restored on the next launch, not scroll positions and the like.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(window) = &self.window {
            eframe::set_value(storage, WINDOW_STATE_KEY, window);
        }
    }

    // Runs even while the window is minimized.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(window) = frame.winit_window().filter(|w| w.is_minimized() != Some(true)) {
            self.window = Some(WindowSettings::from_window(ctx.zoom_factor(), window));
        }
        self.handle_events();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.data_stale) {
            self.load_data();
        }

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
                let mut signed_out = false;
                let mut reordered = None;
                let next = match &mut self.view {
                    View::Settings(view) => match view.ui(ui, &self.backend, &self.auth) {
                        Some(settings::Action::Back) => Some(View::Dashboard),
                        Some(settings::Action::Saved(saved)) => {
                            theme::apply(ui.ctx(), &saved.theme);
                            self.settings = saved;
                            None
                        }
                        Some(settings::Action::SignedOut) => {
                            signed_out = true;
                            None
                        }
                        None => None,
                    },
                    View::Dashboard => {
                        reordered = dashboard::ui(ui, &self.devices, &self.readings, self.loaded, unit);
                        None
                    }
                };
                if let Some(next) = next {
                    self.view = next;
                }
                if signed_out {
                    self.signed_out();
                }
                if let Some(order) = reordered {
                    match self.backend.store.set_device_order(&order) {
                        Ok(()) => self.load_data(),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            });
        });
    }
}
