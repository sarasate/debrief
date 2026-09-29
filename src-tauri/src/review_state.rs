//! Review progress (SPEC §4): one JSON file per session in `<git dir>/debrief/`,
//! never in the working tree. Nothing is written until the reviewer acts,
//! so opening a repo stays read-only.

use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// State key when the repo has no Claude session.
pub const NO_SESSION: &str = "worktree";
pub const STATE_DIR: &str = "debrief";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Keep,
    Revert,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewState {
    pub session_id: String,
    /// path -> worktree blob oid when it was marked viewed.
    pub viewed: BTreeMap<String, String>,
    /// hunk id -> verdict.
    pub verdicts: BTreeMap<String, Verdict>,
    /// Every note ever queued; the queue is the ones no transmission names.
    pub notes: Vec<Note>,
    pub transmitted: Vec<Transmission>,
    /// Hunks `d` discarded, so the next transmit can tell Claude.
    pub discarded: Vec<DiscardRecord>,
    /// Branch review: committed hunks marked revert that a transmit already
    /// asked Claude to revert, so each request is sent once.
    pub requested: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransmitMode {
    #[default]
    Clipboard,
    File,
    Resume,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunk_id: Option<String>,
    /// The `@@` header when the note was written; the hunk may be gone later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunk_header: Option<String>,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Transmission {
    pub at: String,
    pub note_ids: Vec<String>,
    pub mode: TransmitMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscardRecord {
    pub at: String,
    pub path: String,
    pub header: String,
    /// Already listed in a transmitted prompt.
    #[serde(default)]
    pub reported: bool,
}

impl ReviewState {
    pub fn queued_notes(&self) -> Vec<Note> {
        let sent: HashSet<&str> = self.transmitted.iter().flat_map(|t| t.note_ids.iter().map(String::as_str)).collect();
        self.notes.iter().filter(|n| !sent.contains(n.id.as_str())).cloned().collect()
    }

    pub fn unreported_discards(&self) -> Vec<DiscardRecord> {
        self.discarded.iter().filter(|d| !d.reported).cloned().collect()
    }
}

/// A changed file as `reconcile` sees it.
pub struct Current<'a> {
    pub path: &'a str,
    pub oid: &'a str,
    pub hunk_ids: &'a [String],
}

impl ReviewState {
    /// Bring the state in line with the worktree. Drops viewed marks whose
    /// file changed since (returned, for "changed since cleared") or is no
    /// longer dirty, and verdicts on hunks that no longer exist.
    pub fn reconcile(&mut self, files: &[Current]) -> Vec<String> {
        let by_path: HashMap<&str, &Current> = files.iter().map(|f| (f.path, f)).collect();
        let mut changed = Vec::new();
        self.viewed.retain(|path, oid| match by_path.get(path.as_str()) {
            Some(f) if f.oid == oid.as_str() => true,
            Some(_) => {
                changed.push(path.clone());
                false
            }
            None => false,
        });
        let live: HashSet<&str> = files.iter().flat_map(|f| f.hunk_ids.iter().map(String::as_str)).collect();
        self.verdicts.retain(|id, _| live.contains(id.as_str()));
        changed
    }
}

/// Serialises read-modify-write of state files.
pub struct ReviewStore {
    lock: Mutex<()>,
}

impl ReviewStore {
    pub fn new() -> Self {
        Self { lock: Mutex::new(()) }
    }

    pub fn load(&self, git_dir: &Path, session: &str) -> AppResult<ReviewState> {
        let _g = self.lock.lock();
        read(&state_file(git_dir, session)?, session)
    }

    /// Apply `f` and write the file only if the state actually changed.
    pub fn update<T>(&self, git_dir: &Path, session: &str, f: impl FnOnce(&mut ReviewState) -> T) -> AppResult<T> {
        let _g = self.lock.lock();
        let file = state_file(git_dir, session)?;
        let mut st = read(&file, session)?;
        let before = st.clone();
        let out = f(&mut st);
        if st != before {
            write(&file, &st)?;
        }
        Ok(out)
    }
}

/// `<git dir>/debrief/<session>.json`. Session ids come from transcript
/// file content, so they're checked before they touch a path.
pub fn state_file(git_dir: &Path, session: &str) -> AppResult<PathBuf> {
    let ok = !session.is_empty()
        && session.len() <= 128
        && session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(AppError::Input(format!("invalid session id {session:?}")));
    }
    Ok(git_dir.join(STATE_DIR).join(format!("{session}.json")))
}

/// Missing file: fresh state. Unreadable JSON: moved aside to
/// `<name>.corrupt-<ms>` so the next write can't destroy it, then fresh.
fn read(file: &Path, session: &str) -> AppResult<ReviewState> {
    let fresh = || ReviewState { session_id: session.to_string(), ..Default::default() };
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(fresh()),
        Err(e) => return Err(e.into()),
    };
    match serde_json::from_slice::<ReviewState>(&bytes) {
        Ok(mut st) => {
            st.session_id = session.to_string();
            Ok(st)
        }
        Err(_) => {
            let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            std::fs::rename(file, file.with_extension(format!("json.corrupt-{ms}")))?;
            Ok(fresh())
        }
    }
}

/// Write to a temp file and rename, so a crash never leaves half a file.
fn write(file: &Path, st: &ReviewState) -> AppResult<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(st)?)?;
    std::fs::rename(&tmp, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cur<'a>(path: &'a str, oid: &'a str, hunks: &'a [String]) -> Current<'a> {
        Current { path, oid, hunk_ids: hunks }
    }

    #[test]
    fn reconcile_drops_changed_and_clean_files_and_dead_hunks() {
        let mut st = ReviewState::default();
        st.viewed.insert("same.ts".into(), "o1".into());
        st.viewed.insert("changed.ts".into(), "o2".into());
        st.viewed.insert("committed.ts".into(), "o3".into());
        st.verdicts.insert("h-live".into(), Verdict::Keep);
        st.verdicts.insert("h-dead".into(), Verdict::Revert);
        let hunks = vec!["h-live".to_string()];
        let files = [cur("same.ts", "o1", &hunks), cur("changed.ts", "o2-new", &[])];

        let changed = st.reconcile(&files);
        assert_eq!(changed, ["changed.ts"], "a clean file is dropped silently");
        assert_eq!(st.viewed.keys().collect::<Vec<_>>(), ["same.ts"]);
        assert_eq!(st.verdicts.keys().collect::<Vec<_>>(), ["h-live"]);
        assert!(st.reconcile(&files).is_empty(), "reported once");
    }

    #[test]
    fn state_round_trips_and_keeps_unknown_sections() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new();
        store
            .update(dir.path(), "s1", |s| {
                s.viewed.insert("a.ts".into(), "oid".into());
                s.verdicts.insert("abc".into(), Verdict::Revert);
                s.notes.push(Note {
                    id: "n1".into(),
                    path: "a.ts".into(),
                    hunk_id: None,
                    hunk_header: None,
                    text: "keep me".into(),
                    created_at: "2026-09-29T10:00:00.000Z".into(),
                });
            })
            .unwrap();
        let st = store.load(dir.path(), "s1").unwrap();
        assert_eq!(st.session_id, "s1");
        assert_eq!(st.viewed["a.ts"], "oid");
        assert_eq!(st.verdicts["abc"], Verdict::Revert);
        assert_eq!(st.notes[0].text, "keep me");

        let raw = std::fs::read_to_string(dir.path().join("debrief/s1.json")).unwrap();
        assert!(raw.contains("\"revert\"") && raw.contains("\"sessionId\""), "{raw}");
        assert!(store.load(dir.path(), "other").unwrap().viewed.is_empty(), "per session");
    }

    #[test]
    fn m4_state_files_still_load() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("debrief")).unwrap();
        std::fs::write(
            dir.path().join("debrief/s1.json"),
            r#"{"sessionId":"s1","viewed":{"a.ts":"o"},"verdicts":{},"notes":[],"transmitted":[]}"#,
        )
        .unwrap();
        let st = ReviewStore::new().load(dir.path(), "s1").unwrap();
        assert_eq!(st.viewed["a.ts"], "o");
        assert!(st.discarded.is_empty());
    }

    #[test]
    fn queue_is_notes_no_transmission_names() {
        let note = |id: &str| Note {
            id: id.into(),
            path: "a.ts".into(),
            hunk_id: None,
            hunk_header: None,
            text: id.into(),
            created_at: String::new(),
        };
        let mut st = ReviewState { notes: vec![note("n1"), note("n2")], ..Default::default() };
        st.transmitted.push(Transmission { at: String::new(), note_ids: vec!["n1".into()], mode: TransmitMode::File });
        assert_eq!(st.queued_notes().iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    #[test]
    fn nothing_is_written_until_something_changes() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new();
        store.update(dir.path(), "s1", |s| s.reconcile(&[])).unwrap();
        assert!(!dir.path().join("debrief").exists());
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("debrief")).unwrap();
        std::fs::write(dir.path().join("debrief/s1.json"), "{broken").unwrap();
        let store = ReviewStore::new();
        assert!(store.load(dir.path(), "s1").unwrap().viewed.is_empty());
        let backups: Vec<_> = std::fs::read_dir(dir.path().join("debrief"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(backups.iter().any(|n| n.starts_with("s1.json.corrupt-")), "{backups:?}");
    }

    #[test]
    fn session_ids_are_validated() {
        for bad in ["", "../x", "a/b", "a.b", &"x".repeat(200)] {
            assert!(state_file(Path::new("/g"), bad).is_err(), "{bad}");
        }
        assert!(state_file(Path::new("/g"), "8121a8ef-3b71-41c6-9e91-882f3ca16198").is_ok());
        assert!(state_file(Path::new("/g"), NO_SESSION).is_ok());
    }
}
