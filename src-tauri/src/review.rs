//! The review model (SPEC §7): the changed files grouped by intent. Noise
//! and flags join in M3, review state in M4.

use crate::error::AppResult;
use crate::git::types::{ChangedFile, RepoStatus};
use crate::noise::Noise;
use crate::state::RepoState;
use crate::transcript::parse::ParsedSession;
use crate::transcript::sessions::{list_sessions, SessionInfo, TranscriptCache};
use crate::transcript::{EditTool, Turn};
use serde::Serialize;
use git2::Repository;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

const TITLE_WORDS: usize = 6;
/// Upper bound on commits walked when looking for already-committed edits.
const MAX_SESSION_COMMITS: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GroupKind {
    Turn,
    Unattributed,
    Generated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupFile {
    pub path: String,
    /// Edit tools used on this file in the group's turn.
    pub tools: Vec<EditTool>,
    /// Earlier turns that also edited this file.
    pub also_turns: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentGroup {
    pub id: String,
    pub kind: GroupKind,
    pub title: String,
    pub briefing: String,
    pub turn: Option<usize>,
    pub prompt: Option<String>,
    pub files: Vec<GroupFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRef {
    pub index: usize,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewModel {
    pub status: RepoStatus,
    /// The session the groups come from; None when the repo has none.
    pub session: Option<SessionInfo>,
    /// Every turn of the session, for "also touched in turn n" labels.
    pub turns: Vec<TurnRef>,
    /// Turns in prompt order, then UNATTRIBUTED, then GENERATED; empty
    /// groups are left out.
    pub groups: Vec<IntentGroup>,
}

/// `session_id` None picks the most recently written session.
pub fn review_model(
    state: &RepoState,
    cache: &TranscriptCache,
    projects: Option<&Path>,
    session_id: Option<&str>,
) -> AppResult<ReviewModel> {
    let status = crate::git::status::repo_status(state)?;
    let roots = crate::transcript::repo_roots(&state.path()?);
    let sessions = match projects {
        Some(p) => list_sessions(cache, p, &roots)?,
        None => vec![],
    };
    let chosen = session_id
        .and_then(|id| sessions.iter().find(|s| s.0.id == id))
        .or_else(|| sessions.first());
    let noise = Noise::defaults()?;
    let parsed = chosen.map(|s| s.1.as_ref());
    let pending = match parsed {
        Some(p) => Some(drop_committed(p, &committed_since(&state.open()?, p)?)),
        None => None,
    };
    Ok(ReviewModel {
        groups: group(&status.files, pending.as_ref(), &noise),
        turns: parsed
            .map(|p| p.turns.iter().map(|t| TurnRef { index: t.index, title: short_title(&t.prompt) }).collect())
            .unwrap_or_default(),
        session: chosen.map(|s| s.0.clone()),
        status,
    })
}

/// Latest commit time (epoch secs) per path, over the commits made since
/// the session's first edit.
fn committed_since(repo: &Repository, session: &ParsedSession) -> AppResult<HashMap<String, i64>> {
    let mut out = HashMap::new();
    let Some(since) = session.entries.first().and_then(|e| epoch_secs(&e.timestamp)) else { return Ok(out) };
    let Ok(head) = repo.head().and_then(|h| h.peel_to_commit()) else { return Ok(out) };
    let mut walk = repo.revwalk()?;
    walk.push(head.id())?;
    walk.set_sorting(git2::Sort::TIME)?;
    for oid in walk.take(MAX_SESSION_COMMITS) {
        let commit = repo.find_commit(oid?)?;
        let time = commit.time().seconds();
        if time < since {
            break;
        }
        let parent = commit.parents().next().map(|p| p.tree()).transpose()?;
        let diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&commit.tree()?), None)?;
        for d in diff.deltas() {
            for f in [d.old_file(), d.new_file()] {
                if let Some(p) = f.path() {
                    let t = out.entry(p.to_string_lossy().replace('\\', "/")).or_insert(time);
                    *t = (*t).max(time);
                }
            }
        }
    }
    Ok(out)
}

/// The session without edits that a later commit already contains. Those
/// changes are in HEAD, so they can't explain what is dirty now.
fn drop_committed(session: &ParsedSession, committed: &HashMap<String, i64>) -> ParsedSession {
    let mut s = session.clone();
    s.entries.retain(|e| match (committed.get(&e.path), epoch_secs(&e.timestamp)) {
        (Some(c), Some(t)) => t > *c,
        _ => true,
    });
    s
}

/// `2026-09-27T19:26:25.459Z` → seconds since the epoch. Transcripts write
/// UTC with a `Z`; anything else is ignored.
fn epoch_secs(ts: &str) -> Option<i64> {
    let (date, time) = ts.strip_suffix('Z')?.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.splitn(3, ':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss = t.next()?.split('.').next()?.parse::<i64>().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

pub fn group(files: &[ChangedFile], session: Option<&ParsedSession>, noise: &Noise) -> Vec<IntentGroup> {
    let turns: &[Turn] = session.map_or(&[], |s| &s.turns);
    let mut by_turn: Vec<Vec<GroupFile>> = vec![vec![]; turns.len()];
    let (mut unattributed, mut generated) = (vec![], vec![]);

    for f in files {
        let plain = |path: &str| GroupFile { path: path.to_string(), tools: vec![], also_turns: vec![] };
        if noise.matches(&f.path) {
            generated.push(plain(&f.path));
            continue;
        }
        // A renamed file counts under either name.
        let hits: Vec<_> = session
            .map(|s| {
                s.entries.iter().filter(|e| e.path == f.path || f.old_path.as_deref() == Some(e.path.as_str())).collect()
            })
            .unwrap_or_default();
        let Some(last) = hits.iter().map(|e| e.turn).max() else {
            unattributed.push(plain(&f.path));
            continue;
        };
        let also: BTreeSet<usize> = hits.iter().map(|e| e.turn).filter(|t| *t != last).collect();
        let tools: BTreeSet<EditTool> = hits.iter().filter(|e| e.turn == last).map(|e| e.tool).collect();
        by_turn[last - 1].push(GroupFile {
            path: f.path.clone(),
            tools: tools.into_iter().collect(),
            also_turns: also.into_iter().collect(),
        });
    }

    let mut groups: Vec<IntentGroup> = turns
        .iter()
        .zip(by_turn)
        .filter(|(_, files)| !files.is_empty())
        .map(|(t, files)| IntentGroup {
            id: format!("turn-{}", t.index),
            kind: GroupKind::Turn,
            title: short_title(&t.prompt),
            briefing: if t.summary.is_empty() {
                "Claude hasn't closed this turn with a message yet.".into()
            } else {
                t.summary.clone()
            },
            turn: Some(t.index),
            prompt: Some(t.prompt.clone()),
            files,
        })
        .collect();

    if !unattributed.is_empty() {
        let commands: usize = turns.iter().map(|t| t.commands.len()).sum();
        let briefing = match session {
            None => "No Claude Code session found for this repo, so none of these changes can be attributed.".into(),
            Some(_) if commands > 0 => format!(
                "No Edit or Write call in this session covers these files: your own edits, side effects of the {commands} Bash command{} Claude ran, or changes left by an earlier session.",
                if commands == 1 { "" } else { "s" }
            ),
            Some(_) => "No Edit or Write call in this session covers these files: your own edits, or changes left by an earlier session.".into(),
        };
        groups.push(IntentGroup {
            id: "unattributed".into(),
            kind: GroupKind::Unattributed,
            title: "UNATTRIBUTED".into(),
            briefing,
            turn: None,
            prompt: None,
            files: unattributed,
        });
    }
    if !generated.is_empty() {
        groups.push(IntentGroup {
            id: "generated".into(),
            kind: GroupKind::Generated,
            title: "GENERATED & LOCKFILES".into(),
            briefing: "Regenerated by tooling: lockfiles, codegen and build output. Usually safe to skim.".into(),
            turn: None,
            prompt: None,
            files: generated,
        });
    }
    groups
}

/// About six words of the prompt's first line, upper-cased.
pub fn short_title(prompt: &str) -> String {
    let line = prompt.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line = line.trim_start_matches(|c: char| matches!(c, '>' | '#' | '-' | '*') || c.is_whitespace());
    let words: Vec<&str> = line.split_whitespace().collect();
    let mut title = words.iter().take(TITLE_WORDS).copied().collect::<Vec<_>>().join(" ");
    title = title.trim_end_matches(|c: char| matches!(c, '.' | ',' | ':' | ';' | '!' | '?' | '…')).to_string();
    if words.len() > TITLE_WORDS {
        title.push('…');
    }
    if title.is_empty() {
        "UNTITLED TURN".into()
    } else {
        title.to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::FileStatus;
    use crate::transcript::parse::parse_session;
    use std::path::PathBuf;

    fn file(path: &str) -> ChangedFile {
        ChangedFile { path: path.into(), old_path: None, status: FileStatus::Modified, is_binary: false, adds: 1, dels: 0, hunks: 1 }
    }

    fn variants() -> ParsedSession {
        parse_session(
            include_str!("../tests/fixtures/session-variants.jsonl"),
            &[include_str!("../tests/fixtures/session-variants-agent.jsonl").to_string()],
            &[PathBuf::from("/Users/dev/proj")],
        )
    }

    fn summary(groups: &[IntentGroup]) -> Vec<(String, Vec<&str>)> {
        groups.iter().map(|g| (g.id.clone(), g.files.iter().map(|f| f.path.as_str()).collect())).collect()
    }

    #[test]
    fn groups_by_last_turn_then_unattributed_then_generated() {
        let s = variants();
        let files: Vec<_> =
            ["mine.ts", "nb.ipynb", "pnpm-lock.yaml", "src/a.ts", "src/c.ts", "src/sub.ts"].map(file).into();
        let groups = group(&files, Some(&s), &Noise::defaults().unwrap());
        assert_eq!(
            summary(&groups),
            [
                ("turn-1".to_string(), vec!["src/sub.ts"]),
                ("turn-2".to_string(), vec!["nb.ipynb", "src/a.ts", "src/c.ts"]),
                ("unattributed".to_string(), vec!["mine.ts"]),
                ("generated".to_string(), vec!["pnpm-lock.yaml"]),
            ]
        );
        let t2 = &groups[1];
        assert_eq!(t2.title, "SECOND PROMPT: TIDY TESTS");
        assert_eq!(t2.briefing, "Tests tidied.");
        let a = t2.files.iter().find(|f| f.path == "src/a.ts").unwrap();
        assert_eq!(a.also_turns, [1], "edited in turn 1 too, belongs to its last turn");
        assert_eq!(a.tools, [EditTool::Edit]);
        assert!(groups[2].briefing.contains("1 Bash command Claude ran"), "{}", groups[2].briefing);
    }

    #[test]
    fn renamed_file_matches_its_old_path() {
        let s = variants();
        let mut f = file("src/renamed.ts");
        f.old_path = Some("src/c.ts".into());
        let groups = group(&[f], Some(&s), &Noise::defaults().unwrap());
        assert_eq!(summary(&groups), [("turn-2".to_string(), vec!["src/renamed.ts"])]);
    }

    #[test]
    fn claude_written_noise_is_still_generated() {
        let s = variants();
        let groups = group(&[file("src/a.ts"), file("dist/a.js")], Some(&s), &Noise::defaults().unwrap());
        assert_eq!(groups.last().unwrap().id, "generated");
        assert_eq!(groups.last().unwrap().files[0].path, "dist/a.js");
    }

    #[test]
    fn without_a_session_everything_is_unattributed() {
        let groups = group(&[file("a.ts"), file("Cargo.lock")], None, &Noise::defaults().unwrap());
        assert_eq!(
            summary(&groups),
            [("unattributed".to_string(), vec!["a.ts"]), ("generated".to_string(), vec!["Cargo.lock"])]
        );
        assert!(groups[0].briefing.starts_with("No Claude Code session"));
    }

    #[test]
    fn epoch_secs_parses_transcript_timestamps() {
        assert_eq!(epoch_secs("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(epoch_secs("2026-09-27T19:26:25.459Z"), Some(1_790_537_185));
        assert_eq!(epoch_secs("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(epoch_secs("2026-09-27 19:26:25"), None);
    }

    /// Claude writes a.ts and b.ts in turn 1; a commit then takes both; b.ts
    /// changes again outside the ledger. b.ts is not turn 1's work any more.
    #[test]
    fn committed_edits_stop_counting() {
        use crate::git::fixture::Fixture;
        let fx = Fixture::new();
        fx.commit(&[("a.ts", "a\n"), ("b.ts", "b\n")]);
        let now = epoch_secs_now();
        let iso = |secs: i64| {
            // Only needs to order correctly against the commit time.
            let days = secs.div_euclid(86_400);
            let rem = secs.rem_euclid(86_400);
            let (y, m, d) = civil(days);
            format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
        };
        let entry = |path: &str, at: i64| crate::transcript::LedgerEntry {
            path: path.into(),
            tool: EditTool::Write,
            timestamp: iso(at),
            turn: 1,
        };
        let mut s = ParsedSession::default();
        s.turns.push(Turn { index: 1, prompt: "p".into(), summary: String::new(), files: vec![], timestamp: iso(now - 60), commands: vec![] });
        // a.ts edited again after the commit; b.ts only before it.
        s.entries = vec![entry("a.ts", now - 60), entry("b.ts", now - 60), entry("a.ts", now + 60)];

        let committed = committed_since(&fx.repo, &s).unwrap();
        let pending = drop_committed(&s, &committed);
        let paths: Vec<_> = pending.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["a.ts"]);

        let groups = group(&[file("a.ts"), file("b.ts")], Some(&pending), &Noise::defaults().unwrap());
        assert_eq!(summary(&groups), [("turn-1".into(), vec!["a.ts"]), ("unattributed".into(), vec!["b.ts"])]);
    }

    fn epoch_secs_now() -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
    }

    /// Inverse of the days-from-civil step in `epoch_secs`.
    fn civil(z: i64) -> (i64, i64, i64) {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (yoe + era * 400 + i64::from(m <= 2), m, d)
    }

    #[test]
    fn short_title_takes_about_six_words() {
        assert_eq!(short_title("> Implement SPEC §3.2–3.3. First look at a real transcript"), "IMPLEMENT SPEC §3.2–3.3. FIRST LOOK AT…");
        assert_eq!(short_title("fix the guard."), "FIX THE GUARD");
        assert_eq!(short_title("\n\n## Split realm auth\nmore"), "SPLIT REALM AUTH");
        assert_eq!(short_title("   "), "UNTITLED TURN");
    }
}

