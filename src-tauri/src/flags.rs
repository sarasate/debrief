//! "Needs a closer look" flags (SPEC §3.5). Each rule returns a short
//! reason. Secret matches name the pattern and the line, never the value.
//!
//! Noise files skip the rules about review scope (1, 2, 3, 6) but still get
//! the safety rules (4 secrets, 5 conflict markers).

use crate::git::status::ContentScan;
use crate::git::types::{ChangedFile, FileStatus};
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

pub const LARGE_CHANGE_LINES: usize = 400;
pub const TEST_SHRINK_RATIO: f64 = 0.30;
/// Line numbers listed per reason before "+n more".
const MAX_LINES_SHOWN: usize = 3;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum FlagKind {
    Unattributed,
    Marker,
    TestRemoval,
    Secret,
    Conflict,
    Large,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Flag {
    pub kind: FlagKind,
    pub reason: String,
    /// First line the reason points at, for jumping to it.
    pub line: Option<u32>,
}

/// Everything the rules look at for one file.
pub struct FileFacts<'a> {
    pub file: &'a ChangedFile,
    pub scan: Option<&'a ContentScan>,
    pub noise: bool,
    /// No ledger entry covers the file, and a session exists to have one.
    pub unattributed: bool,
    /// Line count of the HEAD version, for the test-shrink rule.
    pub old_lines: Option<usize>,
}

pub fn evaluate(f: &FileFacts) -> Vec<Flag> {
    let added: &[(u32, String)] = f.scan.map_or(&[], |s| &s.added);
    let mut out = Vec::new();
    if !f.noise {
        out.extend(unattributed(f));
        out.extend(markers(added));
        out.extend(test_removal(f));
    }
    out.extend(env_file(f.file));
    out.extend(secrets(added));
    out.extend(conflicts(f.file, added));
    if !f.noise {
        out.extend(large(f.file));
    }
    out
}

// 1 ─────────────────────────────────────────────────────────────────────

fn unattributed(f: &FileFacts) -> Option<Flag> {
    f.unattributed.then(|| Flag {
        kind: FlagKind::Unattributed,
        reason: "Not in the session ledger: no Edit or Write call from Claude covers this change.".into(),
        line: None,
    })
}

// 2 ─────────────────────────────────────────────────────────────────────

static MARKERS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("TODO", r"\bTODO\b"),
        ("FIXME", r"\bFIXME\b"),
        ("XXX", r"\bXXX\b"),
        ("@ts-ignore", r"@ts-ignore\b"),
        ("eslint-disable", r"eslint-disable"),
        ("as any", r"\bas\s+any\b"),
        (".only(", r"\.only\("),
        (".skip(", r"\.skip\("),
    ]
    .into_iter()
    .map(|(n, p)| (n, Regex::new(p).expect("static regex")))
    .collect()
});

fn markers(added: &[(u32, String)]) -> Option<Flag> {
    let (list, first) = describe(&hits_by_pattern(added, &MARKERS, |_| true))?;
    Some(Flag { kind: FlagKind::Marker, reason: format!("Adds {list}."), line: Some(first) })
}

// 3 ─────────────────────────────────────────────────────────────────────

pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let in_test_dir = lower
        .split('/')
        .rev()
        .skip(1)
        .any(|d| matches!(d, "test" | "tests" | "__tests__" | "spec" | "e2e"));
    let stem = name.split('.').next().unwrap_or(name);
    in_test_dir
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains(".e2e-spec.")
        || stem.ends_with("_test")
        || stem.starts_with("test_")
}

fn test_removal(f: &FileFacts) -> Option<Flag> {
    if !is_test_path(&f.file.path) {
        return None;
    }
    if f.file.status == FileStatus::Deleted {
        return Some(Flag { kind: FlagKind::TestRemoval, reason: "Deletes a test file.".into(), line: None });
    }
    let old = f.old_lines.filter(|n| *n > 0)?;
    let ratio = f.file.dels as f64 / old as f64;
    (ratio > TEST_SHRINK_RATIO).then(|| Flag {
        kind: FlagKind::TestRemoval,
        reason: format!(
            "Removes {:.0}% of this test file ({} of {} lines).",
            (ratio * 100.0).min(100.0),
            f.file.dels,
            old
        ),
        line: None,
    })
}

// 4 ─────────────────────────────────────────────────────────────────────

fn env_file(file: &ChangedFile) -> Option<Flag> {
    let name = file.path.rsplit('/').next().unwrap_or(&file.path);
    (name == ".env" || name.starts_with(".env.")).then(|| Flag {
        kind: FlagKind::Secret,
        reason: "Environment file changed: check new variables exist wherever this runs, and that no real values are committed.".into(),
        line: None,
    })
}

static SECRETS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("AWS access key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        ("PEM block", r"-----BEGIN [A-Z0-9 ]+-----"),
        ("sk- API key", r"\bsk-[A-Za-z0-9_-]{20,}"),
        (
            "credential assignment",
            r#"(?i)\b[A-Z0-9_]*(?:KEY|SECRET|TOKEN)\s*[=:]\s*["']?([A-Za-z0-9+/_=-]{24,})"#,
        ),
    ]
    .into_iter()
    .map(|(n, p)| (n, Regex::new(p).expect("static regex")))
    .collect()
});

/// A credential-looking value, not a placeholder or a variable reference:
/// mixes letters and digits.
fn looks_random(v: &str) -> bool {
    v.chars().any(|c| c.is_ascii_digit()) && v.chars().any(|c| c.is_ascii_alphabetic())
}

fn secrets(added: &[(u32, String)]) -> Option<Flag> {
    // Patterns with a capture group check that the value looks random.
    let hits = hits_by_pattern(added, &SECRETS, |c| c.get(1).is_none_or(|v| looks_random(v.as_str())));
    let (list, first) = describe(&hits)?;
    Some(Flag {
        kind: FlagKind::Secret,
        reason: format!("Possible secret: {list}. Value not shown."),
        line: Some(first),
    })
}

// 5 ─────────────────────────────────────────────────────────────────────

fn conflicts(file: &ChangedFile, added: &[(u32, String)]) -> Option<Flag> {
    let lines: Vec<u32> = added
        .iter()
        .filter(|(_, t)| t.starts_with("<<<<<<< ") || t.starts_with(">>>>>>> ") || t == "<<<<<<<" || t == ">>>>>>>")
        .map(|(n, _)| *n)
        .collect();
    if let Some(first) = lines.first() {
        return Some(Flag {
            kind: FlagKind::Conflict,
            reason: format!("Conflict markers ({}).", lines_label(&lines)),
            line: Some(*first),
        });
    }
    (file.status == FileStatus::Conflicted).then(|| Flag {
        kind: FlagKind::Conflict,
        reason: "Unresolved merge conflict in the index.".into(),
        line: None,
    })
}

// 6 ─────────────────────────────────────────────────────────────────────

fn large(file: &ChangedFile) -> Option<Flag> {
    let n = file.adds + file.dels;
    (n > LARGE_CHANGE_LINES).then(|| Flag {
        kind: FlagKind::Large,
        reason: format!("Large change: {n} changed lines (+{} −{}).", file.adds, file.dels),
        line: None,
    })
}

// ───────────────────────────────────────────────────────────────────────

/// Lines matching each pattern, in pattern-table order; misses left out.
fn hits_by_pattern(
    added: &[(u32, String)],
    patterns: &[(&'static str, Regex)],
    accept: impl Fn(&regex::Captures) -> bool,
) -> Vec<(&'static str, Vec<u32>)> {
    patterns
        .iter()
        .map(|(name, re)| {
            let lines = added
                .iter()
                .filter(|(_, text)| re.captures(text).is_some_and(|c| accept(&c)))
                .map(|(n, _)| *n)
                .collect::<Vec<_>>();
            (*name, lines)
        })
        .filter(|(_, lines)| !lines.is_empty())
        .collect()
}

/// "Adds TODO (line 1), as any (lines 2, 9)" style list, plus the first line.
fn describe(hits: &[(&str, Vec<u32>)]) -> Option<(String, u32)> {
    let first = hits.iter().flat_map(|(_, l)| l).min().copied()?;
    let parts: Vec<String> = hits.iter().map(|(name, lines)| format!("{name} ({})", lines_label(lines))).collect();
    Some((parts.join(", "), first))
}

/// "line 4", "lines 3, 9, 12", "lines 3, 9, 12 +2 more".
fn lines_label(lines: &[u32]) -> String {
    let shown: Vec<String> = lines.iter().take(MAX_LINES_SHOWN).map(u32::to_string).collect();
    let more = lines.len().saturating_sub(MAX_LINES_SHOWN);
    let word = if lines.len() == 1 { "line" } else { "lines" };
    let tail = if more > 0 { format!(" +{more} more") } else { String::new() };
    format!("{word} {}{tail}", shown.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, status: FileStatus, adds: usize, dels: usize) -> ChangedFile {
        ChangedFile { path: path.into(), old_path: None, status, is_binary: false, adds, dels, hunks: 1 }
    }

    fn scan(lines: &[&str]) -> ContentScan {
        ContentScan {
            added: lines.iter().enumerate().map(|(i, t)| (i as u32 + 1, t.to_string())).collect(),
            ..Default::default()
        }
    }

    fn flags(f: &ChangedFile, sc: &ContentScan, noise: bool, unattributed: bool, old_lines: Option<usize>) -> Vec<Flag> {
        evaluate(&FileFacts { file: f, scan: Some(sc), noise, unattributed, old_lines })
    }

    fn kinds(fl: &[Flag]) -> Vec<FlagKind> {
        fl.iter().map(|f| f.kind).collect()
    }

    fn modified(path: &str) -> ChangedFile {
        file(path, FileStatus::Modified, 1, 0)
    }

    #[test]
    fn clean_file_has_no_flags() {
        assert!(flags(&modified("src/a.ts"), &scan(&["const a = 1;"]), false, false, None).is_empty());
    }

    // 1
    #[test]
    fn rule1_unattributed_non_noise() {
        let fl = flags(&modified("src/a.ts"), &scan(&[]), false, true, None);
        assert_eq!(kinds(&fl), [FlagKind::Unattributed]);
        assert!(flags(&modified("pnpm-lock.yaml"), &scan(&[]), true, true, None).is_empty(), "noise is exempt");
    }

    // 2
    #[test]
    fn rule2_markers_in_added_lines() {
        let sc = scan(&[
            "// TODO realm check",
            "const x = y as any;",
            "it.only('works', () => {});",
            "// @ts-ignore",
            "/* eslint-disable no-console */",
            "describe.skip('x')",
            "// FIXME and XXX",
            "const todos = TODOList; // not a marker",
            "const casany = hasAnyone;",
        ]);
        let fl = flags(&modified("src/a.ts"), &sc, false, false, None);
        assert_eq!(kinds(&fl), [FlagKind::Marker]);
        let r = &fl[0].reason;
        for m in ["TODO (line 1)", "as any (line 2)", ".only( (line 3)", "@ts-ignore (line 4)", "eslint-disable (line 5)", ".skip( (line 6)", "FIXME (line 7)", "XXX (line 7)"] {
            assert!(r.contains(m), "{m} missing from {r}");
        }
        assert_eq!(fl[0].line, Some(1));
    }

    #[test]
    fn rule2_lists_at_most_three_lines() {
        let sc = scan(&["TODO", "TODO", "TODO", "TODO", "TODO"]);
        let fl = flags(&modified("a.ts"), &sc, false, false, None);
        assert_eq!(fl[0].reason, "Adds TODO (lines 1, 2, 3 +2 more).");
    }

    // 3
    #[test]
    fn rule3_test_paths() {
        for p in ["src/a.test.ts", "src/a.spec.tsx", "test/auth.e2e-spec.ts", "tests/util.py", "pkg/a_test.go", "test_api.py", "src/__tests__/x.js"] {
            assert!(is_test_path(p), "{p}");
        }
        for p in ["src/testing.ts", "src/contest.ts", "latest/a.ts", "src/spec.ts"] {
            assert!(!is_test_path(p), "{p}");
        }
    }

    #[test]
    fn rule3_deleted_or_shrunk_test_file() {
        let del = flags(&file("src/a.test.ts", FileStatus::Deleted, 0, 40), &scan(&[]), false, false, Some(40));
        assert_eq!(kinds(&del), [FlagKind::TestRemoval]);
        assert_eq!(del[0].reason, "Deletes a test file.");

        let shrunk = flags(&file("src/a.test.ts", FileStatus::Modified, 2, 13), &scan(&[]), false, false, Some(40));
        assert_eq!(shrunk[0].reason, "Removes 32% of this test file (13 of 40 lines).");

        let trimmed = flags(&file("src/a.test.ts", FileStatus::Modified, 2, 12), &scan(&[]), false, false, Some(40));
        assert!(trimmed.is_empty(), "exactly 30% is not more than 30%");

        let src = flags(&file("src/a.ts", FileStatus::Deleted, 0, 40), &scan(&[]), false, false, Some(40));
        assert!(src.is_empty(), "not a test file");
    }

    // 4
    #[test]
    fn rule4_env_files() {
        for p in [".env", ".env.example", "apps/api/.env.local"] {
            assert_eq!(kinds(&flags(&modified(p), &scan(&[]), false, false, None)), [FlagKind::Secret], "{p}");
        }
        assert!(flags(&modified("src/env.ts"), &scan(&[]), false, false, None).is_empty());
        assert!(flags(&modified("docs/.envrc-notes.md"), &scan(&[]), false, false, None).is_empty());
    }

    #[test]
    fn rule4_secret_patterns_never_show_the_value() {
        // Built at runtime so the fixtures don't trip secret scanners.
        let aws = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let sk = format!("sk-{}", "ant-api03-Zx9Qw8Er7Ty6Ui5Op4As3Df2");
        let token = concat!("dGhpc2lzYWxvbmdi", "YXNlNjR2YWx1ZTEyMzQ1Ng==").to_string();
        let pem = format!("-----BEGIN {}-----", "RSA PRIVATE KEY");
        let lines = [
            format!("const k = '{aws}';"),
            pem.clone(),
            format!("ANTHROPIC = \"{sk}\""),
            format!("API_TOKEN={token}"),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let fl = flags(&modified("src/config.ts"), &scan(&refs), false, false, None);
        assert_eq!(kinds(&fl), [FlagKind::Secret]);
        let r = &fl[0].reason;
        for name in ["AWS access key (line 1)", "PEM block (line 2)", "sk- API key (line 3)", "credential assignment (line 4)"] {
            assert!(r.contains(name), "{name} missing from {r}");
        }
        for secret in [aws.as_str(), sk.as_str(), token.as_str(), "RSA PRIVATE KEY", "IOSFODNN"] {
            assert!(!r.contains(secret), "reason leaks {secret}: {r}");
        }
        assert!(r.ends_with("Value not shown."));
    }

    #[test]
    fn rule4_placeholders_are_not_secrets() {
        let sc = scan(&[
            "API_KEY=your-api-key-goes-here-please",
            "TOKEN = process.env.GITHUB_TOKEN_VALUE_FROM_ENV",
            "const sk = 'sk-short';",
        ]);
        assert!(flags(&modified("src/a.ts"), &sc, false, false, None).is_empty());
    }

    #[test]
    fn rule4_applies_to_noise_too() {
        let aws = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let line = format!("\"key\": \"{aws}\"");
        let fl = flags(&modified("dist/bundle.js"), &scan(&[&line]), true, true, None);
        assert_eq!(kinds(&fl), [FlagKind::Secret]);
    }

    // 5
    #[test]
    fn rule5_conflict_markers() {
        let sc = scan(&["<<<<<<< HEAD", "a", "=======", "b", ">>>>>>> feature"]);
        let fl = flags(&modified("src/a.ts"), &sc, false, false, None);
        assert_eq!(kinds(&fl), [FlagKind::Conflict]);
        assert_eq!(fl[0].reason, "Conflict markers (lines 1, 5).");

        let rst = scan(&["Title", "======="]);
        assert!(flags(&modified("README.rst"), &rst, false, false, None).is_empty(), "a bare ======= is a heading");

        let idx = flags(&file("src/a.ts", FileStatus::Conflicted, 0, 0), &scan(&[]), false, false, None);
        assert_eq!(kinds(&idx), [FlagKind::Conflict]);
    }

    // 6
    #[test]
    fn rule6_large_change() {
        let fl = flags(&file("src/a.ts", FileStatus::Modified, 300, 101), &scan(&[]), false, false, None);
        assert_eq!(kinds(&fl), [FlagKind::Large]);
        assert_eq!(fl[0].reason, "Large change: 401 changed lines (+300 −101).");
        assert!(flags(&file("src/a.ts", FileStatus::Modified, 300, 100), &scan(&[]), false, false, None).is_empty());
        assert!(flags(&file("pnpm-lock.yaml", FileStatus::Modified, 900, 900), &scan(&[]), true, false, None).is_empty());
    }

    #[test]
    fn rules_combine_in_a_stable_order() {
        let fl = flags(&file("tests/a.test.ts", FileStatus::Modified, 450, 20), &scan(&["// TODO"]), false, true, Some(50));
        assert_eq!(kinds(&fl), [FlagKind::Unattributed, FlagKind::Marker, FlagKind::TestRemoval, FlagKind::Large]);
    }
}
