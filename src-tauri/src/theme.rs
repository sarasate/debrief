//! The native side of themes (docs/PLAN.md M12): the window's appearance
//! and background, so the title bar matches the page and startup doesn't
//! flash the wrong colour. The page itself resolves the theme in
//! src/lib/theme.ts with the same rules as `resolve`.

use crate::settings::{DarkTheme, Settings, ThemeMode};
use tauri::window::Color;
use tauri::{Theme, WebviewWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Deadbolt,
    Daylight,
    Ember,
}

/// The theme the settings pick, given the system appearance.
pub fn resolve(mode: ThemeMode, dark: DarkTheme, system_dark: bool) -> Resolved {
    let dark = match dark {
        DarkTheme::Deadbolt => Resolved::Deadbolt,
        DarkTheme::Ember => Resolved::Ember,
    };
    match mode {
        ThemeMode::Light => Resolved::Daylight,
        ThemeMode::Dark => dark,
        ThemeMode::System if system_dark => dark,
        ThemeMode::System => Resolved::Daylight,
    }
}

/// `bg.base` of each theme; keep in step with src/styles/themes.css.
fn background(theme: Resolved) -> Color {
    match theme {
        Resolved::Deadbolt => Color(0x06, 0x09, 0x0c, 0xff),
        Resolved::Daylight => Color(0xe9, 0xee, 0xf0, 0xff),
        Resolved::Ember => Color(0x0c, 0x09, 0x07, 0xff),
    }
}

/// Sets the window's appearance (app-wide on macOS, which also drives the
/// webview's `prefers-color-scheme`) and background from the settings.
pub fn apply(window: &WebviewWindow, settings: &Settings) -> tauri::Result<()> {
    window.set_theme(match settings.theme_mode {
        ThemeMode::System => None,
        ThemeMode::Light => Some(Theme::Light),
        ThemeMode::Dark => Some(Theme::Dark),
    })?;
    follow_system(window, settings, window.theme()?)
}

/// Repaints the background for a system appearance change; only `System`
/// mode follows it.
pub fn follow_system(window: &WebviewWindow, settings: &Settings, system: Theme) -> tauri::Result<()> {
    let theme = resolve(settings.theme_mode, settings.dark_theme, system != Theme::Light);
    window.set_background_color(Some(background(theme)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_follows_the_os_and_uses_the_chosen_dark_theme() {
        use DarkTheme::*;
        use ThemeMode::*;
        assert_eq!(resolve(System, Deadbolt, false), Resolved::Daylight);
        assert_eq!(resolve(System, Deadbolt, true), Resolved::Deadbolt);
        assert_eq!(resolve(System, Ember, true), Resolved::Ember);
        assert_eq!(resolve(Light, Ember, true), Resolved::Daylight);
        assert_eq!(resolve(Dark, Ember, false), Resolved::Ember);
        assert_eq!(resolve(Dark, Deadbolt, false), Resolved::Deadbolt);
    }
}
