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
    /// How `f` delivers notes (SPEC §6). Resume is opt-in: picking it is the
    /// opt-in.
    pub transmit_mode: TransmitMode,
    /// Explicit path to the `claude` binary; found on PATH and the usual
    /// install dirs when unset.
    pub claude_path: Option<String>,
    /// The DEADBOLT accents (SPEC §2).
    pub accent: Accent,
    pub scanlines: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { last_repo: None, transmit_mode: TransmitMode::default(), claude_path: None, accent: Accent::default(), scanlines: true }
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
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{not json").unwrap();
        assert_eq!(SettingsStore::load(file).get(), Settings::default());
    }
}
