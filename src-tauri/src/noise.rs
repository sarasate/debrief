//! Generated files and lockfiles (SPEC §3.4). The `.debrief.toml` override
//! arrives with M3; for now only the defaults apply.

use crate::error::AppResult;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

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
    pub fn new(globs: &[&str]) -> AppResult<Self> {
        let (mut paths, mut names) = (GlobSetBuilder::new(), GlobSetBuilder::new());
        for g in globs {
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
}
