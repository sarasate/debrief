//! Attribution for a branch review (docs/PLAN.md M9): which sessions and
//! edits count, which commits Claude co-authored, and the BY COMMIT groups.

use crate::error::AppResult;
use crate::git::target::Range;
use crate::time::epoch_secs;
use crate::transcript::parse::ParsedSession;
use git2::{Repository, Sort};
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Commits read per branch; a longer branch shows its newest this many.
const MAX_COMMITS: usize = 500;

static CLAUDE_TRAILER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)^co-authored-by:.*(claude|@anthropic\.com)").expect("static regex"));
static TRAILER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z-]*: ").expect("static regex"));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCommit {
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
    /// Paths it changed against its first parent (new and old names).
    #[serde(skip)]
    pub paths: Vec<String>,
}

/// Non-merge commits on the branch since it left the base, oldest first.
pub fn commits(repo: &Repository, range: &Range) -> AppResult<Vec<BranchCommit>> {
    let mut walk = repo.revwalk()?;
    walk.push(range.head_oid()?)?;
    walk.hide(range.merge_base_oid()?)?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
    let mut out = Vec::new();
    for oid in walk {
        let c = repo.find_commit(oid?)?;
        if c.parent_count() > 1 {
            continue;
        }
        let parent = c.parents().next().map(|p| p.tree()).transpose()?;
        let diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&c.tree()?), None)?;
        let mut paths = Vec::new();
        for d in diff.deltas() {
            for f in [d.new_file(), d.old_file()] {
                if let Some(p) = f.path().map(|p| p.to_string_lossy().replace('\\', "/")) {
                    if !paths.contains(&p) {
                        paths.push(p);
                    }
                }
            }
        }
        let message = String::from_utf8_lossy(c.message_bytes()).to_string();
        out.push(BranchCommit {
            sha: c.id().to_string(),
            short: c.id().to_string()[..7].to_string(),
            subject: c.summary().ok().flatten().unwrap_or_default().to_string(),
            body: body_without_trailers(&message),
            author: c.author().name().unwrap_or_default().to_string(),
            time: c.time().seconds() * 1000,
            claude: CLAUDE_TRAILER.is_match(&message),
            paths,
        });
    }
    if out.len() > MAX_COMMITS {
        out.drain(..out.len() - MAX_COMMITS);
    }
    Ok(out)
}

/// A committed hunk marked revert, to ask Claude to undo in a new commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertRequest {
    pub hunk_id: String,
    pub path: String,
    pub header: String,
}

/// The hunks in `ids` that the branch diff holds and that are committed
/// (not in HEAD → worktree), in file order.
pub fn revert_requests(repo: &Repository, range: &Range, ids: &HashSet<String>) -> AppResult<Vec<RevertRequest>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let uncommitted: HashSet<String> = if range.includes_worktree {
        crate::git::apply::hunks_for(repo, None)?.into_values().flat_map(|f| f.hunks.into_iter().map(|h| h.id)).collect()
    } else {
        HashSet::new()
    };
    let diff = crate::git::diff::review_diff(repo, Some(range))?;
    let mut out = Vec::new();
    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let (path, _) = crate::git::diff::delta_paths(&delta);
        let Some(patch) = git2::Patch::from_diff(&diff, idx)? else { continue };
        for (h, _) in crate::git::diff::walk_hunks(&patch, &path)? {
            if ids.contains(&h.id) && !uncommitted.contains(&h.id) {
                out.push(RevertRequest { hunk_id: h.id, path: path.clone(), header: h.header });
            }
        }
    }
    Ok(out)
}

/// Everything after the subject line, minus a trailing block of
/// `Key: value` trailers.
fn body_without_trailers(message: &str) -> String {
    let mut lines: Vec<&str> = message.lines().skip(1).collect();
    while lines.last().is_some_and(|l| l.trim().is_empty() || TRAILER.is_match(l.trim())) {
        lines.pop();
    }
    lines.join("\n").trim().to_string()
}

/// The sessions' edits that can explain a branch: made after the branch
/// left its base, and, for any file with edits recorded on this branch,
/// only those (a session can switch branches halfway, and work done on
/// `main` just before branching is often what the first commit holds).
pub fn filter_sessions(sessions: &[&ParsedSession], range: &Range, since_secs: i64) -> Vec<ParsedSession> {
    let after = |ts: &str| epoch_secs(ts).is_none_or(|t| t >= since_secs);
    let on_branch: HashSet<&str> = sessions
        .iter()
        .flat_map(|s| s.entries.iter())
        .filter(|e| after(&e.timestamp) && e.branch.as_deref() == Some(range.head.as_str()))
        .map(|e| e.path.as_str())
        .collect();
    sessions
        .iter()
        .map(|s| {
            let mut s = (*s).clone();
            s.entries.retain(|e| {
                after(&e.timestamp) && (!on_branch.contains(e.path.as_str()) || e.branch.as_deref() == Some(range.head.as_str()))
            });
            s
        })
        .filter(|s| !s.entries.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailers_are_stripped_from_the_body_and_detected() {
        let m = "feat: x\n\nWhy we did it.\nMore.\n\nCo-Authored-By: Claude Opus 5 <noreply@anthropic.com>\nSigned-off-by: Dev <d@x>\n";
        assert_eq!(body_without_trailers(m), "Why we did it.\nMore.");
        assert!(CLAUDE_TRAILER.is_match(m));
        assert!(CLAUDE_TRAILER.is_match("x\n\nco-authored-by: Someone <bot@anthropic.com>"));
        assert!(!CLAUDE_TRAILER.is_match("x\n\nCo-Authored-By: Alex <alex@example.com>"));
        assert!(!CLAUDE_TRAILER.is_match("fix: mention claude in the subject only"));
        assert_eq!(body_without_trailers("just a subject"), "");
    }
}

#[cfg(test)]
mod m9 {
    use crate::git::fixture::Fixture;
    use crate::git::target::Target;
    use crate::review::{review_model, GroupKind};
    use crate::review_state::ReviewStore;
    use crate::time::{iso_from_ms, now_ms};
    use crate::transcript::sessions::TranscriptCache;
    use crate::transcript::slug;

    const TRAILER: &str = "\n\nCo-Authored-By: Claude Opus 5 <noreply@anthropic.com>";

    /// One session's transcript: turns of (prompt, [(file, branch)]), each
    /// edit a second after the last, starting at `t0` ms.
    fn transcript(id: &str, cwd: &str, t0: i64, turns: &[(&str, &[(&str, &str)])]) -> String {
        let mut t = t0;
        let mut tick = || {
            t += 1000;
            iso_from_ms(t)
        };
        let mut out = vec![format!(r#"{{"type":"ai-title","sessionId":"{id}","aiTitle":"Session {id}"}}"#)];
        for (n, (prompt, edits)) in turns.iter().enumerate() {
            let branch = edits.first().map_or("main", |e| e.1);
            out.push(format!(
                r#"{{"type":"user","sessionId":"{id}","cwd":"{cwd}","gitBranch":"{branch}","timestamp":"{}","origin":{{"kind":"human"}},"message":{{"role":"user","content":"{prompt}"}}}}"#,
                tick()
            ));
            for (i, (file, branch)) in edits.iter().enumerate() {
                out.push(format!(
                    r#"{{"type":"assistant","sessionId":"{id}","cwd":"{cwd}","gitBranch":"{branch}","timestamp":"{}","message":{{"content":[{{"type":"tool_use","id":"{id}-{n}-{i}","name":"Write","input":{{"file_path":"{cwd}/{file}"}}}}]}}}}"#,
                    tick()
                ));
            }
            out.push(format!(
                r#"{{"type":"assistant","sessionId":"{id}","cwd":"{cwd}","gitBranch":"{branch}","timestamp":"{}","message":{{"content":[{{"type":"text","text":"Done: {prompt}"}}]}}}}"#,
                tick()
            ));
        }
        out.join("\n") + "\n"
    }

    /// The M9 acceptance case: a branch built over two Claude sessions,
    /// a commit Claude made through Bash, and my own commit.
    #[test]
    fn branch_attribution_across_sessions_trailers_and_my_commits() {
        let fx = Fixture::new();
        fx.commit(&[("c.ts", "c\n"), ("keep.ts", "k\n")]);
        let base = crate::git::target::checked_out(&fx.repo).unwrap();
        let root = fx.root().to_string_lossy().to_string();
        let now = now_ms();

        // Session 0 edited c.ts before the branch point: must not count.
        // Session 1 starts on the base branch, then moves to feat.
        // Session 2 works on feat only.
        let projects = tempfile::tempdir().unwrap();
        let dir = projects.path().join(slug(fx.root()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("s0.jsonl"), transcript("s0", &root, now - 3_600_000, &[("old work on c", &[("c.ts", "main")])])).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            transcript("s1", &root, now + 10_000, &[("start a on main", &[("a.ts", base.as_str())]), ("polish a on feat", &[("a.ts", "feat")])]),
        )
        .unwrap();
        std::fs::write(dir.join("s2.jsonl"), transcript("s2", &root, now + 60_000, &[("add b", &[("b.ts", "feat")])])).unwrap();

        fx.branch("feat");
        fx.checkout("feat");
        fx.commit_msg(&[("a.ts", "a\n")], &format!("feat: add a{TRAILER}"));
        fx.commit_msg(&[("b.ts", "b\n")], &format!("feat: add b{TRAILER}"));
        fx.commit_msg(&[("gen/bash.ts", "generated by a script\n")], &format!("chore: run codegen\n\nClaude ran the generator.{TRAILER}"));
        fx.commit_msg(&[("mine.ts", "mine\n"), ("c.ts", "c changed by me\n")], "wip: my own tweaks");
        fx.checkout(&base);

        let (state, cache, store) = (fx.state(), TranscriptCache::new(), ReviewStore::new());
        state.set_target(Target::Branch { head: "feat".into(), base: Some(base.clone()) }).unwrap();
        let m = review_model(&state, &cache, &store, Some(projects.path()), None).unwrap();

        let summary: Vec<(String, GroupKind, Vec<&str>)> = m
            .groups
            .iter()
            .map(|g| (g.title.clone(), g.kind, g.files.iter().map(|f| f.path.as_str()).collect()))
            .collect();
        assert_eq!(
            summary,
            [
                ("S1 · POLISH A ON FEAT".to_string(), GroupKind::Turn, vec!["a.ts"]),
                ("S2 · ADD B".to_string(), GroupKind::Turn, vec!["b.ts"]),
                (format!("{} CHORE: RUN CODEGEN", m.groups[2].commit.as_ref().unwrap().short.to_uppercase()), GroupKind::Commit, vec!["gen/bash.ts"]),
                ("UNATTRIBUTED".to_string(), GroupKind::Unattributed, vec!["c.ts", "mine.ts"]),
            ],
            "{:#?}",
            m.groups
        );
        // a.ts: edits on feat win over the earlier one on the base branch.
        let a = &m.groups[0].files[0];
        assert!(a.also_turns.is_empty(), "the main-branch edit of a.ts is dropped: {:?}", a.also_turns);
        assert_eq!(m.groups[2].briefing, "Claude ran the generator.");
        assert_eq!(m.session.as_ref().unwrap().id, "s2", "newest contributing session");
        assert!(m.turns.iter().all(|t| t.session_id != "s0"), "s0 predates the branch");

        use crate::flags::FlagKind::Unattributed;
        let unattributed = |p: &str| m.files[p].flags.iter().any(|f| f.kind == Unattributed);
        assert!(unattributed("mine.ts") && unattributed("c.ts"));
        assert!(!unattributed("gen/bash.ts"), "a Claude-trailer commit counts as Claude's");

        // BY COMMIT: one group per commit, a file under its last commit.
        let by_commit: Vec<(String, Vec<&str>)> =
            m.commit_groups.iter().map(|g| (g.prompt.clone().unwrap_or_default(), g.files.iter().map(|f| f.path.as_str()).collect())).collect();
        assert_eq!(
            by_commit,
            [
                ("feat: add a".to_string(), vec!["a.ts"]),
                ("feat: add b".to_string(), vec!["b.ts"]),
                ("chore: run codegen".to_string(), vec!["gen/bash.ts"]),
                ("wip: my own tweaks".to_string(), vec!["c.ts", "mine.ts"]),
            ]
        );
        assert!(m.commit_groups.iter().take(3).all(|g| g.commit.as_ref().unwrap().claude));
        assert!(!m.commit_groups[3].commit.as_ref().unwrap().claude);
    }

    #[test]
    fn uncommitted_changes_get_their_own_commit_group() {
        let fx = Fixture::new();
        fx.commit(&[("a.ts", "a\n")]);
        let base = crate::git::target::checked_out(&fx.repo).unwrap();
        fx.branch("feat");
        fx.checkout("feat");
        fx.commit_msg(&[("a.ts", "a2\n"), ("b.ts", "b\n")], "feat: both");
        fx.write("a.ts", "a3\n");
        let (state, cache, store) = (fx.state(), TranscriptCache::new(), ReviewStore::new());
        state.set_target(Target::Branch { head: "feat".into(), base: Some(base) }).unwrap();
        let m = review_model(&state, &cache, &store, None, None).unwrap();
        let g: Vec<(GroupKind, Vec<&str>)> = m.commit_groups.iter().map(|g| (g.kind, g.files.iter().map(|f| f.path.as_str()).collect())).collect();
        assert_eq!(g, [(GroupKind::Commit, vec!["b.ts"]), (GroupKind::Uncommitted, vec!["a.ts"])]);
        assert_eq!(m.commit_groups[1].files[0].also_commits.len(), 1, "a.ts was also in the commit");
    }
}
