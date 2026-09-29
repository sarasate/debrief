use crate::error::{AppError, AppResult};
use crate::git::target::Target;
use git2::Repository;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};

pub struct RepoState {
    inner: Mutex<Option<RepoHandle>>,
}

pub struct RepoHandle {
    pub path: PathBuf,
    /// What is being reviewed; opening a repo starts on its working tree.
    pub target: Target,
}

impl RepoState {
    pub fn new() -> Self {
        Self { inner: Mutex::new(None) }
    }

    pub fn set_path(&self, path: PathBuf) {
        *self.inner.lock() = Some(RepoHandle { path, target: Target::Worktree });
    }

    pub fn path(&self) -> AppResult<PathBuf> {
        self.inner
            .lock()
            .as_ref()
            .map(|h| h.path.clone())
            .ok_or(AppError::NoRepo)
    }

    pub fn target(&self) -> Target {
        self.inner.lock().as_ref().map(|h| h.target.clone()).unwrap_or_default()
    }

    pub fn set_target(&self, target: Target) -> AppResult<()> {
        let mut g = self.inner.lock();
        let h = g.as_mut().ok_or(AppError::NoRepo)?;
        h.target = target;
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.inner.lock().is_some()
    }

    pub fn open(&self) -> AppResult<Repository> {
        let path = self.path()?;
        Repository::discover(path).map_err(Into::into)
    }
}

pub fn discover_path<P: AsRef<Path>>(p: P) -> AppResult<PathBuf> {
    let repo = Repository::discover(p)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repo not supported".into()))?
        .to_path_buf();
    Ok(workdir)
}
