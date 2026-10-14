//! Light and dark palettes matching the web version, and the user's theme choice.

use egui::{Color32, Context, FontId, Stroke, Style, TextStyle, Theme, ThemePreference};

struct Palette {
    bg: Color32,
    surface: Color32,
    surface_2: Color32,
    border: Color32,
    text: Color32,
    accent: Color32,
}

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf5, 0xf5, 0xf3),
    surface: Color32::from_rgb(0xfc, 0xfc, 0xfb),
    surface_2: Color32::from_rgb(0xf0, 0xef, 0xec),
    border: Color32::from_rgb(0xe2, 0xe1, 0xdc),
    text: Color32::from_rgb(0x0b, 0x0b, 0x0b),
    accent: Color32::from_rgb(0x2a, 0x78, 0xd6),
};

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x12, 0x12, 0x11),
    surface: Color32::from_rgb(0x1a, 0x1a, 0x19),
    surface_2: Color32::from_rgb(0x26, 0x26, 0x24),
    border: Color32::from_rgb(0x33, 0x33, 0x2f),
    text: Color32::from_rgb(0xff, 0xff, 0xff),
    accent: Color32::from_rgb(0x39, 0x87, 0xe5),
};

/// Install both palettes and a slightly larger type scale than egui's default.
pub fn install(ctx: &Context) {
    ctx.style_mut_of(Theme::Light, |style| customize(style, &LIGHT));
    ctx.style_mut_of(Theme::Dark, |style| customize(style, &DARK));
}

fn customize(style: &mut Style, p: &Palette) {
    let v = &mut style.visuals;
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.faint_bg_color = p.surface_2;
    v.extreme_bg_color = p.surface_2;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke.color = p.text;

    use egui::FontFamily::{Monospace, Proportional};
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, Proportional)),
        (TextStyle::Body, FontId::new(14.0, Proportional)),
        (TextStyle::Button, FontId::new(14.0, Proportional)),
        (TextStyle::Heading, FontId::new(20.0, Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, Monospace)),
    ]
    .into();
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
}

pub fn preference(theme: &str) -> ThemePreference {
    match theme {
        "light" => ThemePreference::Light,
        "dark" => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

pub fn apply(ctx: &Context, theme: &str) {
    ctx.set_theme(preference(theme));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_settings_to_theme_preferences() {
        assert_eq!(preference("light"), ThemePreference::Light);
        assert_eq!(preference("dark"), ThemePreference::Dark);
        assert_eq!(preference("system"), ThemePreference::System);
        assert_eq!(preference("bogus"), ThemePreference::System);
    }
}
