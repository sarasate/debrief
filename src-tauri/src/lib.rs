mod error;
mod flags;
mod git;
mod noise;
mod review;
mod review_state;
mod settings;
mod state;
mod transcript;
mod watcher;

use error::{AppError, AppResult};
use git::types::*;
use review::ReviewModel;
use review_state::{ReviewStore, Verdict, NO_SESSION};
use settings::SettingsStore;
use transcript::sessions::{list_sessions, SessionInfo, TranscriptCache};
use transcript::{projects_dir, repo_roots, slug, Ledger};
use state::{discover_path, RepoState};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};
use watcher::RepoWatcher;

const REPO_CHANGED: &str = "repo://changed";

/// Point the app at the repo containing `path` and start watching it.
fn attach(path: &Path, state: &RepoState, watcher: &RepoWatcher, app: AppHandle) -> AppResult<RepoInfo> {
    let workdir = discover_path(path)?;
    state.set_path(workdir.clone());
    let emitter = app.clone();
    watcher.watch(&workdir, move || {
        let _ = emitter.emit(REPO_CHANGED, ());
    })?;
    if let Some(projects) = projects_dir() {
        let slugs = repo_roots(&workdir).iter().map(|r| slug(r)).collect();
        watcher.watch_transcripts(&projects, slugs, move || {
            let _ = app.emit(REPO_CHANGED, ());
        })?;
    }
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

/// Sessions for the open repo, newest first.
#[tauri::command]
fn sessions_list(state: State<RepoState>, cache: State<TranscriptCache>) -> AppResult<Vec<SessionInfo>> {
    let Some(projects) = projects_dir() else { return Ok(vec![]) };
    let roots = repo_roots(&state.path()?);
    Ok(list_sessions(&cache, &projects, &roots)?.into_iter().map(|s| s.0).collect())
}

#[tauri::command]
fn ledger_load(session_id: String, state: State<RepoState>, cache: State<TranscriptCache>) -> AppResult<Ledger> {
    let projects = projects_dir().ok_or(AppError::Other("no Claude config dir".into()))?;
    let roots = repo_roots(&state.path()?);
    list_sessions(&cache, &projects, &roots)?
        .into_iter()
        .find(|s| s.0.id == session_id)
        .map(|s| s.1.ledger())
        .ok_or_else(|| AppError::Input(format!("no session {session_id} for this repo")))
}

/// Files grouped by intent for `session_id`, or for the newest session.
#[tauri::command]
fn review_model(
    session_id: Option<String>,
    state: State<RepoState>,
    cache: State<TranscriptCache>,
    store: State<ReviewStore>,
) -> AppResult<ReviewModel> {
    review::review_model(&state, &cache, &store, projects_dir().as_deref(), session_id.as_deref())
}

/// Mark a file viewed at the blob oid the reviewer saw (`oid`, from the
/// model), or unmark it. `session_id` is the model's `stateKey`.
#[tauri::command]
fn review_set_viewed(
    session_id: Option<String>,
    path: String,
    oid: Option<String>,
    viewed: bool,
    state: State<RepoState>,
    store: State<ReviewStore>,
) -> AppResult<()> {
    let repo = state.open()?;
    let key = session_id.unwrap_or_else(|| NO_SESSION.into());
    let oid = match oid {
        Some(o) => o,
        None => git::status::worktree_oid(&state.path()?, &path),
    };
    store.update(repo.path(), &key, |s| {
        if viewed {
            s.viewed.insert(path, oid);
        } else {
            s.viewed.remove(&path);
        }
    })
}

#[tauri::command]
fn review_set_verdict(
    session_id: Option<String>,
    hunk_id: String,
    verdict: Option<Verdict>,
    state: State<RepoState>,
    store: State<ReviewStore>,
) -> AppResult<()> {
    let repo = state.open()?;
    let key = session_id.unwrap_or_else(|| NO_SESSION.into());
    store.update(repo.path(), &key, |s| match verdict {
        Some(v) => {
            s.verdicts.insert(hunk_id, v);
        }
        None => {
            s.verdicts.remove(&hunk_id);
        }
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(RepoState::new())
        .manage(RepoWatcher::new())
        .manage(TranscriptCache::new())
        .manage(ReviewStore::new())
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
        .invoke_handler(tauri::generate_handler![
            repo_open,
            repo_current,
            repo_status,
            diff_file,
            sessions_list,
            ledger_load,
            review_model,
            review_set_viewed,
            review_set_verdict,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("debrief: {e}");
        std::process::exit(1);
    }
}
