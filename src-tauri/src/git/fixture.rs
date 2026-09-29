//! Throwaway repos in a temp dir for unit tests.

use crate::state::RepoState;
use git2::{Repository, Signature};
use std::path::Path;
use tempfile::TempDir;

pub struct Fixture {
    pub dir: TempDir,
    pub repo: Repository,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        Self { dir, repo }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn state(&self) -> RepoState {
        let s = RepoState::new();
        s.set_path(self.root().to_path_buf());
        s
    }

    pub fn write(&self, rel: &str, content: &str) {
        self.write_bytes(rel, content.as_bytes());
    }

    pub fn write_bytes(&self, rel: &str, content: &[u8]) {
        let p = self.root().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        std::fs::remove_file(self.root().join(rel)).unwrap();
    }

    pub fn stage(&self, rel: &str) {
        let mut index = self.repo.index().unwrap();
        index.add_path(Path::new(rel)).unwrap();
        index.write().unwrap();
    }

    /// Write, stage and commit `files` on top of HEAD.
    pub fn commit(&self, files: &[(&str, &str)]) {
        self.commit_msg(files, "fixture");
    }

    /// `commit` with a message (subject, body, trailers).
    pub fn commit_msg(&self, files: &[(&str, &str)], message: &str) {
        for (rel, content) in files {
            self.write(rel, content);
            self.stage(rel);
        }
        let mut index = self.repo.index().unwrap();
        let tree = self.repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Debrief Test", "test@example.invalid").unwrap();
        let parent = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<_> = parent.iter().collect();
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }

    /// Create `name` at HEAD without switching to it.
    pub fn branch(&self, name: &str) {
        let head = self.repo.head().unwrap().peel_to_commit().unwrap();
        self.repo.branch(name, &head, false).unwrap();
    }

    /// Switch to branch `name`, resetting the worktree to its tip.
    pub fn checkout(&self, name: &str) {
        let refname = format!("refs/heads/{name}");
        let obj = self.repo.revparse_single(&refname).unwrap();
        let mut co = git2::build::CheckoutBuilder::new();
        co.force().remove_untracked(true);
        self.repo.checkout_tree(&obj, Some(&mut co)).unwrap();
        self.repo.set_head(&refname).unwrap();
    }

    /// Point branch `name` at the commit `rev` resolves to (a force-push).
    pub fn move_branch(&self, name: &str, rev: &str) {
        let c = self.repo.revparse_single(rev).unwrap().peel_to_commit().unwrap();
        self.repo.branch(name, &c, true).unwrap();
    }
}
