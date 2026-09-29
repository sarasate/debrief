//! What is being reviewed (docs/PLAN.md "Branch & PR review"): the working
//! tree, or a branch from where it left its base to its tip. A checked-out
//! branch also carries the uncommitted changes on top.

use crate::error::{AppError, AppResult};
use git2::{BranchType, Oid, Repository};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// Bases tried when the repo has no `origin/HEAD`.
const TRUNKS: &[&str] = &["main", "master", "trunk", "develop"];
/// Branches listed in the picker, newest first.
const MAX_BRANCHES: usize = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    #[default]
    Worktree,
    /// A local branch by name. `base` None means "detect".
    Branch { head: String, base: Option<String> },
}

/// A branch target resolved to commits, re-done on every read so new
/// commits and moved bases show up.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub head: String,
    pub base: String,
    pub head_sha: String,
    pub merge_base_sha: String,
    /// Commits on `head` since it left `base`.
    pub ahead: usize,
    /// Commits on `base` since then; they are not part of the review.
    pub behind: usize,
    /// `head` is checked out, so the review runs on to the working tree.
    pub includes_worktree: bool,
    /// Tip commit time, ms since the epoch.
    pub head_time: i64,
}

impl Range {
    pub fn head_oid(&self) -> AppResult<Oid> {
        Ok(Oid::from_str(&self.head_sha)?)
    }

    pub fn merge_base_oid(&self) -> AppResult<Oid> {
        Ok(Oid::from_str(&self.merge_base_sha)?)
    }

    /// Review-state key: one file per (branch, base), independent of commits.
    pub fn state_key(&self) -> String {
        let mut h = Sha1::new();
        h.update(self.head.as_bytes());
        h.update(b"\0");
        h.update(self.base.as_bytes());
        let hex: String = h.finalize().iter().take(6).map(|b| format!("{b:02x}")).collect();
        format!("branch-{hex}")
    }
}

/// `origin/HEAD`'s branch (`origin/main`), else the first trunk that exists.
pub fn default_base(repo: &Repository) -> Option<String> {
    if let Ok(r) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Some(name) = r.symbolic_target().ok().flatten().and_then(|t| t.strip_prefix("refs/remotes/")) {
            if repo.revparse_single(name).is_ok() {
                return Some(name.to_string());
            }
        }
    }
    TRUNKS.iter().find(|t| repo.find_branch(t, BranchType::Local).is_ok()).map(|t| t.to_string())
}

fn commit_of<'r>(repo: &'r Repository, rev: &str) -> AppResult<git2::Commit<'r>> {
    repo.revparse_single(rev)
        .and_then(|o| o.peel_to_commit())
        .map_err(|_| AppError::Input(format!("{rev} is not a branch or commit here")))
}

/// Name of the checked-out local branch, if any.
pub fn checked_out(repo: &Repository) -> Option<String> {
    let h = repo.head().ok()?;
    h.is_branch().then(|| h.shorthand().ok().map(String::from)).flatten()
}

/// Resolve a target; None for the working tree.
pub fn resolve(repo: &Repository, target: &Target) -> AppResult<Option<Range>> {
    let Target::Branch { head, base } = target else { return Ok(None) };
    let base = match base {
        Some(b) => b.clone(),
        None => default_base(repo).ok_or_else(|| AppError::Input("no base branch found: pick one".into()))?,
    };
    let head_c = commit_of(repo, head)?;
    let base_c = commit_of(repo, &base)?;
    let merge_base = repo
        .merge_base(head_c.id(), base_c.id())
        .map_err(|_| AppError::Input(format!("{head} and {base} share no history")))?;
    let (ahead, behind) = repo.graph_ahead_behind(head_c.id(), base_c.id())?;
    Ok(Some(Range {
        head: head.clone(),
        base,
        head_sha: head_c.id().to_string(),
        merge_base_sha: merge_base.to_string(),
        ahead,
        behind,
        includes_worktree: checked_out(repo).as_deref() == Some(head.as_str()),
        head_time: head_c.time().seconds() * 1000,
    }))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOption {
    pub name: String,
    pub ahead: usize,
    pub behind: usize,
    pub checked_out: bool,
    /// Tip commit time, ms since the epoch.
    pub updated_at: i64,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetList {
    pub base: Option<String>,
    pub branches: Vec<BranchOption>,
}

/// Local branches other than the base, newest first, with how far each is
/// ahead of it.
pub fn list(repo: &Repository) -> AppResult<TargetList> {
    let base = default_base(repo);
    let base_oid = base.as_deref().and_then(|b| commit_of(repo, b).ok()).map(|c| c.id());
    let current = checked_out(repo);
    let mut branches = Vec::new();
    for b in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = b?;
        let Some(name) = branch.name()?.map(String::from) else { continue };
        // The base itself, or its local twin when the base is `origin/<name>`.
        if base.as_deref().is_some_and(|b| b == name || b.strip_prefix("origin/") == Some(name.as_str())) {
            continue;
        }
        let Ok(c) = branch.get().peel_to_commit() else { continue };
        let (ahead, behind) = match base_oid {
            Some(bo) => repo.graph_ahead_behind(c.id(), bo).unwrap_or((0, 0)),
            None => (0, 0),
        };
        branches.push(BranchOption {
            checked_out: current.as_deref() == Some(name.as_str()),
            name,
            ahead,
            behind,
            updated_at: c.time().seconds() * 1000,
            subject: c.summary().ok().flatten().unwrap_or_default().to_string(),
        });
    }
    branches.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.name.cmp(&b.name)));
    branches.truncate(MAX_BRANCHES);
    Ok(TargetList { base, branches })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;
    use crate::review::review_model;
    use crate::review_state::{ReviewStore, Verdict, NO_SESSION};
    use crate::transcript::sessions::TranscriptCache;

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    /// base: a.txt, b.txt, d.txt. `feat` makes 3 commits (edit a, add c,
    /// edit a again); afterwards the base moves on by editing b.
    fn branch_repo() -> (Fixture, String) {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", &numbered(40)), ("b.txt", "b\n"), ("d.txt", &numbered(20))]);
        let base = checked_out(&fx.repo).unwrap();
        fx.branch("feat");
        fx.checkout("feat");
        fx.commit(&[("a.txt", &numbered(40).replace("line 3\n", "top\n"))]);
        fx.commit(&[("c.txt", "new\n")]);
        fx.commit(&[("a.txt", &numbered(40).replace("line 3\n", "top\n").replace("line 35\n", "bottom\n"))]);
        fx.checkout(&base);
        fx.commit(&[("b.txt", "b on base\n")]);
        (fx, base)
    }

    fn feat(base: &str) -> Target {
        Target::Branch { head: "feat".into(), base: Some(base.into()) }
    }

    #[test]
    fn only_the_branch_changes_show() {
        let (fx, base) = branch_repo();
        let state = fx.state();
        state.set_target(feat(&base)).unwrap();
        let r = resolve(&fx.repo, &state.target()).unwrap().unwrap();
        assert_eq!((r.ahead, r.behind, r.includes_worktree), (3, 1, false));

        let s = crate::git::status::repo_status(&state).unwrap();
        let paths: Vec<_> = s.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "c.txt"], "b.txt moved on the base, not the branch");
        let d = crate::git::diff::diff_file(&state, "a.txt").unwrap();
        assert_eq!(d.hunks.len(), 2, "both a.txt edits, net of the three commits");
        assert!(crate::git::diff::diff_file(&state, "b.txt").is_err());
    }

    #[test]
    fn rebase_keeps_hunk_ids_and_viewed_marks() {
        let (fx, base) = branch_repo();
        let (state, cache, store) = (fx.state(), TranscriptCache::new(), ReviewStore::new());
        state.set_target(feat(&base)).unwrap();
        let m = review_model(&state, &cache, &store, None, None).unwrap();
        assert!(m.state_key.starts_with("branch-"));
        let a = m.files["a.txt"].clone();
        store.update(fx.repo.path(), &m.state_key, |s| {
            s.viewed.insert("a.txt".into(), a.oid.clone());
        }).unwrap();

        // Replay the branch on the moved base and force-move `feat` there.
        fx.branch("feat-rebased");
        fx.checkout("feat-rebased");
        fx.commit(&[("a.txt", &numbered(40).replace("line 3\n", "top\n").replace("line 35\n", "bottom\n")), ("c.txt", "new\n")]);
        fx.checkout(&base);
        fx.move_branch("feat", "feat-rebased");

        let m2 = review_model(&state, &cache, &store, None, None).unwrap();
        assert_eq!(m2.range.as_ref().unwrap().behind, 0, "branch now sits on the new base");
        assert_eq!(m2.files["a.txt"].hunk_ids, a.hunk_ids);
        assert!(m2.files["a.txt"].viewed);
        assert!(m2.invalidated.is_empty());
    }

    #[test]
    fn a_new_commit_reopens_a_cleared_file() {
        let (fx, base) = branch_repo();
        let (state, cache, store) = (fx.state(), TranscriptCache::new(), ReviewStore::new());
        state.set_target(feat(&base)).unwrap();
        let m = review_model(&state, &cache, &store, None, None).unwrap();
        store.update(fx.repo.path(), &m.state_key, |s| {
            s.viewed.insert("a.txt".into(), m.files["a.txt"].oid.clone());
            s.viewed.insert("c.txt".into(), m.files["c.txt"].oid.clone());
        }).unwrap();

        fx.checkout("feat");
        fx.commit(&[("c.txt", "new\nmore\n")]);
        fx.checkout(&base);
        let m2 = review_model(&state, &cache, &store, None, None).unwrap();
        assert_eq!(m2.invalidated, ["c.txt"]);
        assert!(m2.files["a.txt"].viewed && !m2.files["c.txt"].viewed);
    }

    #[test]
    fn branch_and_worktree_keep_separate_progress() {
        let (fx, base) = branch_repo();
        fx.write("b.txt", "dirty on base\n");
        let (state, cache, store) = (fx.state(), TranscriptCache::new(), ReviewStore::new());
        let w = review_model(&state, &cache, &store, None, None).unwrap();
        assert_eq!(w.state_key, NO_SESSION);
        store.update(fx.repo.path(), &w.state_key, |s| {
            s.viewed.insert("b.txt".into(), w.files["b.txt"].oid.clone());
        }).unwrap();

        state.set_target(feat(&base)).unwrap();
        let b = review_model(&state, &cache, &store, None, None).unwrap();
        assert_ne!(b.state_key, w.state_key);
        assert!(b.files.values().all(|f| !f.viewed));

        state.set_target(Target::Worktree).unwrap();
        assert!(review_model(&state, &cache, &store, None, None).unwrap().files["b.txt"].viewed);
    }

    #[test]
    fn checked_out_branch_runs_on_to_the_worktree_and_writes_only_uncommitted_hunks() {
        let (fx, base) = branch_repo();
        fx.checkout("feat");
        // An uncommitted edit in a file the branch never touched.
        let d_dirty = numbered(20).replace("line 10\n", "ten\n");
        fx.write("d.txt", &d_dirty);
        let (state, cache, store) = (fx.state(), TranscriptCache::new(), ReviewStore::new());
        state.set_target(feat(&base)).unwrap();
        let m = review_model(&state, &cache, &store, None, None).unwrap();
        let r = m.range.clone().unwrap();
        assert!(r.includes_worktree);
        let paths: Vec<_> = m.status.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "c.txt", "d.txt"]);
        assert_eq!(m.files["d.txt"].uncommitted.as_deref(), Some(m.files["d.txt"].hunk_ids.as_slice()));
        assert_eq!(m.files["a.txt"].uncommitted.as_deref(), Some(&[][..]), "a.txt is committed only");

        let d_hunk = m.files["d.txt"].hunk_ids[0].clone();
        let a_hunk = m.files["a.txt"].hunk_ids[0].clone();
        store.update(fx.repo.path(), &m.state_key, |s| {
            s.verdicts.insert(d_hunk.clone(), Verdict::Revert);
            s.verdicts.insert(a_hunk.clone(), Verdict::Revert);
        }).unwrap();
        let res = crate::actions::discard_reverted(&fx.repo, fx.root(), &store, &m.state_key, Some(&r)).unwrap();
        assert_eq!(res.discarded, [d_hunk]);
        assert!(res.failed.is_empty(), "the committed hunk is a request, not a failure: {:?}", res.failed);
        assert_eq!(std::fs::read_to_string(fx.root().join("d.txt")).unwrap(), numbered(20));
        assert!(store.load(fx.repo.path(), &m.state_key).unwrap().verdicts.contains_key(&a_hunk));
    }

    #[test]
    fn committed_only_branch_refuses_stage_and_discard() {
        let (fx, base) = branch_repo();
        let r = resolve(&fx.repo, &feat(&base)).unwrap().unwrap();
        let store = ReviewStore::new();
        let err = crate::actions::stage_cleared(&fx.repo, fx.root(), &store, &r.state_key(), Some(&r)).unwrap_err();
        assert!(err.to_string().contains("isn't checked out"), "{err}");
        assert!(crate::actions::discard_reverted(&fx.repo, fx.root(), &store, &r.state_key(), Some(&r)).is_err());
    }

    #[test]
    fn base_detection_prefers_origin_head_then_trunks() {
        let (fx, base) = branch_repo();
        assert_eq!(default_base(&fx.repo).as_deref(), if TRUNKS.contains(&base.as_str()) { Some(base.as_str()) } else { None });

        let tip = fx.repo.revparse_single(&base).unwrap().id();
        fx.repo.reference("refs/remotes/origin/main", tip, true, "test").unwrap();
        fx.repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/main", true, "test").unwrap();
        assert_eq!(default_base(&fx.repo).as_deref(), Some("origin/main"));
        assert!(resolve(&fx.repo, &Target::Branch { head: "feat".into(), base: None }).unwrap().is_some());
    }

    #[test]
    fn list_shows_branches_with_ahead_counts_and_hides_the_base() {
        let (fx, base) = branch_repo();
        fx.branch("empty");
        let tip = fx.repo.revparse_single(&base).unwrap().id();
        let remote = format!("refs/remotes/origin/{base}");
        fx.repo.reference(&remote, tip, true, "test").unwrap();
        fx.repo.reference_symbolic("refs/remotes/origin/HEAD", &remote, true, "test").unwrap();
        let l = list(&fx.repo).unwrap();
        assert_eq!(l.base, Some(format!("origin/{base}")));
        let names: Vec<_> = l.branches.iter().map(|b| (b.name.as_str(), b.ahead, b.checked_out)).collect();
        assert!(names.contains(&("feat", 3, false)), "{names:?}");
        assert!(names.contains(&("empty", 0, false)), "{names:?}");
        assert!(!names.iter().any(|n| n.0 == base), "local twin of origin/{base} hidden: {names:?}");
    }

    #[test]
    fn unknown_branch_is_an_error() {
        let (fx, base) = branch_repo();
        let err = resolve(&fx.repo, &Target::Branch { head: "nope".into(), base: Some(base) }).unwrap_err();
        assert!(err.to_string().contains("nope"));
    }
}
