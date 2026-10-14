//! The egui application: picks the view and keeps the dashboard data fresh.

use airdash_core::api::Device;
use airdash_core::error::{Error, Result};
use airdash_core::refresh::Snapshot;
use airdash_core::settings::Settings;
use airdash_core::state::AppState;
use egui_winit::WindowSettings;

use crate::backend::{AuthInfo, Backend, Task};
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
    status: status_bar::Status,
    /// The refresh in flight, if any.
    refresh: Option<Task<Result<Snapshot>>>,
    /// Restored at launch and saved on every change.
    state: AppState,
    /// The state's devices in the user's order.
    devices: Vec<Device>,
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
            status: status_bar::Status::default(),
            refresh: None,
            state: backend.load_state(),
            backend,
            devices: Vec::new(),
            error: None,
            view: View::Dashboard,
            login: LoginView::default(),
            window: storage.and_then(|s| eframe::get_value(s, WINDOW_STATE_KEY)),
        };
        app.restore_state();
        app.refresh_auth();
        app
    }

    /// Show the last known state right away; a refresh follows once it is older than the refresh interval.
    fn restore_state(&mut self) {
        self.devices = self.state.devices();
        self.status.last_refresh = self.state.last_refresh;
        self.status.last_attempt = self.state.last_refresh;
        self.status.rate_limit = self.state.snapshot.rate_limit.clone();
    }

    fn save_state(&mut self) {
        self.devices = self.state.devices();
        self.error = self.backend.save_state(&self.state).err().map(|e| e.to_string());
    }

    fn refresh_auth(&mut self) {
        self.auth = self.backend.auth_info();
    }

    fn start_refresh(&mut self) {
        self.status.refreshing = true;
        self.status.last_attempt = Some(chrono::Utc::now().timestamp());
        self.refresh = Some(self.backend.refresh());
    }

    /// Start a refresh when one is due, and pick up the result of one that finished.
    fn update_refresh(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.refresh.as_ref().and_then(Task::ready) {
            self.refresh = None;
            self.status.refreshing = false;
            match result {
                Ok(snapshot) => {
                    self.status.last_refresh = self.status.last_attempt;
                    self.status.last_error = None;
                    self.status.rate_limit = snapshot.rate_limit.clone();
                    self.state.snapshot = snapshot;
                    self.state.last_refresh = self.status.last_refresh;
                    self.save_state();
                }
                // A rejected refresh token signs the user out.
                Err(Error::Unauthorized) if !self.backend.session.is_signed_in() => self.signed_out(),
                Err(e) => self.status.last_error = Some(e.to_string()),
            }
        }
        if !self.auth.signed_in {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        if self.status.refresh_due(now) {
            self.start_refresh();
        }
        if let Some(next) = self.status.next_refresh() {
            ctx.request_repaint_after(std::time::Duration::from_secs((next - now).max(1) as u64));
        }
    }

    fn signed_out(&mut self) {
        self.view = View::Dashboard;
        self.refresh = None;
        self.status = status_bar::Status::default();
        // The next account sees only its own devices. Their order is kept, as it is per device.
        self.state = AppState { device_order: std::mem::take(&mut self.state.device_order), ..AppState::default() };
        self.save_state();
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

    // Save the window's size and position soon after every move or resize, not only at exit. eframe writes the
    // file only when something changed, so idle seconds cost nothing.
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(1)
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
        self.update_refresh(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            if status_bar::ui(ui, &self.status) && !self.status.refreshing {
                self.start_refresh();
            }
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
                        let loaded = self.status.last_refresh.is_some();
                        reordered = dashboard::ui(ui, &self.devices, &self.state.snapshot.readings, loaded, unit);
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
                    self.state.device_order = order;
                    self.save_state();
                }
            });
        });
    }
}
