//! Field notes and the transmit prompt (SPEC §6). Notes live in the review
//! state; a transmission records which notes it carried, and the queue is
//! whatever no transmission names.

use crate::error::{AppError, AppResult};
use crate::review_state::{DiscardRecord, Note, ReviewStore, TransmitMode, Transmission, NO_SESSION, STATE_DIR};
use crate::time::{now_iso, now_ms};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub const MAX_NOTE_CHARS: usize = 4000;
pub const FEEDBACK_FILE: &str = "feedback.md";

static SEQ: AtomicU32 = AtomicU32::new(0);

fn note_id() -> String {
    format!("n{:x}{:02x}", now_ms(), SEQ.fetch_add(1, Ordering::Relaxed) % 256)
}

pub fn add(
    store: &ReviewStore,
    git_dir: &Path,
    key: &str,
    path: &str,
    hunk: Option<(String, String)>,
    text: &str,
) -> AppResult<Note> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::Input("empty note".into()));
    }
    if text.chars().count() > MAX_NOTE_CHARS {
        return Err(AppError::Input(format!("notes are capped at {MAX_NOTE_CHARS} characters")));
    }
    let (hunk_id, hunk_header) = hunk.map_or((None, None), |(id, h)| (Some(id), Some(h)));
    let note = Note { id: note_id(), path: path.into(), hunk_id, hunk_header, text: text.into(), created_at: now_iso() };
    store.update(git_dir, key, |s| s.notes.push(note.clone()))?;
    Ok(note)
}

/// Only queued notes can be removed; what was sent stays on record.
pub fn remove(store: &ReviewStore, git_dir: &Path, key: &str, id: &str) -> AppResult<()> {
    store.update(git_dir, key, |s| {
        let queued: Vec<String> = s.queued_notes().into_iter().map(|n| n.id).collect();
        s.notes.retain(|n| n.id != id || !queued.contains(&n.id));
    })
}

/// `@@ -8,9 +11,13 @@ fn ctx` → `@@ -8,9 +11,13 @@`.
fn ranges(header: &str) -> &str {
    match header.get(2..).and_then(|rest| rest.find("@@")) {
        Some(i) => &header[..i + 4],
        None => header,
    }
}

/// The prompt, exactly as SPEC §6 lays it out.
pub fn build_prompt(session: Option<&str>, notes: &[Note], discards: &[DiscardRecord]) -> String {
    let n = notes.len();
    let who = match session {
        Some(id) => format!("session {id}"),
        None => "no linked session".into(),
    };
    let mut out = format!("Review feedback from Debrief ({who}, {n} note{}):\n\n", if n == 1 { "" } else { "s" });
    for (i, note) in notes.iter().enumerate() {
        let scope = match &note.hunk_header {
            Some(h) => format!(" (hunk {})", ranges(h)),
            None => String::new(),
        };
        out.push_str(&format!("{}. {}{scope}\n", i + 1, note.path));
        for line in note.text.lines() {
            out.push_str(&format!("   {line}\n"));
        }
    }
    let reverted = if discards.is_empty() {
        "none".to_string()
    } else {
        discards.iter().map(|d| format!("{} (hunk {})", d.path, ranges(&d.header))).collect::<Vec<_>>().join("; ")
    };
    if n > 0 {
        out.push('\n');
    }
    out.push_str(&format!("Reverted hunks (already discarded from the working tree): {reverted}.\n"));
    out.push_str("Please address the notes, then stop so I can review again.\n");
    out
}

/// What one transmission will carry.
pub struct Batch {
    pub prompt: String,
    pub note_ids: Vec<String>,
    discards: Vec<DiscardRecord>,
}

pub fn prepare(store: &ReviewStore, git_dir: &Path, key: &str) -> AppResult<Batch> {
    let st = store.load(git_dir, key)?;
    let notes = st.queued_notes();
    let discards = st.unreported_discards();
    if notes.is_empty() && discards.is_empty() {
        return Err(AppError::Input("nothing to transmit: queue a note first".into()));
    }
    let session = (key != NO_SESSION).then_some(key);
    Ok(Batch { prompt: build_prompt(session, &notes, &discards), note_ids: notes.iter().map(|n| n.id.clone()).collect(), discards })
}

/// Record the batch as sent: its notes leave the queue, its discards are
/// reported.
pub fn mark_sent(store: &ReviewStore, git_dir: &Path, key: &str, batch_ids: &[String], discards: &[DiscardRecord], mode: TransmitMode) -> AppResult<()> {
    store.update(git_dir, key, |s| {
        s.transmitted.push(Transmission { at: now_iso(), note_ids: batch_ids.to_vec(), mode });
        for d in s.discarded.iter_mut() {
            if discards.iter().any(|x| x.at == d.at && x.path == d.path && x.header == d.header) {
                d.reported = true;
            }
        }
    })
}

impl Batch {
    pub fn discards(&self) -> &[DiscardRecord] {
        &self.discards
    }
}

/// File mode: write `<git dir>/debrief/feedback.md`; return its path and
/// the one-liner to paste, relative to the repo when the git dir is inside it.
pub fn write_feedback(git_dir: &Path, workdir: &Path, prompt: &str) -> AppResult<(PathBuf, String)> {
    let dir = git_dir.join(STATE_DIR);
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(FEEDBACK_FILE);
    std::fs::write(&file, prompt)?;
    let shown = file
        .strip_prefix(workdir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string_lossy().to_string());
    Ok((file, format!("Read {shown} and address it.")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, header: Option<&str>, text: &str) -> Note {
        Note {
            id: "x".into(),
            path: path.into(),
            hunk_id: header.map(|_| "h".into()),
            hunk_header: header.map(String::from),
            text: text.into(),
            created_at: String::new(),
        }
    }

    #[test]
    fn prompt_matches_the_spec_layout() {
        let notes = [
            note(
                "apps/api/src/auth/auth.guard.ts",
                Some("@@ -8,9 +11,13 @@ async canActivate(ctx: ExecutionContext)"),
                "Add a users.realm column + Drizzle migration before relying on session.user.realm.",
            ),
            note(".env.example", None, "Add these to Vercel and 1Password.\nThen tell me."),
        ];
        let discards = [DiscardRecord { at: "t".into(), path: "pnpm-lock.yaml".into(), header: "@@ -212,5 +212,5 @@ importers".into(), reported: false }];
        let p = build_prompt(Some("8121a8ef"), &notes, &discards);
        assert_eq!(
            p,
            "Review feedback from Debrief (session 8121a8ef, 2 notes):\n\
             \n\
             1. apps/api/src/auth/auth.guard.ts (hunk @@ -8,9 +11,13 @@)\n\
             \x20  Add a users.realm column + Drizzle migration before relying on session.user.realm.\n\
             2. .env.example\n\
             \x20  Add these to Vercel and 1Password.\n\
             \x20  Then tell me.\n\
             \n\
             Reverted hunks (already discarded from the working tree): pnpm-lock.yaml (hunk @@ -212,5 +212,5 @@).\n\
             Please address the notes, then stop so I can review again.\n"
        );
    }

    #[test]
    fn prompt_without_reverts_or_session() {
        let p = build_prompt(None, &[note("a.ts", None, "x")], &[]);
        assert!(p.starts_with("Review feedback from Debrief (no linked session, 1 note):"));
        assert!(p.contains("working tree): none.\n"));
    }

    #[test]
    fn add_transmit_and_queue_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new();
        let g = dir.path();
        assert!(add(&store, g, "s1", "a.ts", None, "   ").is_err());
        let n1 = add(&store, g, "s1", "a.ts", Some(("h1".into(), "@@ -1,2 +1,3 @@".into())), " fix it ").unwrap();
        assert_eq!(n1.text, "fix it");
        let n2 = add(&store, g, "s1", "b.ts", None, "and this").unwrap();
        assert_ne!(n1.id, n2.id);

        remove(&store, g, "s1", &n2.id).unwrap();
        let batch = prepare(&store, g, "s1").unwrap();
        assert_eq!(batch.note_ids, [n1.id.clone()]);
        assert!(batch.prompt.contains("1. a.ts (hunk @@ -1,2 +1,3 @@)"));

        mark_sent(&store, g, "s1", &batch.note_ids, batch.discards(), TransmitMode::Clipboard).unwrap();
        let st = store.load(g, "s1").unwrap();
        assert!(st.queued_notes().is_empty());
        assert_eq!(st.transmitted.len(), 1);
        assert!(prepare(&store, g, "s1").is_err(), "queue is empty");

        remove(&store, g, "s1", &n1.id).unwrap();
        assert_eq!(store.load(g, "s1").unwrap().notes.len(), 1, "sent notes stay on record");
    }

    #[test]
    fn discards_are_reported_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new();
        let g = dir.path();
        store
            .update(g, "s1", |s| {
                s.discarded.push(DiscardRecord { at: "t1".into(), path: "a.ts".into(), header: "@@ -1 +1 @@".into(), reported: false })
            })
            .unwrap();
        let batch = prepare(&store, g, "s1").unwrap();
        assert!(batch.prompt.contains("a.ts (hunk @@ -1 +1 @@)"));
        mark_sent(&store, g, "s1", &batch.note_ids, batch.discards(), TransmitMode::File).unwrap();
        assert!(prepare(&store, g, "s1").is_err());
    }

    #[test]
    fn feedback_file_is_pointed_at_relative_to_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        let (file, line) = write_feedback(&git, dir.path(), "hello").unwrap();
        assert_eq!(std::fs::read_to_string(file).unwrap(), "hello");
        assert_eq!(line, "Read .git/debrief/feedback.md and address it.");
    }
}
