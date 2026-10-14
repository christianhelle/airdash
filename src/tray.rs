//! System tray icon, so recording continues while the window is closed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use airdash_core::poller::Poller;
use egui::{Context, ViewportCommand};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    _icon: TrayIcon,
    pause: CheckMenuItem,
    /// Set by "Quit" so the close request is not turned into hiding the window.
    pub quitting: Arc<AtomicBool>,
}

impl Tray {
    /// Must be called on the main thread while the event loop runs.
    pub fn create(ctx: &Context, poller: Arc<Poller>) -> Result<Self, Box<dyn std::error::Error>> {
        let open = MenuItem::with_id("open", "Open AirDash", true, None);
        let refresh = MenuItem::with_id("refresh", "Refresh now", true, None);
        let pause = CheckMenuItem::with_id("pause", "Pause recording", true, false, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);
        let menu = Menu::with_items(&[&open, &refresh, &pause, &PredefinedMenuItem::separator(), &quit])?;

        let icon = eframe::icon_data::from_png_bytes(crate::ICON_PNG)?;
        let icon = tray_icon::Icon::from_rgba(icon.rgba, icon.width, icon.height)?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("AirDash")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;

        let quitting = Arc::new(AtomicBool::new(false));
        let (menu_ctx, menu_quitting) = (ctx.clone(), quitting.clone());
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id().as_ref() {
            "open" => show_window(&menu_ctx),
            "refresh" => poller.refresh_now(),
            "pause" => poller.set_paused(!poller.status().paused),
            "quit" => {
                menu_quitting.store(true, Ordering::SeqCst);
                menu_ctx.send_viewport_cmd(ViewportCommand::Close);
                menu_ctx.request_repaint();
            }
            _ => {}
        }));
        let click_ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(&click_ctx);
            }
        }));

        Ok(Tray { _icon: tray, pause, quitting })
    }

    /// Keep the menu's check mark in step with pausing from the status bar.
    pub fn set_paused(&self, paused: bool) {
        if self.pause.is_checked() != paused {
            self.pause.set_checked(paused);
        }
    }
}

pub fn show_window(ctx: &Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}
