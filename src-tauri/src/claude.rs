//! Resume mode (SPEC §6): run `claude --resume <id> -p <prompt>` as a child
//! process and stream what it prints. Arguments go straight to the process,
//! never through a shell, and nothing from a transcript is run: the session
//! id is validated and the prompt is ours.

use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

/// A GUI app on macOS starts with a bare PATH, so look where the installers
/// put `claude` too.
pub fn find_claude(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p.trim());
        return p.is_file().then_some(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(h) = &home {
        dirs.push(h.join(".claude/local"));
        dirs.push(h.join(".local/bin"));
        dirs.push(h.join(".npm-global/bin"));
        dirs.push(h.join(".bun/bin"));
    }
    dirs.push("/opt/homebrew/bin".into());
    dirs.push("/usr/local/bin".into());
    dirs.into_iter().map(|d| d.join("claude")).find(|p| p.is_file())
}

pub struct ClaudeRunner {
    child: Arc<Mutex<Option<Child>>>,
}

impl ClaudeRunner {
    pub fn new() -> Self {
        Self { child: Arc::new(Mutex::new(None)) }
    }

    #[cfg(test)]
    pub fn running(&self) -> bool {
        self.child.lock().is_some()
    }

    /// Start `bin args…` in `cwd`. Each output line goes to `on_line(stream,
    /// line)`; `on_exit(code)` runs once it ends (None if killed).
    pub fn spawn(
        &self,
        bin: &Path,
        args: &[String],
        cwd: &Path,
        on_line: impl Fn(&'static str, String) + Send + Sync + 'static,
        on_exit: impl FnOnce(Option<i32>) + Send + 'static,
    ) -> AppResult<()> {
        let mut slot = self.child.lock();
        if slot.is_some() {
            return Err(AppError::Input("a resume is already running".into()));
        }
        let mut child = Command::new(bin)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let on_line = Arc::new(on_line);
        let mut readers = vec![];
        if let Some(out) = child.stdout.take() {
            readers.push(pump("stdout", out, on_line.clone()));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(pump("stderr", err, on_line.clone()));
        }
        *slot = Some(child);
        drop(slot);

        let shared = self.child.clone();
        std::thread::spawn(move || {
            // Poll so `cancel` can take the lock and kill in between.
            let code = loop {
                let mut guard = shared.lock();
                let Some(c) = guard.as_mut() else { break None };
                match c.try_wait() {
                    Ok(Some(status)) => {
                        *guard = None;
                        break status.code();
                    }
                    Ok(None) => {}
                    Err(_) => {
                        *guard = None;
                        break None;
                    }
                }
                drop(guard);
                std::thread::sleep(Duration::from_millis(150));
            };
            for r in readers {
                let _ = r.join();
            }
            on_exit(code);
        });
        Ok(())
    }

    /// Kill the running resume, if any.
    pub fn cancel(&self) -> bool {
        let mut guard = self.child.lock();
        match guard.take() {
            Some(mut c) => {
                let _ = c.kill();
                let _ = c.wait();
                true
            }
            None => false,
        }
    }
}

fn pump<R: Read + Send + 'static>(
    stream: &'static str,
    r: R,
    on_line: Arc<impl Fn(&'static str, String) + Send + Sync + 'static>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(r).lines().map_while(Result::ok) {
            on_line(stream, line);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn streams_lines_and_reports_exit() {
        let runner = ClaudeRunner::new();
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        runner
            .spawn(
                Path::new("/bin/echo"),
                &["hello; rm -rf /".into(), "$HOME".into()],
                dir.path(),
                move |s, l| tx.send((s, l)).unwrap(),
                move |code| done_tx.send(code).unwrap(),
            )
            .unwrap();
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap(), Some(0));
        let lines: Vec<_> = rx.try_iter().collect();
        assert_eq!(lines, [("stdout", "hello; rm -rf / $HOME".to_string())], "args are passed verbatim, no shell");
        assert!(!runner.running());
    }

    #[test]
    fn cancel_kills_the_child() {
        let runner = ClaudeRunner::new();
        let (done_tx, done_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        runner.spawn(Path::new("/bin/sleep"), &["30".into()], dir.path(), |_, _| {}, move |c| done_tx.send(c).unwrap()).unwrap();
        assert!(runner.running());
        assert!(runner.spawn(Path::new("/bin/sleep"), &["1".into()], dir.path(), |_, _| {}, |_| {}).is_err(), "one at a time");
        assert!(runner.cancel());
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap(), None);
    }

    #[test]
    fn configured_path_must_exist() {
        assert_eq!(find_claude(Some("/bin/echo")), Some(PathBuf::from("/bin/echo")));
        assert_eq!(find_claude(Some("/nope/claude")), None);
    }
}
