use crate::error::AppResult;
use crate::transcript::dir_matches;
use git2::Repository;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(300);

pub struct RepoWatcher {
    repo: Mutex<Option<RecommendedWatcher>>,
    transcripts: Mutex<Option<RecommendedWatcher>>,
}

impl RepoWatcher {
    pub fn new() -> Self {
        Self { repo: Mutex::new(None), transcripts: Mutex::new(None) }
    }

    /// Replaces any previous watch. Dropping the old watcher closes its
    /// channel, which ends its debounce thread.
    pub fn watch(&self, workdir: &Path, on_change: impl Fn() + Send + 'static) -> AppResult<()> {
        let w = spawn(workdir, on_change, |root| {
            let repo = Repository::open(&root).ok();
            move |p: &Path| is_relevant(repo.as_ref(), &root, p)
        })?;
        *self.repo.lock() = Some(w);
        Ok(())
    }

    /// Follow the session transcripts for a repo: every `*.jsonl` under a
    /// project dir that `transcript::dir_matches` the repo's slugs, so a
    /// new Claude write (or a new session) shows up live.
    pub fn watch_transcripts(
        &self,
        projects: &Path,
        slugs: Vec<String>,
        on_change: impl Fn() + Send + 'static,
    ) -> AppResult<()> {
        if !projects.is_dir() {
            *self.transcripts.lock() = None;
            return Ok(());
        }
        let w = spawn(projects, on_change, move |root| move |p: &Path| is_transcript(&root, &slugs, p))?;
        *self.transcripts.lock() = Some(w);
        Ok(())
    }
}

/// Watch `dir` recursively and debounce into `on_change`. The filter is
/// built on the debounce thread, since a git2 Repository can't cross threads
/// by reference.
fn spawn<F>(
    dir: &Path,
    on_change: impl Fn() + Send + 'static,
    make_filter: impl FnOnce(PathBuf) -> F + Send + 'static,
) -> AppResult<RecommendedWatcher>
where
    F: Fn(&Path) -> bool,
{
    let root = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event.paths);
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    std::thread::spawn(move || {
        let relevant = make_filter(root);
        debounce(rx, |paths| paths.iter().any(|p| relevant(p)), on_change);
    });
    Ok(watcher)
}

/// Emit once the tree has been quiet for DEBOUNCE after a relevant change.
/// Irrelevant events (ignored paths, object writes) never extend the wait.
fn debounce(rx: Receiver<Vec<PathBuf>>, relevant: impl Fn(&[PathBuf]) -> bool, on_change: impl Fn()) {
    while let Ok(paths) = rx.recv() {
        if !relevant(&paths) {
            continue;
        }
        let mut deadline = Instant::now() + DEBOUNCE;
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(paths) if relevant(&paths) => deadline = Instant::now() + DEBOUNCE,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        on_change();
    }
}

/// `<projects>/<matching slug>/…/*.jsonl`: a session or subagent transcript.
fn is_transcript(projects: &Path, slugs: &[String], path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(projects) else { return false };
    let Some(first) = rel.components().next() else { return false };
    let dir = first.as_os_str().to_string_lossy();
    dir_matches(&dir, slugs) && path.extension().is_some_and(|e| e == "jsonl")
}

/// Worktree files that git would show, plus the bits of `.git` that change
/// what HEAD or the index look like.
fn is_relevant(repo: Option<&Repository>, root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else { return true };
    if let Ok(inside) = rel.strip_prefix(".git") {
        return inside == Path::new("index")
            || inside == Path::new("HEAD")
            || inside.starts_with("refs");
    }
    match repo {
        Some(r) => !r.is_path_ignored(rel).unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;

    #[test]
    fn filters_git_internals_and_ignored_paths() {
        let fx = Fixture::new();
        fx.commit(&[(".gitignore", "node_modules/\n*.log\n")]);
        let root = fx.root();
        let r = Some(&fx.repo);
        let at = |rel: &str| is_relevant(r, root, &root.join(rel));

        assert!(at("src/app.ts"));
        assert!(at(".git/index"));
        assert!(at(".git/HEAD"));
        assert!(at(".git/refs/heads/main"));
        assert!(!at(".git/objects/ab/cdef"));
        assert!(!at(".git/index.lock"));
        assert!(!at(".git/debrief/session.json"));
        assert!(!at("node_modules/react/index.js"));
        assert!(!at("debug.log"));
    }

    /// Real file events, so allow generous timeouts for FSEvents latency.
    #[test]
    fn save_fires_once_after_the_burst_and_ignored_writes_do_not() {
        let fx = Fixture::new();
        fx.commit(&[(".gitignore", "build/\n"), ("a.txt", "a\n")]);
        let (tx, rx) = mpsc::channel();
        let w = RepoWatcher::new();
        w.watch(fx.root(), move || {
            let _ = tx.send(());
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(300)); // let the watch settle
        while rx.try_recv().is_ok() {}

        fx.write("build/out.o", "junk\n");
        assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err(), "ignored write fired");

        for i in 0..5 {
            fx.write("a.txt", &format!("save {i}\n"));
            std::thread::sleep(Duration::from_millis(40));
        }
        rx.recv_timeout(Duration::from_secs(5)).expect("save did not fire");
        assert!(rx.recv_timeout(Duration::from_millis(800)).is_err(), "burst fired more than once");
    }

    #[test]
    fn transcript_filter_matches_repo_sessions_only() {
        let p = Path::new("/h/.claude/projects");
        let slugs = vec!["-w-repo".to_string()];
        assert!(is_transcript(p, &slugs, &p.join("-w-repo/abc.jsonl")));
        assert!(is_transcript(p, &slugs, &p.join("-w-repo-src/abc.jsonl")));
        assert!(is_transcript(p, &slugs, &p.join("-w-repo/abc/subagents/agent-1.jsonl")));
        assert!(!is_transcript(p, &slugs, &p.join("-w-repo/memory/MEMORY.md")));
        assert!(!is_transcript(p, &slugs, &p.join("-w-other/abc.jsonl")));
        assert!(!is_transcript(p, &slugs, Path::new("/elsewhere/-w-repo/a.jsonl")));
    }

    #[test]
    fn transcript_write_fires() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("-w-repo");
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = mpsc::channel();
        let w = RepoWatcher::new();
        w.watch_transcripts(tmp.path(), vec!["-w-repo".into()], move || {
            let _ = tx.send(());
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        while rx.try_recv().is_ok() {}

        std::fs::write(dir.join("notes.md"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err(), "non-transcript fired");
        std::fs::write(dir.join("s.jsonl"), "{}\n").unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("transcript write did not fire");
    }

    #[test]
    fn paths_outside_the_root_count_as_relevant() {
        let fx = Fixture::new();
        assert!(is_relevant(Some(&fx.repo), fx.root(), Path::new("/elsewhere/x")));
    }
}
