use super::{clip, repo_rel, EditTool, Ledger, LedgerEntry, Turn};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const PROMPT_CHARS: usize = 280;
const SUMMARY_CHARS: usize = 600;
const COMMAND_CHARS: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct ParsedSession {
    pub id: String,
    /// `ai-title`, else an old-style `summary` line.
    pub title: Option<String>,
    /// The cwd of the first line that has one.
    pub cwd: Option<String>,
    pub started_at: Option<String>,
    pub turns: Vec<Turn>,
    /// Successful edits inside the repo, oldest first.
    pub entries: Vec<LedgerEntry>,
}

impl ParsedSession {
    pub fn first_prompt(&self) -> Option<&str> {
        self.turns.first().map(|t| t.prompt.as_str())
    }

    pub fn last_edit_at(&self) -> Option<&str> {
        self.entries.iter().map(|e| e.timestamp.as_str()).max()
    }

    pub fn ledger(&self) -> Ledger {
        Ledger { turns: self.turns.clone(), entries: self.entries.clone() }
    }
}

struct PendingEdit {
    id: String,
    tool: EditTool,
    file: String,
    cwd: PathBuf,
    timestamp: String,
    turn: usize,
    branch: Option<String>,
}

#[derive(Default)]
struct Collector {
    edits: Vec<PendingEdit>,
    failed: HashSet<String>,
}

impl Collector {
    /// Record edit calls in an assistant line; return its Bash commands.
    fn tool_uses(&mut self, v: &Value, turn: usize, cwd: &Path, ts: &str) -> Vec<String> {
        let mut commands = Vec::new();
        for b in blocks(v).filter(|b| b["type"] == "tool_use") {
            let name = b["name"].as_str().unwrap_or_default();
            let input = &b["input"];
            if name == "Bash" {
                if let Some(cmd) = input["command"].as_str() {
                    commands.push(clip(cmd.trim(), COMMAND_CHARS));
                }
                continue;
            }
            let Some(tool) = EditTool::from_name(name) else { continue };
            let Some(file) = input["file_path"].as_str().or_else(|| input["notebook_path"].as_str()) else {
                continue;
            };
            self.edits.push(PendingEdit {
                id: b["id"].as_str().unwrap_or_default().to_string(),
                tool,
                file: file.to_string(),
                cwd: cwd.to_path_buf(),
                timestamp: ts.to_string(),
                turn,
                branch: git_branch(v),
            });
        }
        commands
    }

    fn tool_results(&mut self, v: &Value) {
        for b in blocks(v).filter(|b| b["type"] == "tool_result" && b["is_error"] == true) {
            if let Some(id) = b["tool_use_id"].as_str() {
                self.failed.insert(id.to_string());
            }
        }
    }
}

/// Parse a session transcript and its subagent transcripts. Malformed lines
/// are skipped, including a half-written last line from a live session.
pub fn parse_session(main: &str, subagents: &[String], roots: &[PathBuf]) -> ParsedSession {
    let mut s = ParsedSession::default();
    let (mut ai_title, mut summary_title) = (None, None);
    let mut c = Collector::default();
    let mut cwd = PathBuf::new();

    for v in lines(main) {
        if s.id.is_empty() {
            if let Some(id) = v["sessionId"].as_str() {
                s.id = id.to_string();
            }
        }
        if let Some(dir) = v["cwd"].as_str() {
            s.cwd.get_or_insert_with(|| dir.to_string());
            cwd = PathBuf::from(dir);
        }
        let ts = v["timestamp"].as_str().unwrap_or_default();
        if s.started_at.is_none() && !ts.is_empty() {
            s.started_at = Some(ts.to_string());
        }
        let sidechain = v["isSidechain"] == true;

        match v["type"].as_str() {
            Some("ai-title") => ai_title = v["aiTitle"].as_str().map(|t| t.trim().to_string()),
            Some("summary") => summary_title = v["summary"].as_str().map(|t| t.trim().to_string()),
            Some("user") => {
                if !sidechain {
                    if let Some(prompt) = human_prompt(&v) {
                        s.turns.push(Turn {
                            index: s.turns.len() + 1,
                            prompt: clip(&prompt, PROMPT_CHARS),
                            summary: String::new(),
                            files: vec![],
                            timestamp: ts.to_string(),
                            commands: vec![],
                            branch: git_branch(&v),
                        });
                    }
                }
                c.tool_results(&v);
            }
            Some("assistant") => {
                let turn = s.turns.len();
                let commands = c.tool_uses(&v, turn, &cwd, ts);
                if let Some(t) = s.turns.last_mut() {
                    t.commands.extend(commands);
                    if !sidechain {
                        for b in blocks(&v).filter(|b| b["type"] == "text") {
                            let text = b["text"].as_str().unwrap_or_default().trim();
                            if !text.is_empty() {
                                t.summary = clip(text, SUMMARY_CHARS);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    s.title = ai_title.or(summary_title).filter(|t| !t.is_empty());

    // Subagents: attribute by time to the parent turn running at that moment.
    for text in subagents {
        let mut cwd = PathBuf::from(s.cwd.clone().unwrap_or_default());
        for v in lines(text) {
            if let Some(dir) = v["cwd"].as_str() {
                cwd = PathBuf::from(dir);
            }
            let ts = v["timestamp"].as_str().unwrap_or_default();
            match v["type"].as_str() {
                Some("assistant") => {
                    let turn = turn_at(&s.turns, ts);
                    let commands = c.tool_uses(&v, turn, &cwd, ts);
                    if turn > 0 {
                        s.turns[turn - 1].commands.extend(commands);
                    }
                }
                Some("user") => c.tool_results(&v),
                _ => {}
            }
        }
    }

    // Edits made before the first prompt (a resumed session, say) go to turn 1.
    let edits: Vec<_> = c.edits.into_iter().filter(|e| !c.failed.contains(&e.id)).collect();
    if s.turns.is_empty() && !edits.is_empty() {
        s.turns.push(Turn {
            index: 1,
            prompt: "(no prompt recorded)".into(),
            summary: String::new(),
            files: vec![],
            timestamp: s.started_at.clone().unwrap_or_default(),
            commands: vec![],
            branch: None,
        });
    }
    for e in edits {
        let Some(path) = repo_rel(&e.file, &e.cwd, roots) else { continue };
        s.entries.push(LedgerEntry { path, tool: e.tool, timestamp: e.timestamp, turn: e.turn.max(1), branch: e.branch });
    }
    s.entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    for e in &s.entries {
        let files = &mut s.turns[e.turn - 1].files;
        if !files.contains(&e.path) {
            files.push(e.path.clone());
        }
    }
    s
}

fn lines(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(Value::is_object)
}

/// The checked-out branch Claude Code recorded on this line. `HEAD` means
/// detached (or no commits yet), which names no branch.
fn git_branch(v: &Value) -> Option<String> {
    v["gitBranch"].as_str().filter(|b| !b.is_empty() && *b != "HEAD").map(String::from)
}

fn blocks(v: &Value) -> impl Iterator<Item = &Value> {
    v["message"]["content"].as_array().into_iter().flatten()
}

/// 1-based index of the last turn that started at or before `ts`; 0 if none.
/// Timestamps are ISO-8601 UTC, so they order as strings.
fn turn_at(turns: &[Turn], ts: &str) -> usize {
    turns.iter().rposition(|t| t.timestamp.as_str() <= ts).map_or(0, |i| i + 1)
}

/// The prompt text when `v` is something the human typed.
fn human_prompt(v: &Value) -> Option<String> {
    if v["isMeta"] == true {
        return None;
    }
    let text = match &v["message"]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(bs) => {
            if bs.iter().any(|b| b["type"] == "tool_result") {
                return None;
            }
            bs.iter().filter_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()).collect::<Vec<_>>().join("\n")
        }
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let human = match v["origin"]["kind"].as_str() {
        Some(kind) => kind == "human",
        // Older transcripts: wrapped command output and interrupt markers
        // are user lines too.
        None => !text.starts_with('<') && !text.starts_with("[Request interrupted"),
    };
    human.then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../../tests/fixtures/session-real.jsonl");
    const SYNTH: &str = include_str!("../../tests/fixtures/session-variants.jsonl");
    const SYNTH_AGENT: &str = include_str!("../../tests/fixtures/session-variants-agent.jsonl");

    fn roots(p: &str) -> Vec<PathBuf> {
        vec![PathBuf::from(p)]
    }

    fn paths(s: &ParsedSession, turn: usize) -> Vec<&str> {
        s.turns[turn - 1].files.iter().map(String::as_str).collect()
    }

    /// A redacted transcript of the session that built M1 (see
    /// scripts/redact-transcript.py).
    #[test]
    fn real_transcript_turns_title_and_edits() {
        let s = parse_session(REAL, &[], &roots("/Users/dev/Workspace/Personal/debrief"));
        assert_eq!(s.id, "8121a8ef-3b71-41c6-9e91-882f3ca16198");
        assert_eq!(s.title.as_deref(), Some("Redacted session title"));
        assert_eq!(s.cwd.as_deref(), Some("/Users/dev/Workspace/Personal/debrief"));

        // Two typed prompts; tool results and notifications don't start turns.
        assert_eq!(s.turns.len(), 2);
        assert_eq!(s.turns[0].prompt, "[prompt 1]");
        assert_eq!(s.turns[1].prompt, "[prompt 2]");

        // M1 wrote these with Write/Edit…
        let t1 = paths(&s, 1);
        for p in ["src-tauri/src/lib.rs", "src-tauri/src/git/diff.rs", "src/App.tsx", "src/panels/DiffPanel.tsx"] {
            assert!(t1.contains(&p), "{p} missing from turn 1: {t1:?}");
        }
        // …and these only through Bash, so they are not in the ledger.
        for p in ["tailwind.config.js", "package.json", "src/styles/hud.css"] {
            assert!(!t1.contains(&p), "{p} came from Bash, not an edit tool");
        }
        // Reads of ../git-ui and writes to the scratchpad are outside the repo.
        assert!(s.entries.iter().all(|e| !e.path.starts_with("..") && !e.path.starts_with('/')));
        // The fixture was captured early in turn 2, after its first Write.
        assert_eq!(paths(&s, 2), ["scripts/redact-transcript.py"]);
        assert!(s.turns[0].commands.len() > 10, "Bash commands are recorded");

        // The summary is the turn's last assistant text block.
        let last_text = REAL
            .lines()
            .take_while(|l| !l.contains("\"[prompt 2]\""))
            .filter_map(|l| l.split("\"text\": \"[text ").nth(1))
            .last()
            .and_then(|rest| rest.split(']').next())
            .unwrap();
        assert_eq!(s.turns[0].summary, format!("[text {last_text}]"));
    }

    #[test]
    fn real_transcript_edit_tools_are_named() {
        let s = parse_session(REAL, &[], &roots("/Users/dev/Workspace/Personal/debrief"));
        let tool = |p: &str| s.entries.iter().find(|e| e.path == p).map(|e| e.tool);
        assert_eq!(tool("src-tauri/src/lib.rs"), Some(EditTool::Write));
        assert!(s.entries.iter().any(|e| e.tool == EditTool::Edit));
    }

    /// A real session (redacted) that started on main and switched to a
    /// feature branch halfway: every edit carries the branch it was made on.
    #[test]
    fn real_branch_session_records_the_branch_per_edit() {
        const BRANCHED: &str = include_str!("../../tests/fixtures/session-branch-real.jsonl");
        let s = parse_session(BRANCHED, &[], &roots("/Users/dev/Workspace/Personal/git-ui"));
        let on = |b: &str| s.entries.iter().filter(|e| e.branch.as_deref() == Some(b)).count();
        assert!(on("main") > 0 && on("feat/diff-panel-navigation") > 0, "{:?}", s.entries.iter().map(|e| &e.branch).collect::<Vec<_>>());
        assert_eq!(on("main") + on("feat/diff-panel-navigation"), s.entries.len(), "no edit without a branch");
        // Once on the feature branch, it never went back.
        let first_feat = s.entries.iter().position(|e| e.branch.as_deref() == Some("feat/diff-panel-navigation")).unwrap();
        assert!(s.entries[first_feat..].iter().all(|e| e.branch.as_deref() == Some("feat/diff-panel-navigation")));
        let switch_turn = s.turns.iter().position(|t| t.branch.as_deref() == Some("feat/diff-panel-navigation")).unwrap();
        assert!(s.turns[..switch_turn].iter().all(|t| t.branch.as_deref() == Some("main")));
        assert!(s.entries.iter().all(|e| !e.path.starts_with('/') && !e.path.contains("outside-repo")));
    }

    #[test]
    fn head_is_not_a_branch() {
        let line = r#"{"type":"assistant","gitBranch":"HEAD","cwd":"/p","timestamp":"2026-01-01T00:00:00Z","message":{"content":[{"type":"tool_use","id":"t","name":"Write","input":{"file_path":"/p/a.ts"}}]}}"#;
        let s = parse_session(line, &[], &roots("/p"));
        assert_eq!(s.entries[0].branch, None);
    }

    #[test]
    fn variants_prompts_and_title() {
        let s = parse_session(SYNTH, &[SYNTH_AGENT.to_string()], &roots("/Users/dev/proj"));
        assert_eq!(s.id, "s-synth");
        assert_eq!(s.title.as_deref(), Some("Old style summary"), "summary line is the fallback title");
        // bash-input, task-notification, interrupt and isMeta lines are not prompts.
        assert_eq!(s.turns.len(), 2, "{:#?}", s.turns);
        assert_eq!(s.turns[0].prompt, "Add realm auth\nwith details");
        assert_eq!(s.turns[1].prompt, "Second prompt: tidy tests");
        assert_eq!(s.turns[0].summary, "Done with realm auth.", "subagent text never becomes the summary");
        assert_eq!(s.turns[1].summary, "Tests tidied.");
        assert_eq!(s.turns[0].commands, ["pnpm install"]);
    }

    #[test]
    fn variants_ledger() {
        let s = parse_session(SYNTH, &[SYNTH_AGENT.to_string()], &roots("/Users/dev/proj"));
        // Failed edits (main and subagent) and paths outside the repo are dropped;
        // the subagent's edit lands in the turn that was running.
        assert_eq!(paths(&s, 1), ["src/a.ts", "src/sub.ts"]);
        // Relative paths resolve against the line's cwd, including `..`.
        assert_eq!(paths(&s, 2), ["tests/x.test.ts", "nb.ipynb", "src/a.ts", "src/c.ts"]);
        let tools: Vec<_> = s.entries.iter().map(|e| (e.path.as_str(), e.tool, e.turn)).collect();
        assert!(tools.contains(&("tests/x.test.ts", EditTool::MultiEdit, 2)));
        assert!(tools.contains(&("nb.ipynb", EditTool::NotebookEdit, 2)));
        assert!(!tools.iter().any(|t| t.0 == "src/b.ts" || t.0.contains("elsewhere") || t.0 == "src/sub-failed.ts"));
        assert_eq!(s.last_edit_at(), Some("2026-09-01T10:01:05.000Z"));
    }

    #[test]
    fn edits_before_any_prompt_get_a_placeholder_turn() {
        let text = r#"{"type":"assistant","sessionId":"x","cwd":"/p","timestamp":"2026-01-01T00:00:00Z","message":{"content":[{"type":"tool_use","id":"t","name":"Write","input":{"file_path":"/p/a.ts"}}]}}"#;
        let s = parse_session(text, &[], &roots("/p"));
        assert_eq!(s.turns.len(), 1);
        assert_eq!(s.turns[0].files, ["a.ts"]);
    }

    #[test]
    fn garbage_is_ignored() {
        let s = parse_session("not json\n[1,2]\n\n{\"type\":", &[], &roots("/p"));
        assert!(s.turns.is_empty() && s.entries.is_empty() && s.id.is_empty());
    }
}
