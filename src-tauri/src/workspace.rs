//! Workspace root (docs/PLAN.md M14): a folder of repos opened with ⌘O, and
//! the picker `P` switches between them with. Only reads: the scan lists
//! folders, the peek reads git status, and nothing is written anywhere.

use crate::error::{AppError, AppResult};
use crate::transcript::{dir_matches, repo_roots, slug};
use git2::{Repository, StatusOptions};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;

/// Levels below the root the scan walks.
const MAX_DEPTH: usize = 3;
/// Repos listed at most; past that the list says it was cut short.
const MAX_REPOS: usize = 500;
/// Folders never worth walking into.
const SKIP: &[&str] = &["node_modules", "target", "vendor", "dist", "build"];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepo {
    pub name: String,
    pub path: String,
    /// Relative to the root, `/`-separated.
    pub rel: String,
    /// A linked worktree (a `.git` file, not a dir).
    pub worktree: bool,
    /// Newest transcript write for this repo, ms since epoch.
    pub last_session: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceScan {
    pub root: String,
    pub name: String,
    pub repos: Vec<WorkspaceRepo>,
    /// More than MAX_REPOS were found.
    pub truncated: bool,
}

/// What the picker shows per repo once it has looked inside.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoPeek {
    pub path: String,
    /// Branch name, "HEAD" when detached; None when HEAD can't be read.
    pub branch: Option<String>,
    /// Changed files (untracked included), None when status failed.
    pub changed: Option<usize>,
    /// HEAD commit time, ms since epoch; None on an unborn branch.
    pub last_commit: Option<i64>,
}

/// What ⌘O picked.
#[derive(Debug)]
pub enum Picked {
    Repo(PathBuf),
    Root(WorkspaceScan),
}

/// Sort a folder picked in the ⌘O dialog (docs/PLAN.md M14): a repo's
/// workdir root opens it; a folder holding repos becomes the root; a
/// subfolder of a repo opens that repo.
pub fn classify(path: &Path) -> AppResult<Picked> {
    let workdir = Repository::discover(path).ok().and_then(|r| r.workdir().map(Path::to_path_buf));
    if let Some(w) = &workdir {
        if same_dir(w, path) {
            return Ok(Picked::Repo(w.clone()));
        }
    }
    let scan = scan(path);
    if !scan.repos.is_empty() {
        return Ok(Picked::Root(scan));
    }
    workdir
        .map(Picked::Repo)
        .ok_or_else(|| AppError::Input(format!("no git repositories under {}", path.display())))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Walk `root` for repos. Symlinks aren't followed, hidden and build
/// folders are skipped, and a repo isn't walked into, so submodules and
/// nested repos don't show up.
pub fn scan(root: &Path) -> WorkspaceScan {
    let mut repos = Vec::new();
    let mut truncated = false;
    walk(root, root, 1, &mut repos, &mut truncated);
    repos.sort_by(|a, b| a.rel.cmp(&b.rel));
    WorkspaceScan {
        root: root.to_string_lossy().to_string(),
        name: root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.to_string_lossy().to_string()),
        repos,
        truncated,
    }
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<WorkspaceRepo>, truncated: &mut bool) {
    if depth > MAX_DEPTH || *truncated {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        // DirEntry::file_type doesn't follow symlinks.
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && !SKIP.contains(&name.as_ref())
        })
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for d in dirs {
        let git = d.join(".git");
        let kind = std::fs::symlink_metadata(&git).ok().map(|m| m.is_file());
        if let Some(worktree) = kind {
            if out.len() == MAX_REPOS {
                *truncated = true;
                return;
            }
            out.push(WorkspaceRepo {
                name: d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                rel: d.strip_prefix(root).unwrap_or(&d).to_string_lossy().replace('\\', "/"),
                path: d.to_string_lossy().to_string(),
                worktree,
                last_session: None,
            });
        } else if !looks_bare(&d) {
            walk(root, &d, depth + 1, out, truncated);
        }
    }
}

/// A bare repo: skipped, and not walked into.
fn looks_bare(d: &Path) -> bool {
    d.join("HEAD").is_file() && d.join("objects").is_dir() && d.join("refs").is_dir()
}

/// Newest transcript write per repo path, from file mtimes only (no
/// transcript is read). A project dir goes to the repo with the longest
/// matching slug, so `repo-live` isn't counted for `repo` when both are
/// listed.
pub fn session_times(projects: &Path, repos: &[WorkspaceRepo]) -> HashMap<String, i64> {
    let slugs: Vec<(usize, Vec<String>)> = repos
        .iter()
        .enumerate()
        .map(|(i, r)| (i, repo_roots(Path::new(&r.path)).iter().map(|p| slug(p)).collect()))
        .collect();
    let mut out: HashMap<String, i64> = HashMap::new();
    let Ok(dirs) = std::fs::read_dir(projects) else { return out };
    for dir in dirs.flatten() {
        let name = dir.file_name().to_string_lossy().to_string();
        let owner = slugs
            .iter()
            .filter(|(_, s)| dir_matches(&name, s))
            .max_by_key(|(_, s)| s.iter().filter(|s| name.starts_with(s.as_str())).map(String::len).max().unwrap_or(0))
            .map(|(i, _)| *i);
        let Some(i) = owner else { continue };
        let Some(newest) = newest_jsonl(&dir.path()) else { continue };
        let slot = out.entry(repos[i].path.clone()).or_insert(newest);
        *slot = (*slot).max(newest);
    }
    out
}

fn newest_jsonl(dir: &Path) -> Option<i64> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .filter_map(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .max()
}

/// Branch, changed-file count and HEAD time of one repo. Untracked folders
/// count once and aren't walked, so a big repo doesn't stall the picker.
pub fn peek(path: &Path) -> RepoPeek {
    let mut out = RepoPeek { path: path.to_string_lossy().to_string(), branch: None, changed: None, last_commit: None };
    let Ok(repo) = Repository::open(path) else { return out };
    out.branch = match repo.head() {
        Ok(h) if h.is_branch() => h.shorthand().ok().map(String::from),
        Ok(_) => Some("HEAD".into()),
        // Unborn: HEAD still names the branch it will create.
        Err(_) => repo
            .find_reference("HEAD")
            .ok()
            .and_then(|r| r.symbolic_target().ok().flatten().map(|t| t.trim_start_matches("refs/heads/").to_string())),
    };
    out.last_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).map(|c| c.time().seconds() * 1000);
    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(false).include_ignored(false).exclude_submodules(true);
    out.changed = repo.statuses(Some(&mut opts)).ok().map(|s| s.len());
    out
}

/// `peek` over several repos on a few threads, in input order.
pub fn peek_all(paths: &[PathBuf]) -> Vec<RepoPeek> {
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8).min(paths.len().max(1));
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<RepoPeek>>> = paths.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(p) = paths.get(i) else { break };
                *slots[i].lock() = Some(peek(p));
            });
        }
    });
    slots.into_iter().zip(paths).map(|(s, p)| s.into_inner().unwrap_or_else(|| peek(p))).collect()
}

/// The last scan, so `P` opens instantly and `workspace_status` can refuse
/// paths it didn't list.
pub struct Workspace {
    last: Mutex<Option<WorkspaceScan>>,
}

impl Workspace {
    pub fn new() -> Self {
        Self { last: Mutex::new(None) }
    }

    pub fn set(&self, scan: WorkspaceScan) {
        *self.last.lock() = Some(scan);
    }

    pub fn clear(&self) {
        *self.last.lock() = None;
    }

    /// The cached scan of `root`, walking it again when `fresh` or when
    /// the cache is for another root.
    pub fn scan(&self, root: &Path, fresh: bool) -> WorkspaceScan {
        let mut g = self.last.lock();
        let root_s = root.to_string_lossy();
        match g.as_ref() {
            Some(s) if !fresh && s.root == root_s => s.clone(),
            _ => {
                let s = scan(root);
                *g = Some(s.clone());
                s
            }
        }
    }

    /// Paths from `wanted` the last scan listed; anything else is refused.
    pub fn listed(&self, wanted: &[String]) -> AppResult<Vec<PathBuf>> {
        let g = self.last.lock();
        let scan = g.as_ref().ok_or_else(|| AppError::Input("no workspace root".into()))?;
        wanted
            .iter()
            .map(|w| {
                scan.repos
                    .iter()
                    .find(|r| &r.path == w)
                    .map(|r| PathBuf::from(&r.path))
                    .ok_or_else(|| AppError::Input(format!("{w} is not in the workspace")))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn init(path: &Path) -> Repository {
        fs::create_dir_all(path).unwrap();
        Repository::init(path).unwrap()
    }

    fn commit(repo: &Repository, file: &str, body: &str) {
        let workdir = repo.workdir().unwrap();
        fs::write(workdir.join(file), body).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new(file)).unwrap();
        idx.write().unwrap();
        let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@t").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "c", &tree, &parents).unwrap();
    }

    fn rels(s: &WorkspaceScan) -> Vec<&str> {
        s.repos.iter().map(|r| r.rel.as_str()).collect()
    }

    #[test]
    fn classify_repo_root_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("a"));
        match classify(&dir.path().join("a")).unwrap() {
            Picked::Repo(p) => assert!(same_dir(&p, &dir.path().join("a"))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn classify_folder_of_repos_is_a_root() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("a"));
        init(&dir.path().join("group/b"));
        match classify(dir.path()).unwrap() {
            Picked::Root(s) => assert_eq!(rels(&s), ["a", "group/b"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn classify_subfolder_of_a_repo_opens_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("a"));
        fs::create_dir_all(dir.path().join("a/src/deep")).unwrap();
        match classify(&dir.path().join("a/src")).unwrap() {
            Picked::Repo(p) => assert!(same_dir(&p, &dir.path().join("a"))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn classify_empty_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("plain/folder")).unwrap();
        let err = classify(&dir.path().join("plain")).unwrap_err().to_string();
        assert!(err.contains("no git repositories"), "{err}");
    }

    #[test]
    fn scan_stops_at_depth_three() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("x/y/three"));
        init(&dir.path().join("x/y/z/four"));
        assert_eq!(rels(&scan(dir.path())), ["x/y/three"]);
    }

    #[test]
    fn scan_skips_hidden_and_build_folders() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join(".hidden/r"));
        init(&dir.path().join("node_modules/r"));
        init(&dir.path().join("target/r"));
        init(&dir.path().join("vendor/r"));
        init(&dir.path().join("ok"));
        assert_eq!(rels(&scan(dir.path())), ["ok"]);
    }

    #[test]
    fn scan_does_not_descend_into_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("outer"));
        init(&dir.path().join("outer/inner"));
        assert_eq!(rels(&scan(dir.path())), ["outer"]);
    }

    #[test]
    fn scan_tags_a_linked_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let main = init(&dir.path().join("main"));
        commit(&main, "a.txt", "a\n");
        main.worktree("wt", &dir.path().join("wt"), None).unwrap();
        let s = scan(dir.path());
        assert_eq!(rels(&s), ["main", "wt"]);
        assert_eq!(s.repos.iter().map(|r| r.worktree).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn scan_skips_bare_repos() {
        let dir = tempfile::tempdir().unwrap();
        Repository::init_bare(dir.path().join("bare.git")).unwrap();
        init(&dir.path().join("ok"));
        assert_eq!(rels(&scan(dir.path())), ["ok"]);
    }

    #[cfg(unix)]
    #[test]
    fn scan_ignores_symlinks_so_loops_end() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("ok"));
        fs::create_dir_all(dir.path().join("a")).unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("a/loop")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("ok"), dir.path().join("alias")).unwrap();
        assert_eq!(rels(&scan(dir.path())), ["ok"]);
    }

    #[test]
    fn scan_caps_the_list() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..=MAX_REPOS {
            fs::create_dir_all(dir.path().join(format!("r{i:04}/.git"))).unwrap();
        }
        let s = scan(dir.path());
        assert_eq!(s.repos.len(), MAX_REPOS);
        assert!(s.truncated);
    }

    #[test]
    fn peek_dirty_clean_detached_and_unborn() {
        let dir = tempfile::tempdir().unwrap();
        let dirty = init(&dir.path().join("dirty"));
        commit(&dirty, "a.txt", "a\n");
        fs::write(dir.path().join("dirty/a.txt"), "b\n").unwrap();
        fs::create_dir_all(dir.path().join("dirty/newdir/sub")).unwrap();
        fs::write(dir.path().join("dirty/newdir/sub/x"), "x").unwrap();
        fs::write(dir.path().join("dirty/newdir/y"), "y").unwrap();

        let clean = init(&dir.path().join("clean"));
        commit(&clean, "a.txt", "a\n");

        let detached = init(&dir.path().join("detached"));
        commit(&detached, "a.txt", "a\n");
        let oid = detached.head().unwrap().target().unwrap();
        detached.set_head_detached(oid).unwrap();

        init(&dir.path().join("unborn"));

        let p = peek_all(&["dirty", "clean", "detached", "unborn"].map(|n| dir.path().join(n)));
        // a.txt modified, newdir/ untracked counted once.
        assert_eq!(p[0].changed, Some(2));
        assert_eq!(p[1].changed, Some(0));
        let branch = clean.head().unwrap().shorthand().unwrap().to_string();
        assert_eq!(p[1].branch.as_deref(), Some(branch.as_str()));
        assert!(p[1].last_commit.is_some());
        assert_eq!(p[2].branch.as_deref(), Some("HEAD"));
        assert_eq!(p[3].branch.as_deref(), Some(branch.as_str()));
        assert_eq!((p[3].changed, p[3].last_commit), (Some(0), None));
    }

    #[test]
    fn listed_refuses_paths_outside_the_last_scan() {
        let dir = tempfile::tempdir().unwrap();
        init(&dir.path().join("a"));
        let ws = Workspace::new();
        assert!(ws.listed(&["/x".into()]).is_err(), "no scan yet");
        let s = ws.scan(dir.path(), false);
        let a = s.repos[0].path.clone();
        assert_eq!(ws.listed(std::slice::from_ref(&a)).unwrap(), [PathBuf::from(&a)]);
        let err = ws.listed(&[a, "/etc".into()]).unwrap_err().to_string();
        assert!(err.contains("not in the workspace"), "{err}");
    }

    #[test]
    fn session_times_go_to_the_longest_slug() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("ws");
        init(&root.join("repo"));
        init(&root.join("repo-live"));
        let s = scan(&root);
        let projects = dir.path().join("projects");
        let live_slug = slug(&PathBuf::from(&s.repos[1].path));
        fs::create_dir_all(projects.join(&live_slug)).unwrap();
        fs::write(projects.join(&live_slug).join("s.jsonl"), "{}").unwrap();
        fs::write(projects.join(&live_slug).join("notes.txt"), "").unwrap();
        let t = session_times(&projects, &s.repos);
        assert!(t.contains_key(&s.repos[1].path));
        assert!(!t.contains_key(&s.repos[0].path));
    }
}
