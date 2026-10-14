//! Light and dark palettes matching the web version, and the user's theme choice.

use egui::{Context, ThemePreference};

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
