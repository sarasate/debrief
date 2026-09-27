use super::diff::{delta_paths, delta_status, worktree_diff};
use super::types::*;
use crate::error::{AppError, AppResult};
use crate::state::RepoState;
use git2::{Oid, Patch, Repository};
use std::collections::HashMap;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub fn repo_info(workdir: &Path) -> RepoInfo {
    let name = workdir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    RepoInfo { workdir: workdir.to_string_lossy().to_string(), name }
}

/// Added lines scanned per file, and chars kept per line. Enough for the
/// flag rules; minified or huge files are "large change" anyway.
const SCAN_MAX_LINES: usize = 5_000;
const SCAN_MAX_CHARS: usize = 2_000;

/// What the flag rules need from a file's diff. Internal: never serialized,
/// because added lines may hold secrets.
#[derive(Debug, Clone, Default)]
pub struct ContentScan {
    /// (new line number, text) of added lines, capped.
    pub added: Vec<(u32, String)>,
    /// The HEAD blob, for the test-shrink rule.
    pub old_blob: Option<Oid>,
}

pub fn repo_status(state: &RepoState) -> AppResult<RepoStatus> {
    Ok(scan(&state.open()?, false)?.0)
}

/// Status plus, when `content` is set, a ContentScan per changed path.
pub fn repo_status_scanned(repo: &Repository) -> AppResult<(RepoStatus, HashMap<String, ContentScan>)> {
    scan(repo, true)
}

fn scan(repo: &Repository, content: bool) -> AppResult<(RepoStatus, HashMap<String, ContentScan>)> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repo not supported".into()))?
        .to_path_buf();

    let diff = worktree_diff(repo)?;
    let mut files = Vec::with_capacity(diff.deltas().len());
    let mut scans = HashMap::new();
    let (mut adds, mut dels) = (0, 0);
    let mut last_write: Option<i64> = None;

    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let Some(status) = delta_status(delta.status()) else { continue };
        let (path, old_path) = delta_paths(&delta);

        let patch = Patch::from_diff(&diff, idx)?;
        let (is_binary, hunks, a, d) = match &patch {
            Some(p) => {
                let (_, a, d) = p.line_stats()?;
                (p.delta().flags().is_binary(), p.num_hunks(), a, d)
            }
            None => (true, 0, 0, 0),
        };
        if content {
            let old = delta.old_file().id();
            let mut sc = ContentScan { added: vec![], old_blob: (!old.is_zero()).then_some(old) };
            if let (Some(p), false) = (&patch, is_binary) {
                collect_added(p, &mut sc.added)?;
            }
            scans.insert(path.clone(), sc);
        }
        adds += a;
        dels += d;

        if let Some(ms) = mtime_ms(&workdir.join(&path)) {
            last_write = Some(last_write.map_or(ms, |cur| cur.max(ms)));
        }

        files.push(ChangedFile { path, old_path, status, is_binary, adds: a, dels: d, hunks });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let status = RepoStatus {
        repo: repo_info(&workdir),
        head: head_info(repo),
        files,
        adds,
        dels,
        last_write,
    };
    Ok((status, scans))
}

fn collect_added(p: &Patch, out: &mut Vec<(u32, String)>) -> AppResult<()> {
    for h in 0..p.num_hunks() {
        for l in 0..p.num_lines_in_hunk(h)? {
            if out.len() >= SCAN_MAX_LINES {
                return Ok(());
            }
            let line = p.line_in_hunk(h, l)?;
            if line.origin() != '+' {
                continue;
            }
            let text = String::from_utf8_lossy(line.content());
            let text = text.trim_end_matches(['\n', '\r']);
            let text: String = text.chars().take(SCAN_MAX_CHARS).collect();
            out.push((line.new_lineno().unwrap_or(0), text));
        }
    }
    Ok(())
}

fn head_info(repo: &Repository) -> Option<HeadInfo> {
    let r = repo.head().ok()?;
    let branch = if r.is_branch() { r.shorthand().ok().map(String::from) } else { None };
    Some(HeadInfo {
        branch,
        sha: r.target().map(|o| o.to_string()).unwrap_or_default(),
        detached: repo.head_detached().unwrap_or(false),
    })
}

fn mtime_ms(p: &Path) -> Option<i64> {
    let modified = std::fs::metadata(p).ok()?.modified().ok()?;
    let ms = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(ms).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;

    fn find<'a>(s: &'a RepoStatus, path: &str) -> &'a ChangedFile {
        s.files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} missing"))
    }

    #[test]
    fn clean_repo_has_no_files() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        let s = repo_status(&fx.state()).unwrap();
        assert!(s.files.is_empty());
        assert_eq!((s.adds, s.dels), (0, 0));
        assert!(s.last_write.is_none());
        assert!(s.head.is_some());
    }

    #[test]
    fn reports_each_kind_of_change_sorted_by_path() {
        let fx = Fixture::new();
        fx.commit(&[("keep.txt", "1\n2\n3\n"), ("gone.txt", "bye\n")]);
        fx.write("keep.txt", "1\n2\n3\n4\n");
        fx.remove("gone.txt");
        fx.write("staged.txt", "s\n");
        fx.stage("staged.txt");
        fx.write("untracked.txt", "u\nv\n");

        let s = repo_status(&fx.state()).unwrap();
        let paths: Vec<_> = s.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["gone.txt", "keep.txt", "staged.txt", "untracked.txt"]);
        assert_eq!(find(&s, "keep.txt").status, FileStatus::Modified);
        assert_eq!(find(&s, "gone.txt").status, FileStatus::Deleted);
        assert_eq!(find(&s, "staged.txt").status, FileStatus::Added);
        assert_eq!(find(&s, "untracked.txt").status, FileStatus::Untracked);

        assert_eq!((find(&s, "keep.txt").adds, find(&s, "keep.txt").dels), (1, 0));
        assert_eq!(find(&s, "gone.txt").dels, 1);
        assert_eq!(find(&s, "untracked.txt").adds, 2);
        assert_eq!((s.adds, s.dels), (4, 1));
        assert!(s.last_write.is_some());
    }

    #[test]
    fn ignored_files_are_excluded() {
        let fx = Fixture::new();
        fx.commit(&[(".gitignore", "target/\n")]);
        fx.write("target/out.o", "junk\n");
        assert!(repo_status(&fx.state()).unwrap().files.is_empty());
    }

    #[test]
    fn detects_staged_rename() {
        let fx = Fixture::new();
        let body = "fn main() {\n    println!(\"hello\");\n}\n".repeat(4);
        fx.commit(&[("old.rs", &body)]);
        std::fs::rename(fx.root().join("old.rs"), fx.root().join("new.rs")).unwrap();
        let mut index = fx.repo.index().unwrap();
        index.remove_path(Path::new("old.rs")).unwrap();
        index.add_path(Path::new("new.rs")).unwrap();
        index.write().unwrap();

        let s = repo_status(&fx.state()).unwrap();
        assert_eq!(s.files.len(), 1, "{:?}", s.files);
        assert_eq!(s.files[0].status, FileStatus::Renamed);
        assert_eq!(s.files[0].path, "new.rs");
        assert_eq!(s.files[0].old_path.as_deref(), Some("old.rs"));
    }

    #[test]
    fn unborn_branch_treats_everything_as_new() {
        let fx = Fixture::new();
        fx.write("a.txt", "a\n");
        let s = repo_status(&fx.state()).unwrap();
        assert!(s.head.is_none());
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.files[0].status, FileStatus::Untracked);
    }
}
