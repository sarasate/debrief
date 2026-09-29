//! Stage cleared and discard reverted (SPEC §5): `git::apply` driven by the
//! review state. These are two of the three actions that write to the repo;
//! each runs only on an explicit key press, and discard only after the UI's
//! confirmation.

use crate::error::AppResult;
use crate::git::apply::{self, DiscardResult, Failure, StageResult};
use crate::git::status::worktree_oid;
use crate::review_state::{ReviewStore, Verdict};
use git2::Repository;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Stage every viewed file whose content is still what was viewed, minus
/// hunks marked revert. Files changed since they were cleared are skipped.
pub fn stage_cleared(repo: &Repository, workdir: &Path, store: &ReviewStore, key: &str) -> AppResult<StageResult> {
    let st = store.load(repo.path(), key)?;
    let reverted: HashSet<String> =
        st.verdicts.iter().filter(|(_, v)| **v == Verdict::Revert).map(|(id, _)| id.clone()).collect();
    let mut targets = BTreeMap::new();
    let mut stale = vec![];
    for (path, oid) in &st.viewed {
        if worktree_oid(workdir, path) == *oid {
            targets.insert(path.clone(), reverted.clone());
        } else {
            stale.push(path.clone());
        }
    }
    let mut res = apply::stage(repo, workdir, &targets)?;
    res.skipped.extend(stale.into_iter().map(|p| Failure {
        id: p.clone(),
        path: p,
        reason: "changed since cleared".into(),
    }));
    Ok(res)
}

/// Reverse-apply every hunk marked revert, then move viewed marks along with
/// the files, so our own discard doesn't read as "changed since cleared".
pub fn discard_reverted(repo: &Repository, workdir: &Path, store: &ReviewStore, key: &str) -> AppResult<DiscardResult> {
    let st = store.load(repo.path(), key)?;
    let ids: HashSet<String> =
        st.verdicts.iter().filter(|(_, v)| **v == Verdict::Revert).map(|(id, _)| id.clone()).collect();
    if ids.is_empty() {
        return Ok(DiscardResult::default());
    }
    let res = apply::discard(repo, workdir, &ids)?;
    store.update(repo.path(), key, |s| {
        for (path, before, after) in &res.touched {
            if s.viewed.get(path) == Some(before) {
                s.viewed.insert(path.clone(), after.clone());
            }
        }
        for id in &res.discarded {
            s.verdicts.remove(id);
        }
    })?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;
    use crate::review::review_model;
    use crate::review_state::NO_SESSION;
    use crate::transcript::sessions::TranscriptCache;

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    #[test]
    fn stage_then_discard_through_review_state() {
        let fx = Fixture::new();
        let base = numbered(40);
        fx.commit(&[("a.txt", &base), ("b.txt", "b\n"), ("c.txt", "c\n")]);
        let dirty = base.replace("line 3\n", "top\n").replace("line 35\n", "bottom\n");
        fx.write("a.txt", &dirty);
        fx.write("b.txt", "b2\n");
        fx.write("c.txt", "c2\n");
        let (cache, store, state) = (TranscriptCache::new(), ReviewStore::new(), fx.state());
        let m = review_model(&state, &cache, &store, None, None).unwrap();
        let a = m.files["a.txt"].clone();
        let git = fx.repo.path();
        store
            .update(git, NO_SESSION, |s| {
                s.viewed.insert("a.txt".into(), a.oid.clone());
                s.viewed.insert("b.txt".into(), m.files["b.txt"].oid.clone());
                s.verdicts.insert(a.hunk_ids[0].clone(), Verdict::Revert);
            })
            .unwrap();
        fx.write("b.txt", "b3\n"); // changed after it was cleared

        let staged = stage_cleared(&fx.repo, fx.root(), &store, NO_SESSION).unwrap();
        assert_eq!(staged.staged, ["a.txt"]);
        assert_eq!(staged.skipped.iter().map(|s| (s.path.as_str(), s.reason.as_str())).collect::<Vec<_>>(), [("b.txt", "changed since cleared")]);
        let idx = fx.repo.index().unwrap();
        let blob = |p: &str| idx.get_path(Path::new(p), 0).map(|e| String::from_utf8(fx.repo.find_blob(e.id).unwrap().content().to_vec()).unwrap());
        assert_eq!(blob("a.txt").unwrap(), base.replace("line 35\n", "bottom\n"), "reverted top hunk left unstaged");
        assert_eq!(blob("c.txt").unwrap(), "c\n", "uncleared file not staged");

        let d = discard_reverted(&fx.repo, fx.root(), &store, NO_SESSION).unwrap();
        assert_eq!(d.discarded, [a.hunk_ids[0].clone()]);
        assert_eq!(std::fs::read_to_string(fx.root().join("a.txt")).unwrap(), base.replace("line 35\n", "bottom\n"));

        let m = review_model(&state, &cache, &store, None, None).unwrap();
        assert_eq!(m.invalidated, ["b.txt"], "only the real edit; our own discard isn't 'changed since cleared'");
        assert!(m.files["a.txt"].viewed);
        assert!(m.verdicts.is_empty());
        assert_eq!(m.files["a.txt"].hunk_ids, [a.hunk_ids[1].clone()]);
    }

    #[test]
    fn nothing_marked_means_nothing_happens() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        fx.write("a.txt", "b\n");
        let store = ReviewStore::new();
        assert!(discard_reverted(&fx.repo, fx.root(), &store, NO_SESSION).unwrap().discarded.is_empty());
        assert!(stage_cleared(&fx.repo, fx.root(), &store, NO_SESSION).unwrap().staged.is_empty());
        assert_eq!(std::fs::read_to_string(fx.root().join("a.txt")).unwrap(), "b\n");
    }
}
