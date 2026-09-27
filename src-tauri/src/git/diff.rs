use super::types::{DiffHunk, DiffLine, DiffLineKind, FileDiff, FileStatus};
use crate::error::{AppError, AppResult};
use crate::state::RepoState;
use git2::{Delta, Diff, DiffDelta, DiffFindOptions, DiffOptions, Patch, Repository};
use sha1::{Digest, Sha1};

/// HEAD against the working tree, staged and unstaged combined: "what changed
/// since the last commit". Untracked files are included with their content,
/// ignored files are not, and renames are detected.
pub(crate) fn worktree_diff(repo: &Repository) -> AppResult<Diff<'_>> {
    let head_tree = match repo.head() {
        Ok(h) => Some(h.peel_to_tree()?),
        Err(_) => None, // unborn branch: everything is new
    };
    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .include_ignored(false)
        .include_typechange(true)
        .context_lines(3);
    let mut diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))?;
    let mut find = DiffFindOptions::new();
    find.renames(true).for_untracked(true);
    diff.find_similar(Some(&mut find))?;
    Ok(diff)
}

pub(crate) fn delta_status(d: Delta) -> Option<FileStatus> {
    match d {
        Delta::Added | Delta::Copied => Some(FileStatus::Added),
        Delta::Modified => Some(FileStatus::Modified),
        Delta::Deleted => Some(FileStatus::Deleted),
        Delta::Renamed => Some(FileStatus::Renamed),
        Delta::Typechange => Some(FileStatus::Typechange),
        Delta::Untracked => Some(FileStatus::Untracked),
        Delta::Conflicted => Some(FileStatus::Conflicted),
        Delta::Unmodified | Delta::Ignored | Delta::Unreadable => None,
    }
}

/// Repo-relative (path, old path). The old path is only set when it differs.
pub(crate) fn delta_paths(delta: &DiffDelta) -> (String, Option<String>) {
    let lossy = |p: Option<&std::path::Path>| p.map(|p| p.to_string_lossy().to_string());
    let old = lossy(delta.old_file().path());
    let path = lossy(delta.new_file().path())
        .or_else(|| old.clone())
        .unwrap_or_default();
    let old = old.filter(|o| o != &path);
    (path, old)
}

pub fn diff_file(state: &RepoState, path: &str) -> AppResult<FileDiff> {
    let repo = state.open()?;
    let diff = worktree_diff(&repo)?;
    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let Some(status) = delta_status(delta.status()) else { continue };
        let (new_path, old_path) = delta_paths(&delta);
        if new_path != path {
            continue;
        }
        let Some(patch) = Patch::from_diff(&diff, idx)? else {
            return Ok(FileDiff { path: new_path, old_path, status, is_binary: true, hunks: vec![] });
        };
        let is_binary = patch.delta().flags().is_binary();
        let hunks = if is_binary { vec![] } else { collect_hunks(&patch, &new_path)? };
        return Ok(FileDiff { path: new_path, old_path, status, is_binary, hunks });
    }
    Err(AppError::Input(format!("{path} has no uncommitted changes")))
}

fn collect_hunks(patch: &Patch, path: &str) -> AppResult<Vec<DiffHunk>> {
    let mut hunks = Vec::with_capacity(patch.num_hunks());
    for h in 0..patch.num_hunks() {
        let (hunk, line_count) = patch.hunk(h)?;
        let mut body = Vec::new();
        let mut lines = Vec::with_capacity(line_count);
        for l in 0..line_count {
            let line = patch.line_in_hunk(h, l)?;
            let kind = match line.origin() {
                '+' => DiffLineKind::Addition,
                '-' => DiffLineKind::Deletion,
                ' ' => DiffLineKind::Context,
                _ => continue, // "\ No newline at end of file" and friends
            };
            body.push(line.origin() as u8);
            body.extend_from_slice(line.content());
            let content = String::from_utf8_lossy(line.content());
            lines.push(DiffLine {
                kind,
                content: content.trim_end_matches(['\n', '\r']).to_string(),
                old_lineno: line.old_lineno(),
                new_lineno: line.new_lineno(),
            });
        }
        let header = String::from_utf8_lossy(hunk.header()).trim_end().to_string();
        hunks.push(DiffHunk {
            id: hunk_id(path, hunk.old_start(), hunk.old_lines(), hunk.new_start(), hunk.new_lines(), &body),
            header,
            old_start: hunk.old_start(),
            old_lines: hunk.old_lines(),
            new_start: hunk.new_start(),
            new_lines: hunk.new_lines(),
            lines,
        });
    }
    Ok(hunks)
}

/// sha1(path + ranges + body), first 12 hex chars (SPEC §3.1). The body is
/// every line's origin char followed by its raw bytes.
pub fn hunk_id(path: &str, old_start: u32, old_lines: u32, new_start: u32, new_lines: u32, body: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(path.as_bytes());
    h.update(format!("\0{old_start},{old_lines},{new_start},{new_lines}\0").as_bytes());
    h.update(body);
    h.finalize().iter().take(6).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Fixture;

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    /// Replace line `at` (1-based) of a `numbered` file.
    fn edit_line(src: &str, at: usize, text: &str) -> String {
        src.lines()
            .enumerate()
            .map(|(i, l)| if i + 1 == at { format!("{text}\n") } else { format!("{l}\n") })
            .collect()
    }

    #[test]
    fn hunk_id_is_12_hex_chars_and_deterministic() {
        let a = hunk_id("a.txt", 1, 3, 1, 4, b"+x\n");
        assert_eq!(a.len(), 12);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, hunk_id("a.txt", 1, 3, 1, 4, b"+x\n"));
        assert_ne!(a, hunk_id("b.txt", 1, 3, 1, 4, b"+x\n"));
        assert_ne!(a, hunk_id("a.txt", 2, 3, 1, 4, b"+x\n"));
        assert_ne!(a, hunk_id("a.txt", 1, 3, 1, 4, b"+y\n"));
    }

    #[test]
    fn modified_file_has_lines_and_line_numbers() {
        let fx = Fixture::new();
        let base = numbered(10);
        fx.commit(&[("a.txt", &base)]);
        fx.write("a.txt", &edit_line(&base, 5, "changed"));

        let d = diff_file(&fx.state(), "a.txt").unwrap();
        assert_eq!(d.status, FileStatus::Modified);
        assert_eq!(d.hunks.len(), 1);
        let h = &d.hunks[0];
        assert!(h.header.starts_with("@@ -2,7 +2,7 @@"), "{}", h.header);
        let del = h.lines.iter().find(|l| l.kind == DiffLineKind::Deletion).unwrap();
        let add = h.lines.iter().find(|l| l.kind == DiffLineKind::Addition).unwrap();
        assert_eq!((del.content.as_str(), del.old_lineno), ("line 5", Some(5)));
        assert_eq!((add.content.as_str(), add.new_lineno), ("changed", Some(5)));
    }

    #[test]
    fn hunk_ids_stable_across_refresh() {
        let fx = Fixture::new();
        let base = numbered(40);
        fx.commit(&[("a.txt", &base)]);
        fx.write("a.txt", &edit_line(&edit_line(&base, 3, "top"), 35, "bottom"));

        let first = diff_file(&fx.state(), "a.txt").unwrap();
        let again = diff_file(&fx.state(), "a.txt").unwrap();
        assert_eq!(first.hunks.len(), 2);
        let ids = |d: &FileDiff| d.hunks.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&first), ids(&again));
        assert_ne!(first.hunks[0].id, first.hunks[1].id);
    }

    #[test]
    fn editing_one_hunk_keeps_the_other_hunk_id() {
        let fx = Fixture::new();
        let base = numbered(40);
        fx.commit(&[("a.txt", &base)]);
        let two_hunks = edit_line(&edit_line(&base, 3, "top"), 35, "bottom");
        fx.write("a.txt", &two_hunks);
        let before = diff_file(&fx.state(), "a.txt").unwrap();

        // Rewrite only the lower hunk (same line count, so no range shift).
        fx.write("a.txt", &edit_line(&two_hunks, 35, "bottom, again"));
        let after = diff_file(&fx.state(), "a.txt").unwrap();

        assert_eq!(before.hunks[0].id, after.hunks[0].id);
        assert_ne!(before.hunks[1].id, after.hunks[1].id);
    }

    #[test]
    fn staged_and_unstaged_changes_are_combined() {
        let fx = Fixture::new();
        let base = numbered(40);
        fx.commit(&[("a.txt", &base)]);
        let staged = edit_line(&base, 3, "staged");
        fx.write("a.txt", &staged);
        fx.stage("a.txt");
        fx.write("a.txt", &edit_line(&staged, 35, "unstaged"));

        let d = diff_file(&fx.state(), "a.txt").unwrap();
        assert_eq!(d.hunks.len(), 2, "HEAD vs workdir shows both edits");
    }

    #[test]
    fn untracked_file_diffs_as_additions() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        fx.write("new/b.txt", "one\ntwo\n");

        let d = diff_file(&fx.state(), "new/b.txt").unwrap();
        assert_eq!(d.status, FileStatus::Untracked);
        assert_eq!(d.hunks.len(), 1);
        assert!(d.hunks[0].lines.iter().all(|l| l.kind == DiffLineKind::Addition));
    }

    #[test]
    fn binary_file_has_no_hunks() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        fx.write_bytes("img.bin", &[0, 159, 146, 150, 0, 1, 2, 3]);

        let d = diff_file(&fx.state(), "img.bin").unwrap();
        assert!(d.is_binary);
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn clean_path_is_an_error() {
        let fx = Fixture::new();
        fx.commit(&[("a.txt", "a\n")]);
        assert!(diff_file(&fx.state(), "a.txt").is_err());
    }
}
