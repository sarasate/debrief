mod error;
mod git;
mod settings;
mod state;
mod watcher;

use error::{AppError, AppResult};
use git::types::*;
use settings::SettingsStore;
use state::{discover_path, RepoState};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};
use watcher::RepoWatcher;

const REPO_CHANGED: &str = "repo://changed";

/// Point the app at the repo containing `path` and start watching it.
fn attach(path: &Path, state: &RepoState, watcher: &RepoWatcher, app: AppHandle) -> AppResult<RepoInfo> {
    let workdir = discover_path(path)?;
    state.set_path(workdir.clone());
    watcher.watch(&workdir, move || {
        let _ = app.emit(REPO_CHANGED, ());
    })?;
    Ok(git::status::repo_info(&workdir))
}

#[tauri::command]
fn repo_open(
    path: String,
    state: State<RepoState>,
    watcher: State<RepoWatcher>,
    settings: State<SettingsStore>,
    app: AppHandle,
) -> AppResult<RepoInfo> {
    let info = attach(Path::new(&path), &state, &watcher, app)?;
    settings.update(|s| s.last_repo = Some(info.workdir.clone()))?;
    Ok(info)
}

/// The repo restored at startup (or opened since), if any.
#[tauri::command]
fn repo_current(state: State<RepoState>) -> AppResult<Option<RepoInfo>> {
    if !state.is_open() {
        return Ok(None);
    }
    Ok(Some(git::status::repo_info(&state.path()?)))
}

#[tauri::command]
fn repo_status(state: State<RepoState>) -> AppResult<RepoStatus> {
    git::status::repo_status(&state)
}

#[tauri::command]
fn diff_file(path: String, state: State<RepoState>) -> AppResult<FileDiff> {
    git::diff::diff_file(&state, &path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(RepoState::new())
        .manage(RepoWatcher::new())
        .setup(|app| {
            let dir = app
                .path()
                .app_config_dir()
                .map_err(|e| AppError::Other(e.to_string()))?;
            let settings = SettingsStore::load(dir.join("settings.json"));
            // A last-used repo that has since moved or vanished just means we
            // start on the open-repo screen.
            if let Some(last) = settings.get().last_repo {
                let state: State<RepoState> = app.state();
                let watcher: State<RepoWatcher> = app.state();
                let _ = attach(Path::new(&last), &state, &watcher, app.handle().clone());
            }
            app.manage(settings);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![repo_open, repo_current, repo_status, diff_file])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("debrief: {e}");
        std::process::exit(1);
    }
}
