//! The egui application: picks the view and routes backend events to it.

use airdash_core::api::{Device, DeviceSensors};
use airdash_core::poller::{PollerEvent, PollerStatus};
use airdash_core::settings::Settings;

use crate::backend::{AuthInfo, Backend};
use crate::theme;
use crate::tray::Tray;
use crate::views::settings::{self, SettingsView};
use crate::views::{dashboard, device::DeviceView, login::LoginView, status_bar};

enum View {
    Dashboard,
    Device(DeviceView),
    Settings(SettingsView),
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
    /// `None` when the platform has no tray, in which case closing the window quits.
    tray: Option<Tray>,
    /// Explaining, the first time the window is closed, that AirDash keeps running in the tray.
    close_notice: bool,
}

impl AirDashApp {
    pub fn new(ctx: &egui::Context, backend: Backend) -> Self {
        theme::install(ctx);
        theme::apply(ctx, &backend.settings().theme);
        let tray = Tray::create(ctx, backend.poller.clone())
            .inspect_err(|e| eprintln!("AirDash: no tray icon: {e}"))
            .ok();
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
            tray,
            close_notice: false,
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
                    if let Some(tray) = &self.tray {
                        tray.set_paused(status.paused);
                    }
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
                    let open = matches!(self.view, View::Settings(_));
                    if ui.selectable_label(open, "Settings").clicked() && !open {
                        self.view = View::Settings(SettingsView::new(&self.settings));
                    }
                });
            });
            ui.add_space(6.0);
        });
    }
}

impl AirDashApp {
    fn close_notice_ui(&mut self, ctx: &egui::Context) {
        enum Choice {
            KeepRunning,
            Quit,
        }
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("close-notice")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading("AirDash keeps recording");
            ui.add_space(4.0);
            ui.label(
                "Closing the window keeps AirDash running in the background, so it can keep recording your readings. \
                 Open it again from the AirDash icon in the system tray, or by launching AirDash again. \
                 To stop it completely, choose Quit in the tray icon's menu.",
            );
            ui.add_space(4.0);
            ui.weak("You can make closing the window quit AirDash in Settings, under Recording.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Keep running in the background").clicked() {
                    choice = Some(Choice::KeepRunning);
                }
                if ui.button("Quit AirDash").clicked() {
                    choice = Some(Choice::Quit);
                }
            });
        });
        if choice.is_none() {
            // Escape or a click outside just keeps the window open.
            if modal.should_close() {
                self.close_notice = false;
            }
            return;
        }

        self.close_notice = false;
        let settings = Settings { tray_notice_seen: true, ..self.settings.clone() };
        match self.backend.save_settings(settings) {
            Ok(saved) => self.settings = saved,
            Err(e) => self.error = Some(e.to_string()),
        }
        match choice {
            Some(Choice::KeepRunning) => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false)),
            _ => {
                if let Some(tray) = &self.tray {
                    tray.quitting.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for AirDashApp {
    // Only the window's size and position are restored on the next launch, not scroll positions and the like.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    // Runs even while the window is hidden in the tray.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Some(tray) = &self.tray else { return };
        let quitting = tray.quitting.load(std::sync::atomic::Ordering::SeqCst);
        if ctx.input(|i| i.viewport().close_requested()) && self.settings.close_to_tray && !quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.settings.tray_notice_seen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                // Some desktops show no tray icon, so say where AirDash went before hiding it.
                self.close_notice = true;
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_events();
        if self.close_notice {
            self.close_notice_ui(ui.ctx());
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
                    View::Device(view) => match self.devices.iter().find(|d| d.serial_number == view.serial) {
                        Some(device) => view.ui(ui, &self.backend, device, unit, self.readings_version).then_some(View::Dashboard),
                        // The device was removed from the account.
                        None => Some(View::Dashboard),
                    },
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
                    View::Dashboard => match dashboard::ui(ui, &self.devices, &self.readings, self.loaded, unit) {
                        Some(dashboard::Action::Open(serial)) => Some(View::Device(DeviceView::new(serial))),
                        Some(dashboard::Action::Reorder(order)) => {
                            reordered = Some(order);
                            None
                        }
                        None => None,
                    },
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
