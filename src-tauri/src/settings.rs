use crate::error::AppResult;
use crate::review_state::TransmitMode;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// App settings, persisted as JSON in the app config dir — never in a repo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub last_repo: Option<String>,
    /// A folder of repos opened with ⌘O; `P` switches between them
    /// (docs/PLAN.md M14).
    pub workspace_root: Option<String>,
    /// How `f` delivers notes (SPEC §6). Resume is opt-in: picking it is the
    /// opt-in.
    pub transmit_mode: TransmitMode,
    /// Explicit path to the `claude` binary; found on PATH and the usual
    /// install dirs when unset.
    pub claude_path: Option<String>,
    /// The DEADBOLT accents (SPEC §2).
    pub accent: Accent,
    pub scanlines: bool,
    /// Follow macOS, or force light / dark (docs/PLAN.md M12).
    pub theme_mode: ThemeMode,
    /// The dark theme used by `Dark`, and by `System` when macOS is dark.
    pub dark_theme: DarkTheme,
    /// Console drawer height, % of the window.
    pub console_height: u8,
    /// App `O` opens the repo in (`open -a <app>`).
    pub terminal_app: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_repo: None,
            workspace_root: None,
            transmit_mode: TransmitMode::default(),
            claude_path: None,
            accent: Accent::default(),
            scanlines: true,
            theme_mode: ThemeMode::default(),
            dark_theme: DarkTheme::default(),
            console_height: 40,
            terminal_app: "Terminal".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Cyan,
    Green,
    Amber,
    Red,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DarkTheme {
    #[default]
    Deadbolt,
    Ember,
}

pub struct SettingsStore {
    file: PathBuf,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    /// A missing or unreadable file yields defaults; settings are never worth
    /// refusing to start over.
    pub fn load(file: PathBuf) -> Self {
        let settings = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { file, inner: Mutex::new(settings) }
    }

    pub fn get(&self) -> Settings {
        self.inner.lock().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> AppResult<()> {
        let mut settings = self.inner.lock();
        f(&mut settings);
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_vec_pretty(&*settings)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path().join("settings.json"));
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn last_repo_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested").join("settings.json");
        SettingsStore::load(file.clone())
            .update(|s| s.last_repo = Some("/tmp/repo".into()))
            .unwrap();
        let reloaded = SettingsStore::load(file);
        assert_eq!(reloaded.get().last_repo.as_deref(), Some("/tmp/repo"));
    }

    #[test]
    fn m1_settings_file_still_loads_with_clipboard_default() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, r#"{"lastRepo":"/r"}"#).unwrap();
        let s = SettingsStore::load(file).get();
        assert_eq!(s.last_repo.as_deref(), Some("/r"));
        assert_eq!(s.transmit_mode, TransmitMode::Clipboard);
        assert_eq!(s.accent, Accent::Cyan);
        assert!(s.scanlines, "scanlines default on");
        assert_eq!((s.console_height, s.terminal_app.as_str()), (40, "Terminal"));
        assert_eq!((s.theme_mode, s.dark_theme), (ThemeMode::System, DarkTheme::Deadbolt));
    }

    #[test]
    fn m11_settings_file_loads_with_theme_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(
            &file,
            r#"{"lastRepo":"/r","accent":"amber","scanlines":false,"consoleHeight":55,"terminalApp":"iTerm"}"#,
        )
        .unwrap();
        let s = SettingsStore::load(file).get();
        assert_eq!((s.accent, s.scanlines, s.console_height), (Accent::Amber, false, 55));
        assert_eq!((s.theme_mode, s.dark_theme), (ThemeMode::System, DarkTheme::Deadbolt));
    }

    #[test]
    fn theme_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        SettingsStore::load(file.clone())
            .update(|s| {
                s.theme_mode = ThemeMode::Dark;
                s.dark_theme = DarkTheme::Ember;
            })
            .unwrap();
        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(raw.contains(r#""themeMode": "dark""#) && raw.contains(r#""darkTheme": "ember""#), "{raw}");
        let s = SettingsStore::load(file).get();
        assert_eq!((s.theme_mode, s.dark_theme), (ThemeMode::Dark, DarkTheme::Ember));
    }

    #[test]
    fn m13_settings_file_loads_with_no_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, r#"{"lastRepo":"/r","themeMode":"dark","terminalApp":"iTerm"}"#).unwrap();
        let s = SettingsStore::load(file.clone()).get();
        assert_eq!((s.last_repo.as_deref(), s.workspace_root.as_deref()), (Some("/r"), None));
        SettingsStore::load(file.clone()).update(|s| s.workspace_root = Some("/ws".into())).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains(r#""workspaceRoot": "/ws""#));
        assert_eq!(SettingsStore::load(file).get().workspace_root.as_deref(), Some("/ws"));
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{not json").unwrap();
        assert_eq!(SettingsStore::load(file).get(), Settings::default());
    }
}
