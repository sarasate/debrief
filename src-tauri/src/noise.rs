//! Generated files and lockfiles (SPEC §3.4), with the `.debrief.toml`
//! override at the repo root:
//!
//! ```toml
//! [noise]
//! globs = ["*.lock"]            # replace the defaults entirely
//! extra = ["**/*.generated.cs"] # or add to them
//! ```
//!
//! The file is only read, never written. A broken file falls back to the
//! defaults and reports why, so a typo can't hide or unmask everything.

use crate::error::AppResult;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFIG_FILE: &str = ".debrief.toml";

pub const DEFAULT_GLOBS: &[&str] = &[
    "*.lock",
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "bun.lockb",
    "Cargo.lock",
    "**/__generated__/**",
    "**/*.gen.ts",
    "**/routeTree.gen.ts",
    "**/*.snap",
    "**/dist/**",
    "**/build/**",
    "**/*.min.js",
    "**/openapi.json",
];

pub struct Noise {
    /// Patterns with a `/`, matched against the repo-relative path.
    paths: GlobSet,
    /// Bare patterns (`*.lock`), matched against the file name at any depth,
    /// as in `.gitignore`.
    names: GlobSet,
}

impl Noise {
    pub fn new<S: AsRef<str>>(globs: &[S]) -> AppResult<Self> {
        let (mut paths, mut names) = (GlobSetBuilder::new(), GlobSetBuilder::new());
        for g in globs {
            let g = g.as_ref();
            let glob = GlobBuilder::new(g).literal_separator(true).build()?;
            if g.contains('/') {
                paths.add(glob);
            } else {
                names.add(glob);
            }
        }
        Ok(Self { paths: paths.build()?, names: names.build()? })
    }

    pub fn defaults() -> AppResult<Self> {
        Self::new(DEFAULT_GLOBS)
    }

    pub fn matches(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.paths.is_match(path) || self.names.is_match(name)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    noise: NoiseSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoiseSection {
    globs: Option<Vec<String>>,
    #[serde(default)]
    extra: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConfigSource {
    Defaults,
    File,
}

/// The noise globs in effect, as shown to the user.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoiseConfig {
    pub globs: Vec<String>,
    pub source: ConfigSource,
    /// Why `.debrief.toml` was ignored, if it was.
    pub error: Option<String>,
}

/// Noise for the repo at `root`, from `.debrief.toml` when present and valid.
pub fn load(root: &Path) -> AppResult<(Noise, NoiseConfig)> {
    let defaults = || DEFAULT_GLOBS.iter().map(|g| g.to_string()).collect::<Vec<_>>();
    let text = match std::fs::read_to_string(root.join(CONFIG_FILE)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Noise::defaults()?, NoiseConfig { globs: defaults(), source: ConfigSource::Defaults, error: None }));
        }
        Err(e) => return fallback(defaults(), format!("{CONFIG_FILE}: {e}")),
    };
    let parsed: ConfigFile = match toml::from_str(&text) {
        Ok(c) => c,
        Err(e) => return fallback(defaults(), format!("{CONFIG_FILE}: {}", e.message())),
    };
    let mut globs = parsed.noise.globs.unwrap_or_else(defaults);
    globs.extend(parsed.noise.extra);
    match Noise::new(&globs) {
        Ok(n) => Ok((n, NoiseConfig { globs, source: ConfigSource::File, error: None })),
        Err(e) => fallback(defaults(), format!("{CONFIG_FILE}: {e}")),
    }
}

fn fallback(globs: Vec<String>, error: String) -> AppResult<(Noise, NoiseConfig)> {
    Ok((Noise::new(&globs)?, NoiseConfig { globs, source: ConfigSource::Defaults, error: Some(error) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_globs_match_lockfiles_and_codegen() {
        let n = Noise::defaults().unwrap();
        for p in [
            "pnpm-lock.yaml",
            "apps/web/pnpm-lock.yaml",
            "src-tauri/Cargo.lock",
            "poetry.lock",
            "apps/web/src/routeTree.gen.ts",
            "apps/api/src/__generated__/openapi.json",
            "openapi.json",
            "apps/web/dist/index.js",
            "dist/index.js",
            "packages/ui/build/out.css",
            "src/__snapshots__/app.test.ts.snap",
            "public/vendor.min.js",
        ] {
            assert!(n.matches(p), "{p} should be noise");
        }
    }

    #[test]
    fn source_files_are_not_noise() {
        let n = Noise::defaults().unwrap();
        for p in [
            "src/lock.ts",
            "src/distance.ts",
            "apps/api/src/auth/auth.guard.ts",
            "src/buildInfo.ts",
            "package.json",
            "docs/openapi.md",
        ] {
            assert!(!n.matches(p), "{p} should not be noise");
        }
    }

    fn with_config(body: Option<&str>) -> (tempfile::TempDir, Noise, NoiseConfig) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(b) = body {
            std::fs::write(dir.path().join(CONFIG_FILE), b).unwrap();
        }
        let (n, c) = load(dir.path()).unwrap();
        (dir, n, c)
    }

    #[test]
    fn no_config_file_uses_defaults() {
        let (_d, n, c) = with_config(None);
        assert_eq!(c.source, ConfigSource::Defaults);
        assert!(c.error.is_none());
        assert!(n.matches("Cargo.lock"));
    }

    #[test]
    fn extra_globs_add_to_the_defaults() {
        let (_d, n, c) = with_config(Some("[noise]\nextra = [\"**/*.generated.cs\"]\n"));
        assert_eq!(c.source, ConfigSource::File);
        assert!(n.matches("src/Api.generated.cs"));
        assert!(n.matches("pnpm-lock.yaml"), "defaults still apply");
    }

    #[test]
    fn globs_replace_the_defaults() {
        let (_d, n, c) = with_config(Some("[noise]\nglobs = [\"docs/**\"]\n"));
        assert_eq!(c.globs, ["docs/**"]);
        assert!(n.matches("docs/a.md"));
        assert!(!n.matches("pnpm-lock.yaml"), "defaults replaced");
    }

    #[test]
    fn broken_config_falls_back_to_defaults_and_says_why() {
        for body in ["[noise\nglobs = 1", "[noise]\nglob = [\"x\"]\n", "[noise]\nextra = [\"a[\"]\n"] {
            let (_d, n, c) = with_config(Some(body));
            assert_eq!(c.source, ConfigSource::Defaults, "{body}");
            assert!(c.error.as_deref().is_some_and(|e| e.starts_with(".debrief.toml")), "{body}: {:?}", c.error);
            assert!(n.matches("Cargo.lock"));
        }
    }
}
