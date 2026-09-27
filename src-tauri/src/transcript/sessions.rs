use super::parse::{parse_session, ParsedSession};
use super::{dir_matches, inside, slug};
use crate::error::AppResult;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    /// AI title, else the first prompt.
    pub title: String,
    pub first_prompt: Option<String>,
    pub cwd: String,
    pub started_at: Option<String>,
    /// Newest mtime across the transcript and its subagents, ms since epoch.
    pub updated_at: i64,
    pub last_edit_at: Option<String>,
    pub turns: usize,
    pub edits: usize,
    pub path: String,
}

impl SessionInfo {
    fn new(p: &ParsedSession, path: &Path, updated_at: i64) -> Self {
        let first_prompt = p.first_prompt().map(String::from);
        Self {
            id: p.id.clone(),
            title: p.title.clone().or_else(|| first_prompt.clone()).unwrap_or_else(|| p.id.clone()),
            first_prompt,
            cwd: p.cwd.clone().unwrap_or_default(),
            started_at: p.started_at.clone(),
            updated_at,
            last_edit_at: p.last_edit_at().map(String::from),
            turns: p.turns.len(),
            edits: p.entries.len(),
            path: path.to_string_lossy().to_string(),
        }
    }
}

/// (path, len, mtime ms) of each file a session is parsed from.
type Stamp = Vec<(PathBuf, u64, i64)>;

struct Cached {
    stamp: Stamp,
    roots: Vec<PathBuf>,
    parsed: Arc<ParsedSession>,
}

/// Parsed transcripts, reused until one of their files changes. A live
/// session's transcript grows with every tool call, and the watcher asks for
/// a new model each time.
pub struct TranscriptCache {
    map: Mutex<HashMap<PathBuf, Cached>>,
}

impl TranscriptCache {
    pub fn new() -> Self {
        Self { map: Mutex::new(HashMap::new()) }
    }

    pub fn load(&self, main: &Path, roots: &[PathBuf]) -> AppResult<(Arc<ParsedSession>, i64)> {
        let mut files = vec![main.to_path_buf()];
        files.extend(subagent_files(main));
        let stamp = stamp(&files);
        let updated = stamp.iter().map(|s| s.2).max().unwrap_or(0);
        if let Some(c) = self.map.lock().get(main) {
            if c.stamp == stamp && c.roots == roots {
                return Ok((c.parsed.clone(), updated));
            }
        }
        let main_text = read_lossy(main)?;
        let agents = files[1..].iter().filter_map(|f| read_lossy(f).ok()).collect::<Vec<_>>();
        let parsed = Arc::new(parse_session(&main_text, &agents, roots));
        self.map.lock().insert(
            main.to_path_buf(),
            Cached { stamp, roots: roots.to_vec(), parsed: parsed.clone() },
        );
        Ok((parsed, updated))
    }
}

/// Sessions for the repo, newest first: started in the repo root or below
/// it, with at least one prompt or edit.
pub fn list_sessions(
    cache: &TranscriptCache,
    projects: &Path,
    roots: &[PathBuf],
) -> AppResult<Vec<(SessionInfo, Arc<ParsedSession>)>> {
    let slugs: Vec<String> = roots.iter().map(|r| slug(r)).collect();
    let Ok(dirs) = std::fs::read_dir(projects) else { return Ok(vec![]) };
    let mut out = Vec::new();
    for dir in dirs.flatten() {
        let name = dir.file_name().to_string_lossy().to_string();
        if !dir_matches(&name, &slugs) || !dir.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok((parsed, updated)) = cache.load(&path, roots) else { continue };
            let in_repo = parsed.cwd.as_deref().is_some_and(|c| inside(Path::new(c), roots));
            if !in_repo || (parsed.turns.is_empty() && parsed.entries.is_empty()) {
                continue;
            }
            out.push((SessionInfo::new(&parsed, &path, updated), parsed));
        }
    }
    out.sort_by(|a, b| b.0.updated_at.cmp(&a.0.updated_at).then_with(|| a.0.id.cmp(&b.0.id)));
    Ok(out)
}

/// `<dir>/<session-id>/subagents/*.jsonl`, sorted.
fn subagent_files(main: &Path) -> Vec<PathBuf> {
    let dir = main.with_extension("").join("subagents");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    files
}

fn stamp(files: &[PathBuf]) -> Stamp {
    files
        .iter()
        .filter_map(|f| {
            let m = std::fs::metadata(f).ok()?;
            let ms = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis();
            Some((f.clone(), m.len(), i64::try_from(ms).ok()?))
        })
        .collect()
}

fn read_lossy(p: &Path) -> AppResult<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(p)?).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn session(id: &str, cwd: &str, prompt: &str) -> String {
        [
            format!(r#"{{"type":"user","sessionId":"{id}","cwd":"{cwd}","timestamp":"2026-09-01T10:00:00Z","origin":{{"kind":"human"}},"message":{{"role":"user","content":"{prompt}"}}}}"#),
            format!(r#"{{"type":"assistant","sessionId":"{id}","cwd":"{cwd}","timestamp":"2026-09-01T10:00:01Z","message":{{"content":[{{"type":"tool_use","id":"t-{id}","name":"Write","input":{{"file_path":"{cwd}/a.ts"}}}}]}}}}"#),
        ]
        .join("\n")
    }

    fn write(path: &Path, body: &str, age_secs: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
        let t = SystemTime::now() - Duration::from_secs(age_secs);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn lists_repo_sessions_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path();
        let roots = vec![PathBuf::from("/w/enna")];
        write(&projects.join("-w-enna/old.jsonl"), &session("old", "/w/enna", "first"), 300);
        write(&projects.join("-w-enna/new.jsonl"), &session("new", "/w/enna", "second"), 10);
        // Started in a subdirectory of the repo: included.
        write(&projects.join("-w-enna-src/sub.jsonl"), &session("sub", "/w/enna/src", "third"), 100);
        // Prefix collision with another repo: dropped by the cwd check.
        write(&projects.join("-w-enna-live/live.jsonl"), &session("live", "/w/enna-live", "x"), 1);
        // Unrelated project and a metadata-only session: dropped.
        write(&projects.join("-w-other/o.jsonl"), &session("o", "/w/other", "x"), 1);
        write(&projects.join("-w-enna/empty.jsonl"), r#"{"type":"mode","sessionId":"empty"}"#, 1);

        let cache = TranscriptCache::new();
        let ids: Vec<_> = list_sessions(&cache, projects, &roots).unwrap().into_iter().map(|s| s.0.id).collect();
        assert_eq!(ids, ["new", "sub", "old"]);
    }

    #[test]
    fn title_falls_back_to_first_prompt_and_counts_edits() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("-w-r/s.jsonl"), &session("s", "/w/r", "fix the guard"), 1);
        let cache = TranscriptCache::new();
        let (info, _) = list_sessions(&cache, tmp.path(), &[PathBuf::from("/w/r")]).unwrap().remove(0);
        assert_eq!(info.title, "fix the guard");
        assert_eq!((info.turns, info.edits), (1, 1));
        assert_eq!(info.last_edit_at.as_deref(), Some("2026-09-01T10:00:01Z"));
    }

    #[test]
    fn subagent_files_are_parsed_and_invalidate_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("-w-r/s.jsonl");
        write(&main, &session("s", "/w/r", "go"), 5);
        let roots = vec![PathBuf::from("/w/r")];
        let cache = TranscriptCache::new();
        assert_eq!(cache.load(&main, &roots).unwrap().0.entries.len(), 1);

        let agent = r#"{"type":"assistant","isSidechain":true,"cwd":"/w/r","timestamp":"2026-09-01T10:00:05Z","message":{"content":[{"type":"tool_use","id":"sa","name":"Edit","input":{"file_path":"/w/r/b.ts"}}]}}"#;
        write(&tmp.path().join("-w-r/s/subagents/agent-1.jsonl"), agent, 0);
        let (parsed, _) = cache.load(&main, &roots).unwrap();
        assert_eq!(parsed.turns[0].files, ["a.ts", "b.ts"]);
    }

    #[test]
    fn missing_projects_dir_is_empty() {
        let cache = TranscriptCache::new();
        assert!(list_sessions(&cache, Path::new("/nonexistent/debrief"), &[]).unwrap().is_empty());
    }
}
