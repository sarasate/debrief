//! The console (docs/PLAN.md M11): a real shell on a pseudo-terminal in the
//! repo under review. It runs whatever the user types, as the user; Debrief
//! itself never writes into it (CLAUDE.md).

use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const READ_CHUNK: usize = 16 * 1024;

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    shell_pid: Option<u32>,
}

/// Every open console, by id. One per repo in practice; the frontend keeps
/// the id.
pub struct Consoles {
    sessions: Arc<Mutex<HashMap<u32, Session>>>,
    next: AtomicU32,
}

/// How to start a shell. `Default` is the user's login shell.
#[derive(Debug, Clone, Default)]
pub struct ShellSpec {
    /// None: `$SHELL`, else /bin/zsh.
    pub program: Option<String>,
    /// None: `-l`, so a GUI app gets the user's PATH.
    pub args: Option<Vec<String>>,
}

impl ShellSpec {
    fn command(&self, cwd: &Path) -> CommandBuilder {
        let program = self
            .program
            .clone()
            .or_else(|| std::env::var("SHELL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "/bin/zsh".into());
        let mut cmd = CommandBuilder::new(program);
        for a in self.args.clone().unwrap_or_else(|| vec!["-l".into()]) {
            cmd.arg(a);
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("DEBRIEF", "1");
        cmd
    }
}

impl Consoles {
    pub fn new() -> Self {
        Self { sessions: Arc::new(Mutex::new(HashMap::new())), next: AtomicU32::new(1) }
    }

    /// Start a shell in `cwd`. Output goes to `on_output` in chunks as it
    /// arrives; `on_exit(id, code)` runs once the shell has ended (code None
    /// if it was killed by a signal).
    pub fn open(
        &self,
        cwd: &Path,
        cols: u16,
        rows: u16,
        shell: &ShellSpec,
        on_output: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(u32, Option<u32>) + Send + 'static,
    ) -> AppResult<u32> {
        let pty = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|e| AppError::Other(format!("console: {e}")))?;
        let mut child = pty
            .slave
            .spawn_command(shell.command(cwd))
            .map_err(|e| AppError::Other(format!("console: {e}")))?;
        // The slave end belongs to the shell now; keeping it open here would
        // stop the reader seeing EOF when the shell exits.
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader().map_err(|e| AppError::Other(format!("console: {e}")))?;
        let writer = pty.master.take_writer().map_err(|e| AppError::Other(format!("console: {e}")))?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.sessions.lock().insert(
            id,
            Session {
                killer: child.clone_killer(),
                shell_pid: child.process_id(),
                master: pty.master,
                writer,
            },
        );

        std::thread::spawn(move || {
            let mut buf = vec![0u8; READ_CHUNK];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => on_output(buf[..n].to_vec()),
                }
            }
        });
        let sessions = self.sessions.clone();
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            sessions.lock().remove(&id);
            on_exit(id, code);
        });
        Ok(id)
    }

    pub fn write(&self, id: u32, data: &[u8]) -> AppResult<()> {
        let mut g = self.sessions.lock();
        let s = g.get_mut(&id).ok_or_else(|| AppError::Input("console is closed".into()))?;
        s.writer.write_all(data)?;
        s.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> AppResult<()> {
        let g = self.sessions.lock();
        let s = g.get(&id).ok_or_else(|| AppError::Input("console is closed".into()))?;
        s.master.resize(size(cols, rows)).map_err(|e| AppError::Other(format!("console: {e}")))
    }

    /// Hang up the terminal (SIGHUP to its foreground job, as closing a
    /// terminal window does) and kill the shell.
    pub fn close(&self, id: u32) -> bool {
        let Some(mut s) = self.sessions.lock().remove(&id) else { return false };
        drop(s.writer);
        let _ = s.killer.kill();
        drop(s.master);
        true
    }

    pub fn close_all(&self) {
        let ids: Vec<u32> = self.sessions.lock().keys().copied().collect();
        for id in ids {
            self.close(id);
        }
    }

    /// A command is running in some console: the terminal's foreground
    /// process group isn't the shell's own.
    pub fn busy(&self) -> bool {
        self.sessions.lock().values().any(|s| match (s.master.process_group_leader(), s.shell_pid) {
            (Some(fg), Some(shell)) => fg as u32 != shell,
            _ => false,
        })
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// Plain `sh` without a profile, so tests don't depend on the user's shell.
    fn sh() -> ShellSpec {
        ShellSpec { program: Some("/bin/sh".into()), args: Some(vec![]) }
    }

    /// Collect output until `needle` shows up or the timeout passes. The
    /// terminal echoes input, so a needle must be something only the
    /// command's output contains (e.g. from `$((…))`).
    fn wait_for(rx: &mpsc::Receiver<Vec<u8>>, out: &mut String, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if out.contains(needle) {
                return true;
            }
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
                out.push_str(&String::from_utf8_lossy(&chunk));
            }
        }
        out.contains(needle)
    }

    fn start(dir: &Path) -> (Consoles, u32, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<Option<u32>>) {
        let consoles = Consoles::new();
        let (tx, rx) = mpsc::channel();
        let (etx, erx) = mpsc::channel();
        let id = consoles
            .open(dir, 100, 30, &sh(), move |b| {
                let _ = tx.send(b);
            }, move |_, c| {
                let _ = etx.send(c);
            })
            .unwrap();
        (consoles, id, rx, erx)
    }

    #[test]
    fn runs_in_the_repo_with_a_real_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (consoles, id, rx, _) = start(&root);
        consoles.write(id, b"pwd; echo \"term=$TERM debrief=$DEBRIEF\"; test -t 0 && echo tty-$((40+2))\n").unwrap();
        let mut out = String::new();
        assert!(wait_for(&rx, &mut out, "tty-42"), "{out}");
        assert!(out.contains(&root.to_string_lossy().to_string()), "{out}");
        assert!(out.contains("term=xterm-256color debrief=1"), "{out}");
        consoles.close(id);
    }

    #[test]
    fn resize_reaches_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let (consoles, id, rx, _) = start(dir.path());
        consoles.resize(id, 132, 40).unwrap();
        consoles.write(id, b"stty size\n").unwrap();
        let mut out = String::new();
        assert!(wait_for(&rx, &mut out, "40 132"), "{out}");
        consoles.close(id);
    }

    #[test]
    fn busy_while_a_command_runs_and_close_kills_it() {
        let dir = tempfile::tempdir().unwrap();
        let (consoles, id, rx, erx) = start(dir.path());
        let mut out = String::new();
        consoles.write(id, b"echo ready-$((1+1))\n").unwrap();
        assert!(wait_for(&rx, &mut out, "ready-2"));
        assert!(!consoles.busy(), "idle shell");
        consoles.write(id, b"sleep 30\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !consoles.busy() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(consoles.busy(), "sleep is the foreground job");

        assert!(consoles.close(id));
        erx.recv_timeout(Duration::from_secs(5)).expect("shell ended after close");
        assert!(consoles.write(id, b"x").is_err(), "closed");
        assert!(!consoles.busy());
    }

    #[test]
    fn exit_is_reported_with_its_code() {
        let dir = tempfile::tempdir().unwrap();
        let (consoles, id, _rx, erx) = start(dir.path());
        consoles.write(id, b"exit 3\n").unwrap();
        assert_eq!(erx.recv_timeout(Duration::from_secs(5)).unwrap(), Some(3));
        assert!(consoles.write(id, b"x").is_err(), "exited console is gone");
    }
}
