//! Branch history (docs/PLAN.md M13): the first-parent log of the active
//! branch, and one commit's diff. Read-only.

use super::diff::{collect_hunks, delta_paths, delta_status, diff_from};
use super::target::{self, Range};
use super::types::FileDiff;
use crate::branch::{body_without_trailers, CLAUDE_TRAILER};
use crate::error::{AppError, AppResult};
use crate::state::RepoState;
use git2::{Commit, Oid, Patch, Repository, Sort};
use serde::Serialize;
use std::collections::HashMap;

/// Most commits one page returns.
const MAX_PAGE: usize = 500;
/// Files shown for one commit; the rest are counted, not diffed.
const MAX_COMMIT_FILES: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Branch,
    Remote,
    Tag,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub sha: String,
    pub short: String,
    pub subject: String,
    /// Message body without the trailer block.
    pub body: String,
    pub author: String,
    /// ms since the epoch.
    pub time: i64,
    /// Has a `Co-Authored-By: Claude …` trailer.
    pub claude: bool,
    pub merge: bool,
    /// Branches, remote branches and tags pointing here.
    pub refs: Vec<RefLabel>,
    /// Against the first parent.
    pub files: usize,
    pub adds: usize,
    pub dels: usize,
    /// Above the merge-base: the branch's own commit, not its base's.
    pub on_branch: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchLog {
    /// The branch whose log this is; "HEAD" when detached.
    pub head: String,
    /// The base the merge-base was taken against, when there is one.
    pub base: Option<String>,
    pub merge_base: Option<String>,
    pub entries: Vec<LogEntry>,
    /// More commits past this page.
    pub more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub entry: LogEntry,
    /// First parent; None for a root commit.
    pub parent: Option<String>,
    pub files: Vec<FileDiff>,
    /// Changed files past `MAX_COMMIT_FILES`, not included.
    pub omitted: usize,
}

/// The tip whose log is shown: the branch under review, else HEAD. None on
/// an unborn branch.
fn tip(repo: &Repository, range: Option<&Range>) -> AppResult<Option<(String, Oid)>> {
    if let Some(r) = range {
        return Ok(Some((r.head.clone(), r.head_oid()?)));
    }
    let Ok(head) = repo.head() else { return Ok(None) };
    let oid = head.peel_to_commit()?.id();
    Ok(Some((target::checked_out(repo).unwrap_or_else(|| "HEAD".into()), oid)))
}

/// The review's base, else the detected one, unless it is the branch
/// itself (`main` against `main` or `origin/main`).
fn base_for(repo: &Repository, range: Option<&Range>, head: &str) -> Option<String> {
    if let Some(r) = range {
        return Some(r.base.clone());
    }
    let base = target::default_base(repo)?;
    let short = base.split_once('/').map_or(base.as_str(), |(_, b)| b);
    (base != head && short != head).then_some(base)
}

pub fn branch_log(state: &RepoState, skip: usize, limit: usize) -> AppResult<BranchLog> {
    let repo = state.open()?;
    let range = target::resolve(&repo, &state.target())?;
    let Some((head, tip)) = tip(&repo, range.as_ref())? else {
        let head = target::checked_out(&repo).unwrap_or_else(|| "HEAD".into());
        return Ok(BranchLog { head, base: None, merge_base: None, entries: vec![], more: false });
    };
    let base = base_for(&repo, range.as_ref(), &head);
    let merge_base = match &base {
        Some(b) => {
            let b = repo.revparse_single(b)?.peel_to_commit()?.id();
            repo.merge_base(tip, b).ok()
        }
        None => None,
    };

    let mut walk = repo.revwalk()?;
    walk.push(tip)?;
    walk.simplify_first_parent()?;
    walk.set_sorting(Sort::TOPOLOGICAL)?;
    let refs = ref_labels(&repo)?;
    let limit = limit.clamp(1, MAX_PAGE);
    let mut crossed = merge_base.is_none();
    let mut entries = Vec::new();
    let mut more = false;
    for (i, oid) in walk.enumerate() {
        let oid = oid?;
        if !crossed {
            let mb = merge_base.expect("checked above");
            crossed = oid == mb || repo.graph_descendant_of(mb, oid)?;
        }
        if i < skip {
            continue;
        }
        if entries.len() == limit {
            more = true;
            break;
        }
        let c = repo.find_commit(oid)?;
        entries.push(entry(&repo, &c, &refs, !crossed)?);
    }
    Ok(BranchLog { head, base, merge_base: merge_base.map(|o| o.to_string()), entries, more })
}

pub fn commit_diff(state: &RepoState, sha: &str) -> AppResult<CommitDetail> {
    let repo = state.open()?;
    let c = find(&repo, sha)?;
    let parent = c.parents().next();
    let parent_tree = parent.as_ref().map(|p| p.tree()).transpose()?;
    let diff = diff_from(&repo, parent_tree.as_ref(), Some(&c.tree()?))?;
    let mut files = Vec::new();
    let mut omitted = 0;
    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let Some(status) = delta_status(delta.status()) else { continue };
        if files.len() == MAX_COMMIT_FILES {
            omitted += 1;
            continue;
        }
        let (path, old_path) = delta_paths(&delta);
        let patch = Patch::from_diff(&diff, idx)?;
        let is_binary = patch.as_ref().is_none_or(|p| p.delta().flags().is_binary());
        let hunks = match &patch {
            Some(p) if !is_binary => collect_hunks(p, &path)?,
            _ => vec![],
        };
        files.push(FileDiff { path, old_path, status, is_binary, hunks });
    }
    let refs = ref_labels(&repo)?;
    Ok(CommitDetail {
        entry: entry(&repo, &c, &refs, false)?,
        parent: parent.map(|p| p.id().to_string()),
        files,
        omitted,
    })
}

/// A commit by full or abbreviated sha; nothing else (no refs, no ranges).
pub(crate) fn find<'r>(repo: &'r Repository, sha: &str) -> AppResult<Commit<'r>> {
    if sha.len() < 4 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Input(format!("{sha} is not a commit sha")));
    }
    let obj = repo.revparse_single(sha).map_err(|_| AppError::Input(format!("no commit {sha}")))?;
    Ok(obj.peel_to_commit()?)
}

fn entry(repo: &Repository, c: &Commit, refs: &HashMap<Oid, Vec<RefLabel>>, on_branch: bool) -> AppResult<LogEntry> {
    let parent = c.parents().next().map(|p| p.tree()).transpose()?;
    // Stats only, so skip rename detection: it's the slow part.
    let stats = repo.diff_tree_to_tree(parent.as_ref(), Some(&c.tree()?), None)?.stats()?;
    let message = String::from_utf8_lossy(c.message_bytes()).to_string();
    let sha = c.id().to_string();
    Ok(LogEntry {
        short: sha[..7].to_string(),
        sha,
        subject: c.summary().ok().flatten().unwrap_or_default().to_string(),
        body: body_without_trailers(&message),
        author: c.author().name().unwrap_or_default().to_string(),
        time: c.time().seconds() * 1000,
        claude: CLAUDE_TRAILER.is_match(&message),
        merge: c.parent_count() > 1,
        refs: refs.get(&c.id()).cloned().unwrap_or_default(),
        files: stats.files_changed(),
        adds: stats.insertions(),
        dels: stats.deletions(),
        on_branch,
    })
}

/// Every branch, remote branch and tag by the commit it points at, local
/// branches first. `origin/HEAD` is left out.
fn ref_labels(repo: &Repository) -> AppResult<HashMap<Oid, Vec<RefLabel>>> {
    let mut out: HashMap<Oid, Vec<RefLabel>> = HashMap::new();
    for r in repo.references()? {
        let r = r?;
        let kind = if r.is_branch() {
            RefKind::Branch
        } else if r.is_remote() {
            RefKind::Remote
        } else if r.is_tag() {
            RefKind::Tag
        } else {
            continue;
        };
        let Ok(name) = r.shorthand() else { continue };
        if kind == RefKind::Remote && name.ends_with("/HEAD") {
            continue;
        }
        let Ok(c) = r.peel_to_commit() else { continue };
        out.entry(c.id()).or_default().push(RefLabel { name: name.to_string(), kind });
    }
    for labels in out.values_mut() {
        labels.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;
    use crate::git::target::Target;

    fn subjects(l: &BranchLog) -> Vec<&str> {
        l.entries.iter().map(|e| e.subject.as_str()).collect()
    }

    #[test]
    fn unborn_repo_has_an_empty_log() {
        let fx = Fixture::new();
        let l = branch_log(&fx.state(), 0, 50).unwrap();
        assert!(l.entries.is_empty());
        assert!(!l.more);
    }

    #[test]
    fn newest_first_and_paged() {
        let fx = Fixture::new();
        for i in 1..=5 {
            fx.commit_msg(&[("a.txt", &format!("{i}\n"))], &format!("c{i}"));
        }
        let s = fx.state();
        let first = branch_log(&s, 0, 2).unwrap();
        assert_eq!(subjects(&first), ["c5", "c4"]);
        assert!(first.more);
        let last = branch_log(&s, 4, 2).unwrap();
        assert_eq!(subjects(&last), ["c1"]);
        assert!(!last.more);
        assert_eq!((first.entries[0].files, first.entries[0].adds, first.entries[0].dels), (1, 1, 1));
    }

    #[test]
    fn on_main_there_is_no_base() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        fx.repo.branch("main", &fx.repo.head().unwrap().peel_to_commit().unwrap(), true).ok();
        fx.checkout("main");
        let l = branch_log(&fx.state(), 0, 50).unwrap();
        assert_eq!(l.head, "main");
        assert_eq!(l.base, None);
        assert!(l.entries.iter().all(|e| !e.on_branch));
    }

    #[test]
    fn marks_the_commits_above_the_merge_base() {
        let fx = Fixture::new();
        fx.commit_msg(&[("a.txt", "a\n")], "base 1");
        fx.repo.branch("main", &fx.repo.head().unwrap().peel_to_commit().unwrap(), true).ok();
        fx.checkout("main");
        fx.commit_msg(&[("a.txt", "b\n")], "base 2");
        fx.branch("feat");
        fx.checkout("feat");
        fx.commit_msg(&[("b.txt", "1\n")], "feat 1");
        fx.commit_msg(&[("b.txt", "2\n")], "feat 2\n\nCo-Authored-By: Claude <noreply@anthropic.com>");

        let l = branch_log(&fx.state(), 0, 50).unwrap();
        assert_eq!(l.head, "feat");
        assert_eq!(l.base.as_deref(), Some("main"));
        assert_eq!(subjects(&l), ["feat 2", "feat 1", "base 2", "base 1"]);
        let on: Vec<bool> = l.entries.iter().map(|e| e.on_branch).collect();
        assert_eq!(on, [true, true, false, false]);
        assert_eq!(l.merge_base.as_deref(), Some(l.entries[2].sha.as_str()));
        assert!(l.entries[0].claude && !l.entries[1].claude);
        assert_eq!(l.entries[0].refs, [RefLabel { name: "feat".into(), kind: RefKind::Branch }]);
        assert!(l.entries[2].refs.iter().any(|r| r.name == "main"));
    }

    #[test]
    fn follows_the_reviewed_branch_not_head() {
        let fx = Fixture::new();
        fx.commit_msg(&[("a.txt", "a\n")], "base");
        fx.repo.branch("main", &fx.repo.head().unwrap().peel_to_commit().unwrap(), true).ok();
        fx.checkout("main");
        fx.branch("feat");
        fx.checkout("feat");
        fx.commit_msg(&[("b.txt", "1\n")], "on feat");
        fx.checkout("main");
        let s = fx.state();
        s.set_target(Target::Branch { head: "feat".into(), base: None }).unwrap();
        let l = branch_log(&s, 0, 50).unwrap();
        assert_eq!(subjects(&l), ["on feat", "base"]);
        assert_eq!(l.head, "feat");
    }

    #[test]
    fn merges_follow_the_first_parent() {
        let fx = Fixture::new();
        fx.commit_msg(&[("a.txt", "a\n")], "root");
        let root = fx.repo.head().unwrap().peel_to_commit().unwrap();
        fx.commit_msg(&[("a.txt", "b\n")], "mainline");
        let main_tip = fx.repo.head().unwrap().peel_to_commit().unwrap();
        // A side commit off root, merged into the mainline.
        let sig = git2::Signature::now("T", "t@example.invalid").unwrap();
        let mut tb = fx.repo.treebuilder(Some(&root.tree().unwrap())).unwrap();
        let blob = fx.repo.blob(b"side\n").unwrap();
        tb.insert("side.txt", blob, 0o100644).unwrap();
        let side_tree = fx.repo.find_tree(tb.write().unwrap()).unwrap();
        let side = fx.repo.commit(None, &sig, &sig, "side", &side_tree, &[&root]).unwrap();
        let side = fx.repo.find_commit(side).unwrap();
        fx.repo.commit(Some("HEAD"), &sig, &sig, "merge side", &main_tip.tree().unwrap(), &[&main_tip, &side]).unwrap();

        let l = branch_log(&fx.state(), 0, 50).unwrap();
        assert_eq!(subjects(&l), ["merge side", "mainline", "root"]);
        assert!(l.entries[0].merge);
    }

    #[test]
    fn commit_diff_lists_every_file() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n"), ("gone.txt", "x\n")]);
        std::fs::remove_file(fx.root().join("gone.txt")).unwrap();
        let mut index = fx.repo.index().unwrap();
        index.remove_path(std::path::Path::new("gone.txt")).unwrap();
        index.write().unwrap();
        fx.commit_msg(&[("a.txt", "b\n"), ("new.txt", "n\n")], "second");
        let sha = fx.repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

        let d = commit_diff(&fx.state(), &sha[..8]).unwrap();
        assert_eq!(d.entry.subject, "second");
        assert!(d.parent.is_some());
        let paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "gone.txt", "new.txt"]);
        assert!(d.files.iter().all(|f| f.hunks.len() == 1));
    }

    #[test]
    fn commit_diff_takes_only_shas() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        assert!(commit_diff(&fx.state(), "HEAD").is_err());
        assert!(commit_diff(&fx.state(), "deadbeef").is_err());
    }
}
