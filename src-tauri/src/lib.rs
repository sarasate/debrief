mod actions;
mod claude;
mod error;
mod flags;
mod git;
mod noise;
mod notes;
mod review;
mod review_state;
mod settings;
mod state;
mod time;
mod transcript;
mod watcher;

use error::{AppError, AppResult};
use git::types::*;
use review::ReviewModel;
use claude::ClaudeRunner;
use review_state::{Note, ReviewStore, TransmitMode, Verdict, NO_SESSION};
use serde::Serialize;
use settings::{Accent, Settings};
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

/// Local branches for the picker, with the detected base.
#[tauri::command]
fn target_list(state: State<RepoState>) -> AppResult<git::target::TargetList> {
    git::target::list(&state.open()?)
}

/// Switch what is reviewed. Returns the resolved branch, or None for the
/// working tree; an unresolvable branch leaves the target unchanged.
#[tauri::command]
fn target_set(target: git::target::Target, state: State<RepoState>) -> AppResult<Option<git::target::Range>> {
    let range = git::target::resolve(&state.open()?, &target)?;
    state.set_target(target)?;
    Ok(range)
}

/// `a`: stage every cleared file, minus hunks marked revert.
#[tauri::command]
fn stage_cleared(
    session_id: String,
    state: State<RepoState>,
    store: State<ReviewStore>,
) -> AppResult<git::apply::StageResult> {
    let repo = state.open()?;
    let range = git::target::resolve(&repo, &state.target())?;
    actions::stage_cleared(&repo, &state.path()?, &store, &session_id, range.as_ref())
}

/// `d`, after the UI's confirmation: reverse-apply hunks marked revert.
#[tauri::command]
fn discard_reverted(
    session_id: String,
    state: State<RepoState>,
    store: State<ReviewStore>,
) -> AppResult<git::apply::DiscardResult> {
    let repo = state.open()?;
    let range = git::target::resolve(&repo, &state.target())?;
    actions::discard_reverted(&repo, &state.path()?, &store, &session_id, range.as_ref())
}

#[tauri::command]
fn notes_add(
    session_id: String,
    path: String,
    hunk_id: Option<String>,
    hunk_header: Option<String>,
    text: String,
    state: State<RepoState>,
    store: State<ReviewStore>,
) -> AppResult<Note> {
    let hunk = hunk_id.zip(hunk_header);
    notes::add(&store, state.open()?.path(), &session_id, &path, hunk, &text)
}

#[tauri::command]
fn notes_remove(session_id: String, id: String, state: State<RepoState>, store: State<ReviewStore>) -> AppResult<()> {
    notes::remove(&store, state.open()?.path(), &session_id, &id)
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum TransmitOutcome {
    Copied { notes: usize },
    Written { notes: usize, path: String },
    Started { notes: usize },
    /// Resume only: the session looks active; call again with `force`.
    Confirm { message: String },
}

const ACTIVE_SESSION_MS: i64 = 60_000;
const TRANSMIT_OUTPUT: &str = "transmit://output";
const TRANSMIT_DONE: &str = "transmit://done";
const REVIEW_UPDATED: &str = "review://updated";

#[derive(Clone, Serialize)]
struct OutputLine {
    stream: &'static str,
    line: String,
}

#[derive(Clone, Serialize)]
struct Done {
    code: Option<i32>,
    sent: bool,
}

/// `f`: deliver the queued notes in the configured mode (SPEC §6).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn notes_transmit(
    session_id: String,
    force: bool,
    state: State<RepoState>,
    store: State<ReviewStore>,
    settings: State<SettingsStore>,
    cache: State<TranscriptCache>,
    runner: State<ClaudeRunner>,
    app: AppHandle,
) -> AppResult<TransmitOutcome> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let repo = state.open()?;
    let git_dir = repo.path().to_path_buf();
    let workdir = state.path()?;
    let batch = notes::prepare(&store, &git_dir, &session_id)?;
    let count = batch.note_ids.len();
    let mode = settings.get().transmit_mode;
    let copy = |text: &str| app.clipboard().write_text(text.to_string()).map_err(|e| AppError::Other(format!("clipboard: {e}")));
    match mode {
        TransmitMode::Clipboard => {
            copy(&batch.prompt)?;
            notes::mark_sent(&store, &git_dir, &session_id, &batch.note_ids, batch.discards(), mode)?;
            Ok(TransmitOutcome::Copied { notes: count })
        }
        TransmitMode::File => {
            let (file, pointer) = notes::write_feedback(&git_dir, &workdir, &batch.prompt)?;
            copy(&pointer)?;
            notes::mark_sent(&store, &git_dir, &session_id, &batch.note_ids, batch.discards(), mode)?;
            Ok(TransmitOutcome::Written { notes: count, path: file.to_string_lossy().to_string() })
        }
        TransmitMode::Resume => {
            if session_id == NO_SESSION {
                return Err(AppError::Input("resume needs a linked Claude session".into()));
            }
            review_state::state_file(&git_dir, &session_id)?; // validates the id
            let projects = projects_dir().ok_or(AppError::Other("no Claude config dir".into()))?;
            let session = list_sessions(&cache, &projects, &repo_roots(&workdir))?
                .into_iter()
                .map(|s| s.0)
                .find(|s| s.id == session_id)
                .ok_or_else(|| AppError::Input(format!("session {session_id} not found for this repo")))?;
            let age = time::now_ms() - session.updated_at;
            if age < ACTIVE_SESSION_MS && !force {
                return Ok(TransmitOutcome::Confirm {
                    message: format!("session written {}s ago, it may still be running", age.max(0) / 1000),
                });
            }
            let bin = claude::find_claude(settings.get().claude_path.as_deref())
                .ok_or_else(|| AppError::Other("claude CLI not found; set claudePath in settings".into()))?;
            let cwd = if session.cwd.is_empty() { workdir.clone() } else { std::path::PathBuf::from(&session.cwd) };
            let args = vec!["--resume".to_string(), session_id.clone(), "-p".to_string(), batch.prompt.clone()];
            let (out_app, done_app) = (app.clone(), app.clone());
            let ids = batch.note_ids.clone();
            let discards = batch.discards().to_vec();
            runner.spawn(
                &bin,
                &args,
                &cwd,
                move |stream, line| {
                    let _ = out_app.emit(TRANSMIT_OUTPUT, OutputLine { stream, line });
                },
                move |code| {
                    // Only a clean exit takes the notes off the queue.
                    let sent = code == Some(0)
                        && notes::mark_sent(&done_app.state::<ReviewStore>(), &git_dir, &session_id, &ids, &discards, mode).is_ok();
                    let _ = done_app.emit(TRANSMIT_DONE, Done { code, sent });
                    let _ = done_app.emit(REVIEW_UPDATED, ());
                },
            )?;
            Ok(TransmitOutcome::Started { notes: count })
        }
    }
}

#[tauri::command]
fn transmit_cancel(runner: State<ClaudeRunner>) -> AppResult<bool> {
    Ok(runner.cancel())
}

#[tauri::command]
fn settings_get(settings: State<SettingsStore>) -> AppResult<Settings> {
    Ok(settings.get())
}

#[tauri::command]
fn settings_set(
    transmit_mode: Option<TransmitMode>,
    claude_path: Option<String>,
    accent: Option<Accent>,
    scanlines: Option<bool>,
    settings: State<SettingsStore>,
) -> AppResult<Settings> {
    if let Some(p) = claude_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        if !std::path::Path::new(p).is_file() {
            return Err(AppError::Input(format!("{p} is not a file")));
        }
    }
    settings.update(|s| {
        if let Some(a) = accent {
            s.accent = a;
        }
        if let Some(on) = scanlines {
            s.scanlines = on;
        }
        if let Some(m) = transmit_mode {
            s.transmit_mode = m;
        }
        if let Some(p) = claude_path {
            s.claude_path = (!p.trim().is_empty()).then(|| p.trim().to_string());
        }
    })?;
    Ok(settings.get())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(RepoState::new())
        .manage(RepoWatcher::new())
        .manage(TranscriptCache::new())
        .manage(ReviewStore::new())
        .manage(ClaudeRunner::new())
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
            target_list,
            target_set,
            stage_cleared,
            discard_reverted,
            notes_add,
            notes_remove,
            notes_transmit,
            transmit_cancel,
            settings_get,
            settings_set,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("debrief: {e}");
        std::process::exit(1);
    }
}
