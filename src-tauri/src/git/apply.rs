//! The only code that writes to the working tree or the index (SPEC §5):
//! discard reverted hunks, and stage cleared files minus reverted hunks.
//!
//! Both rest on `reverse`: undo hunks on a file's bytes by checking that the
//! hunk's new side is exactly where the diff says, then swapping in its old
//! side. No fuzz: if the bytes don't match, nothing is written.

use super::diff::{delta_paths, delta_status, walk_hunks, worktree_diff, RawSides};
use super::types::FileStatus;
use crate::error::AppResult;
use git2::{IndexEntry, IndexTime, Oid, Patch, Repository};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Hunk {
    pub id: String,
    pub header: String,
    pub new_start: u32,
    pub new_lines: u32,
    pub raw: RawSides,
}

#[derive(Debug, Clone)]
pub struct FileHunks {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    /// A hunk id for discard, a path for stage.
    pub id: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardResult {
    pub discarded: Vec<String>,
    pub failed: Vec<Failure>,
    pub warnings: Vec<String>,
    /// (path, oid before, oid after), so review state can follow the file.
    #[serde(skip)]
    pub touched: Vec<(String, String, String)>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageResult {
    pub staged: Vec<String>,
    pub skipped: Vec<Failure>,
}

/// Current HEAD→worktree hunks for `paths` (every changed file when None).
pub fn hunks_for(repo: &Repository, paths: Option<&HashSet<&str>>) -> AppResult<HashMap<String, FileHunks>> {
    let diff = worktree_diff(repo)?;
    let mut out = HashMap::new();
    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let Some(status) = delta_status(delta.status()) else { continue };
        let (path, old_path) = delta_paths(&delta);
        if paths.is_some_and(|ps| !ps.contains(path.as_str())) {
            continue;
        }
        let mut hunks = vec![];
        if let Some(p) = Patch::from_diff(&diff, idx)? {
            if !p.delta().flags().is_binary() {
                for (h, raw) in walk_hunks(&p, &path)? {
                    hunks.push(Hunk { id: h.id, header: h.header, new_start: h.new_start, new_lines: h.new_lines, raw });
                }
            }
        }
        out.insert(path.clone(), FileHunks { path, old_path, status, hunks });
    }
    Ok(out)
}

/// Undo `hunks` on `content` (the new side of the diff).
pub fn reverse(content: &[u8], hunks: &[&Hunk]) -> Result<Vec<u8>, String> {
    let mut lines: Vec<Vec<u8>> = content.split_inclusive(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
    let mut order: Vec<&&Hunk> = hunks.iter().collect();
    order.sort_by(|a, b| b.new_start.cmp(&a.new_start));
    for h in order {
        let start = if h.new_lines == 0 { h.new_start as usize } else { h.new_start as usize - 1 };
        let end = start + h.new_lines as usize;
        if end > lines.len() || lines[start..end].concat() != h.raw.new {
            return Err(format!("{} no longer matches the file", h.header));
        }
        let old: Vec<Vec<u8>> = h.raw.old.split_inclusive(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
        lines.splice(start..end, old);
    }
    Ok(lines.concat())
}

fn read_opt(p: &Path) -> AppResult<Option<Vec<u8>>> {
    match std::fs::read(p) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn blob_oid(bytes: Option<&[u8]>) -> String {
    match bytes {
        Some(b) => Oid::hash_object(git2::ObjectType::Blob, b).map(|o| o.to_string()).unwrap_or_default(),
        None => "deleted".into(),
    }
}

/// The file's entry in HEAD, if it has one.
fn head_entry(repo: &Repository, path: &str) -> Option<(Oid, u32)> {
    let tree = repo.head().ok()?.peel_to_tree().ok()?;
    let e = tree.get_path(Path::new(path)).ok()?;
    Some((e.id(), e.filemode() as u32))
}

fn entry(path: &str, mode: u32, id: Oid, size: usize) -> IndexEntry {
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode,
        uid: 0,
        gid: 0,
        file_size: u32::try_from(size).unwrap_or(u32::MAX),
        id,
        flags: path.len().min(0xfff) as u16,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}

/// Mode for staging bytes at `path`: the index's, else HEAD's, else the
/// worktree file's exec bit.
fn mode_for(repo: &Repository, workdir: &Path, path: &str) -> u32 {
    if let Some(e) = repo.index().ok().and_then(|i| i.get_path(Path::new(path), 0)) {
        return e.mode;
    }
    if let Some((_, m)) = head_entry(repo, path) {
        return m;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(workdir.join(path)).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return 0o100755;
        }
    }
    let _ = workdir;
    0o100644
}

/// Put the index entry for `path` back to what HEAD has (or remove it).
fn reset_to_head(repo: &Repository, index: &mut git2::Index, path: &str) -> AppResult<()> {
    match head_entry(repo, path) {
        Some((id, mode)) => {
            let size = repo.find_blob(id).map(|b| b.size()).unwrap_or(0);
            index.add(&entry(path, mode, id, size))?;
        }
        None => {
            if index.get_path(Path::new(path), 0).is_some() {
                index.remove_path(Path::new(path))?;
            }
        }
    }
    Ok(())
}

/// Reverse-apply every hunk in `ids` to the working tree, file by file.
/// A file whose hunks don't match is left untouched.
pub fn discard(repo: &Repository, workdir: &Path, ids: &HashSet<String>) -> AppResult<DiscardResult> {
    let files = hunks_for(repo, None)?;
    let mut res = DiscardResult::default();
    let mut found: HashSet<&str> = HashSet::new();
    let mut index = repo.index()?;
    let mut index_dirty = false;

    let mut sorted: Vec<&FileHunks> = files.values().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    for f in sorted {
        let sel: Vec<&Hunk> = f.hunks.iter().filter(|h| ids.contains(&h.id)).collect();
        if sel.is_empty() {
            continue;
        }
        found.extend(sel.iter().map(|h| h.id.as_str()));
        let abs = workdir.join(&f.path);
        let before = read_opt(&abs)?;
        let after = match reverse(before.as_deref().unwrap_or_default(), &sel) {
            Ok(b) => b,
            Err(reason) => {
                for h in &sel {
                    res.failed.push(Failure { id: h.id.clone(), path: f.path.clone(), reason: reason.clone() });
                }
                continue;
            }
        };
        // Undoing all of a new file means the file shouldn't exist.
        let remove = after.is_empty()
            && matches!(f.status, FileStatus::Added | FileStatus::Untracked)
            && sel.len() == f.hunks.len();
        if remove {
            std::fs::remove_file(&abs)?;
        } else {
            if let Some(dir) = abs.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&abs, &after)?;
            #[cfg(unix)]
            if before.is_none() && head_entry(repo, &f.path).is_some_and(|(_, m)| m == 0o100755) {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&abs, std::fs::Permissions::from_mode(0o755))?;
            }
        }
        let after_opt = (!remove).then_some(after.as_slice());

        // Fully staged file: keep the index in step, or a commit would bring
        // the discarded hunk back.
        let before_oid = blob_oid(before.as_deref());
        let after_oid = blob_oid(after_opt);
        if let Some(e) = index.get_path(Path::new(&f.path), 0) {
            let head = head_entry(repo, &f.path).map(|(id, _)| id.to_string());
            if e.id.to_string() == before_oid {
                match after_opt {
                    Some(bytes) => index.add_frombuffer(&entry(&f.path, e.mode, Oid::ZERO_SHA1, bytes.len()), bytes)?,
                    None => index.remove_path(Path::new(&f.path))?,
                }
                index_dirty = true;
            } else if e.id.to_string() != after_oid && Some(e.id.to_string()) != head {
                res.warnings.push(format!(
                    "{}: the staged copy still has other changes; check `git diff --cached`",
                    f.path
                ));
            }
        }
        res.discarded.extend(sel.iter().map(|h| h.id.clone()));
        res.touched.push((f.path.clone(), before_oid, after_oid));
    }
    if index_dirty {
        index.write()?;
    }
    for id in ids {
        if !found.contains(id.as_str()) {
            res.failed.push(Failure {
                id: id.clone(),
                path: String::new(),
                reason: "no longer in the diff: the file changed since the hunk was marked".into(),
            });
        }
    }
    Ok(res)
}

/// Stage each file in `targets` (path → reverted hunk ids) as its worktree
/// content minus those hunks. The worktree itself is not touched.
pub fn stage(repo: &Repository, workdir: &Path, targets: &BTreeMap<String, HashSet<String>>) -> AppResult<StageResult> {
    let paths: HashSet<&str> = targets.keys().map(String::as_str).collect();
    let files = hunks_for(repo, Some(&paths))?;
    let mut index = repo.index()?;
    let mut res = StageResult::default();

    for (path, reverted) in targets {
        let skip = |res: &mut StageResult, reason: &str| {
            res.skipped.push(Failure { id: path.clone(), path: path.clone(), reason: reason.into() })
        };
        let Some(f) = files.get(path) else {
            skip(&mut res, "no longer changed");
            continue;
        };
        let sel: Vec<&Hunk> = f.hunks.iter().filter(|h| reverted.contains(&h.id)).collect();
        if !f.hunks.is_empty() && sel.len() == f.hunks.len() {
            reset_to_head(repo, &mut index, path)?;
            if let Some(old) = &f.old_path {
                reset_to_head(repo, &mut index, old)?;
            }
            skip(&mut res, "every hunk is marked revert");
            continue;
        }
        let abs = workdir.join(path);
        let bytes = read_opt(&abs)?;
        match (&bytes, sel.is_empty()) {
            (None, _) => {
                if index.get_path(Path::new(path), 0).is_some() {
                    index.remove_path(Path::new(path))?;
                }
            }
            (Some(_), true) => index.add_path(Path::new(path))?,
            (Some(b), false) => match reverse(b, &sel) {
                Ok(staged) => {
                    let mode = mode_for(repo, workdir, path);
                    index.add_frombuffer(&entry(path, mode, Oid::ZERO_SHA1, staged.len()), &staged)?;
                }
                Err(reason) => {
                    skip(&mut res, &reason);
                    continue;
                }
            },
        }
        // A rename is staged as delete-old + add-new.
        if let Some(old) = &f.old_path {
            if index.get_path(Path::new(old), 0).is_some() {
                index.remove_path(Path::new(old))?;
            }
        }
        res.staged.push(path.clone());
    }
    index.write()?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    fn two_hunk_file(fx: &Fixture) -> (String, String) {
        let base = numbered(40);
        fx.commit(&[("a.txt", &base)]);
        let dirty = base.replace("line 3\n", "top\n").replace("line 35\n", "bottom\n");
        fx.write("a.txt", &dirty);
        (base, dirty)
    }

    fn ids(fx: &Fixture, path: &str) -> Vec<String> {
        let set: HashSet<&str> = [path].into();
        hunks_for(&fx.repo, Some(&set)).unwrap()[path].hunks.iter().map(|h| h.id.clone()).collect()
    }

    fn read(fx: &Fixture, p: &str) -> String {
        std::fs::read_to_string(fx.root().join(p)).unwrap()
    }

    fn index_blob(fx: &Fixture, p: &str) -> Option<String> {
        let idx = fx.repo.index().unwrap();
        let e = idx.get_path(Path::new(p), 0)?;
        Some(String::from_utf8(fx.repo.find_blob(e.id).unwrap().content().to_vec()).unwrap())
    }

    /// The M5 acceptance test: revert one hunk, the other survives.
    #[test]
    fn discarding_one_hunk_keeps_the_other() {
        let fx = Fixture::new();
        let (base, _) = two_hunk_file(&fx);
        let h = ids(&fx, "a.txt");
        assert_eq!(h.len(), 2);

        let r = discard(&fx.repo, fx.root(), &[h[1].clone()].into()).unwrap();
        assert_eq!(r.discarded, [h[1].clone()]);
        assert!(r.failed.is_empty());
        assert_eq!(read(&fx, "a.txt"), base.replace("line 3\n", "top\n"));
        assert_eq!(ids(&fx, "a.txt"), [h[0].clone()], "surviving hunk keeps its id");
    }

    #[test]
    fn discarding_both_hunks_restores_head() {
        let fx = Fixture::new();
        let (base, _) = two_hunk_file(&fx);
        let h: HashSet<String> = ids(&fx, "a.txt").into_iter().collect();
        discard(&fx.repo, fx.root(), &h).unwrap();
        assert_eq!(read(&fx, "a.txt"), base);
    }

    #[test]
    fn stale_hunk_id_fails_and_touches_nothing() {
        let fx = Fixture::new();
        let (_, dirty) = two_hunk_file(&fx);
        let r = discard(&fx.repo, fx.root(), &["000000000000".to_string()].into()).unwrap();
        assert!(r.discarded.is_empty());
        assert_eq!(r.failed.len(), 1);
        assert!(r.failed[0].reason.contains("no longer in the diff"));
        assert_eq!(read(&fx, "a.txt"), dirty);
    }

    #[test]
    fn reverse_refuses_mismatched_bytes() {
        let h = Hunk {
            id: "x".into(),
            header: "@@ -1,1 +1,1 @@".into(),
            new_start: 1,
            new_lines: 1,
            raw: RawSides { old: b"a\n".to_vec(), new: b"b\n".to_vec() },
        };
        assert_eq!(reverse(b"b\nrest\n", &[&h]).unwrap(), b"a\nrest\n");
        assert!(reverse(b"c\nrest\n", &[&h]).is_err());
        assert!(reverse(b"", &[&h]).is_err());
    }

    #[test]
    fn missing_final_newline_survives() {
        let fx = Fixture::new();
        fx.commit(&[("n.txt", "one\ntwo")]);
        fx.write("n.txt", "one\nTWO");
        let h: HashSet<String> = ids(&fx, "n.txt").into_iter().collect();
        discard(&fx.repo, fx.root(), &h).unwrap();
        assert_eq!(read(&fx, "n.txt"), "one\ntwo");

        fx.write("n.txt", "one\ntwo\n");
        let h: HashSet<String> = ids(&fx, "n.txt").into_iter().collect();
        discard(&fx.repo, fx.root(), &h).unwrap();
        assert_eq!(read(&fx, "n.txt"), "one\ntwo");
    }

    #[test]
    fn discarding_a_new_file_removes_it_and_a_deleted_file_comes_back() {
        let fx = Fixture::new();
        fx.commit(&[("gone.txt", "keep\nme\n")]);
        fx.write("new/extra.txt", "x\ny\n");
        fx.remove("gone.txt");
        let mut h: HashSet<String> = ids(&fx, "new/extra.txt").into_iter().collect();
        h.extend(ids(&fx, "gone.txt"));
        let r = discard(&fx.repo, fx.root(), &h).unwrap();
        assert_eq!(r.discarded.len(), 2, "{:?}", r.failed);
        assert!(!fx.root().join("new/extra.txt").exists());
        assert_eq!(read(&fx, "gone.txt"), "keep\nme\n");
    }

    #[test]
    fn discard_keeps_a_fully_staged_index_in_step() {
        let fx = Fixture::new();
        let (base, dirty) = two_hunk_file(&fx);
        fx.stage("a.txt");
        assert_eq!(index_blob(&fx, "a.txt").unwrap(), dirty);
        let h = ids(&fx, "a.txt");
        let r = discard(&fx.repo, fx.root(), &[h[1].clone()].into()).unwrap();
        let expect = base.replace("line 3\n", "top\n");
        assert_eq!(read(&fx, "a.txt"), expect);
        assert_eq!(index_blob(&fx, "a.txt").unwrap(), expect, "no staged copy of the discarded hunk");
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn stage_leaves_reverted_hunks_unstaged() {
        let fx = Fixture::new();
        let (base, dirty) = two_hunk_file(&fx);
        let h = ids(&fx, "a.txt");
        let targets: BTreeMap<String, HashSet<String>> = [("a.txt".to_string(), [h[1].clone()].into())].into();
        let r = stage(&fx.repo, fx.root(), &targets).unwrap();
        assert_eq!(r.staged, ["a.txt"]);
        assert_eq!(index_blob(&fx, "a.txt").unwrap(), base.replace("line 3\n", "top\n"));
        assert_eq!(read(&fx, "a.txt"), dirty, "worktree untouched");
    }

    #[test]
    fn stage_whole_new_and_deleted_files() {
        let fx = Fixture::new();
        fx.commit(&[("old.txt", "x\n"), ("keep.txt", "k\n")]);
        fx.write("new.txt", "n\n");
        fx.remove("old.txt");
        let targets: BTreeMap<String, HashSet<String>> =
            [("new.txt".to_string(), HashSet::new()), ("old.txt".to_string(), HashSet::new())].into();
        let r = stage(&fx.repo, fx.root(), &targets).unwrap();
        assert_eq!(r.staged, ["new.txt", "old.txt"]);
        assert_eq!(index_blob(&fx, "new.txt").unwrap(), "n\n");
        assert!(index_blob(&fx, "old.txt").is_none(), "deletion staged");
    }

    #[test]
    fn stage_skips_fully_reverted_and_clean_files() {
        let fx = Fixture::new();
        let (base, _) = two_hunk_file(&fx);
        fx.stage("a.txt");
        let all: HashSet<String> = ids(&fx, "a.txt").into_iter().collect();
        let targets: BTreeMap<String, HashSet<String>> =
            [("a.txt".to_string(), all), ("clean.txt".to_string(), HashSet::new())].into();
        let r = stage(&fx.repo, fx.root(), &targets).unwrap();
        assert!(r.staged.is_empty());
        let reasons: Vec<_> = r.skipped.iter().map(|s| (s.path.as_str(), s.reason.as_str())).collect();
        assert_eq!(reasons, [("a.txt", "every hunk is marked revert"), ("clean.txt", "no longer changed")]);
        assert_eq!(index_blob(&fx, "a.txt").unwrap(), base, "index back to HEAD");
    }
}
