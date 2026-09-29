//! Claude Code transcripts (SPEC §3.2). Transcripts are data: nothing in
//! them is ever executed.
//!
//! Checked against real transcripts (Claude Code 2.1.x). Where they differ
//! from what the SPEC expected:
//! - A human prompt is a `user` line with `origin.kind == "human"`. Other
//!   user lines are tool results, task notifications, `!` bash echoes and
//!   interrupt markers. Older lines without `origin` fall back to a heuristic.
//! - The title comes from `ai-title` lines (`aiTitle`); `summary` lines are
//!   an older format and only used when there is no AI title.
//! - Subagents write their own transcripts to
//!   `<session-id>/subagents/agent-*.jsonl`; their edits belong to the
//!   parent turn that was running at the time.
//! - A failed or denied tool call gets a `tool_result` with `is_error: true`,
//!   so it doesn't count as an edit.
//! - The project-dir slug replaces every non-alphanumeric char with `-`, not
//!   just `/` (`.claude/worktrees` becomes `-claude-worktrees`).

pub mod parse;
pub mod sessions;

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditTool {
    Edit,
    MultiEdit,
    Write,
    NotebookEdit,
}

impl EditTool {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Edit" => Some(Self::Edit),
            "MultiEdit" => Some(Self::MultiEdit),
            "Write" => Some(Self::Write),
            "NotebookEdit" => Some(Self::NotebookEdit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    /// Repo-relative, `/`-separated.
    pub path: String,
    pub tool: EditTool,
    pub timestamp: String,
    /// 1-based index of the turn this edit belongs to.
    pub turn: usize,
    /// Branch checked out when the edit was made (`gitBranch`); None when
    /// detached or not recorded.
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    /// 1-based, in prompt order.
    pub index: usize,
    /// The user message that started the turn (first 280 chars).
    pub prompt: String,
    /// The last assistant text block of the turn (first 600 chars).
    pub summary: String,
    /// Files edited in this turn, in first-edit order.
    pub files: Vec<String>,
    pub timestamp: String,
    /// Bash commands run in this turn, recorded (never run) because they may
    /// write files we can't attribute.
    pub commands: Vec<String>,
    /// Branch checked out when the prompt was sent.
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    pub turns: Vec<Turn>,
    pub entries: Vec<LedgerEntry>,
}

/// `$CLAUDE_CONFIG_DIR/projects`, else `~/.claude/projects`.
pub fn projects_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("projects"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude").join("projects"))
}

/// The directory name Claude Code uses for a cwd.
pub fn slug(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Sessions started in the repo or any directory below it. The prefix match
/// over-selects (`enna` also matches `enna-live`), so callers confirm with
/// the session's cwd.
pub fn dir_matches(name: &str, slugs: &[String]) -> bool {
    slugs.iter().any(|s| name == s || name.strip_prefix(s.as_str()).is_some_and(|rest| rest.starts_with('-')))
}

/// The repo root as written, plus its canonical form (`/tmp` vs
/// `/private/tmp`), so paths recorded either way resolve.
pub fn repo_roots(workdir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![normalize(workdir)];
    if let Ok(c) = workdir.canonicalize() {
        let c = normalize(&c);
        if !roots.contains(&c) {
            roots.push(c);
        }
    }
    roots
}

/// Resolve `.` and `..` without touching the filesystem; also drops a
/// trailing slash.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn inside(path: &Path, roots: &[PathBuf]) -> bool {
    let p = normalize(path);
    roots.iter().any(|r| p.starts_with(r))
}

/// A tool's `file_path` as a repo-relative path, or None when it points
/// outside the repo.
pub fn repo_rel(file: &str, cwd: &Path, roots: &[PathBuf]) -> Option<String> {
    let p = Path::new(file);
    let abs = normalize(&if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) });
    roots.iter().find_map(|r| {
        let rel = abs.strip_prefix(r).ok()?.to_string_lossy().replace('\\', "/");
        (!rel.is_empty()).then_some(rel)
    })
}

/// First `n` chars, with an ellipsis when cut.
pub fn clip(s: &str, n: usize) -> String {
    let mut it = s.chars();
    let head: String = it.by_ref().take(n).collect();
    if it.next().is_some() {
        format!("{}…", head.trim_end())
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_matches_claude_code() {
        assert_eq!(slug(Path::new("/Users/dev/Workspace/Personal/debrief")), "-Users-dev-Workspace-Personal-debrief");
        assert_eq!(
            slug(Path::new("/Users/dev/w/repo/.claude/worktrees/fix-x")),
            "-Users-dev-w-repo--claude-worktrees-fix-x"
        );
    }

    #[test]
    fn dir_match_is_exact_or_a_dash_prefix() {
        let slugs = vec!["-u-enna".to_string()];
        assert!(dir_matches("-u-enna", &slugs));
        assert!(dir_matches("-u-enna-src", &slugs));
        assert!(dir_matches("-u-enna-live", &slugs), "over-selects; cwd check filters it");
        assert!(!dir_matches("-u-ennab", &slugs));
        assert!(!dir_matches("-u-enn", &slugs));
    }

    #[test]
    fn repo_rel_resolves_relative_and_dotdot_paths() {
        let roots = vec![PathBuf::from("/r/repo")];
        let cwd = Path::new("/r/repo/src-tauri");
        assert_eq!(repo_rel("/r/repo/src/a.ts", cwd, &roots).as_deref(), Some("src/a.ts"));
        assert_eq!(repo_rel("src/lib.rs", cwd, &roots).as_deref(), Some("src-tauri/src/lib.rs"));
        assert_eq!(repo_rel("../README.md", cwd, &roots).as_deref(), Some("README.md"));
        assert_eq!(repo_rel("/r/other/a.ts", cwd, &roots), None);
        assert_eq!(repo_rel("/r/repo-two/a.ts", cwd, &roots), None, "not a path prefix");
        assert_eq!(repo_rel("/r/repo", cwd, &roots), None);
    }

    #[test]
    fn clip_counts_chars_not_bytes() {
        assert_eq!(clip("äöü", 5), "äöü");
        assert_eq!(clip("äöü ß", 3), "äöü…");
    }
}
