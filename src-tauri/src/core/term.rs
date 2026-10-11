//! A shell on a host, shown in a window of our own: `ssh host` with a
//! pseudo-terminal for its screen and keyboard, and, as everywhere else, a
//! second pty as its controlling terminal where its questions arrive and
//! are answered. The two never mix: a password goes from the keychain (or
//! the prompt window) down the controlling terminal, the remote shell's
//! output comes up the screen pty, and nothing typed in the window ever
//! reaches a password prompt. Through the master when one is up, on its
//! own login otherwise. With a `command` (the Console's `tmux attach`)
//! ssh runs that on the host instead of the login shell, with a pty still.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use super::log;
use super::prompt::{Origin, PromptHandler};
use super::pty::Pty;
use super::ssh;

pub struct Terminal {
    pub host: String,
    pub pid: u32,
    /// Our side of the screen pty: what is written here is typed, what is
    /// read is drawn.
    screen: Arc<OwnedFd>,
    /// The controlling terminal's slave side, held for the process's
    /// lifetime (see `pty`).
    _asking: OwnedFd,
    gone: Arc<AtomicBool>,
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

impl Terminal {
    /// Start `ssh host [command]` at `cols` × `rows`. `on_output` is called
    /// from a thread with every piece of screen output, `on_exit` once, with
    /// the exit status, when ssh is gone.
    pub fn open(
        host: &str,
        command: Option<&str>,
        cols: u16,
        rows: u16,
        handler: Arc<dyn PromptHandler>,
        mut on_output: impl FnMut(&[u8]) + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<Terminal, String> {
        let screen = Pty::open()?;
        let size = winsize(cols, rows);
        unsafe { libc::ioctl(screen.master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        let mut cmd = Command::new(ssh::binary("ssh"));
        cmd.args(ssh::base_args());
        if let Ok(resolved) = ssh::resolve(host) {
            cmd.args(ssh::keepalive_args(&resolved));
        }
        // One failed login per attempt, as for a master: a stored secret
        // the server rejects must not be offered again and again.
        cmd.args(["-o", "NumberOfPasswordPrompts=1"]);
        if let Some(c) = command {
            // A command gets no remote pty unless asked for one.
            cmd.args(["-t", host, c]);
        } else {
            cmd.arg(host);
        }
        cmd.env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor");
        // The screen pty is the child's stdin, stdout and stderr: ssh sees
        // a terminal, puts it in raw mode and asks the server for a pty of
        // the same size. The copies Command keeps go with it, so the only
        // holder of the slave after spawn is the child, and our side reads
        // as hung up when it exits.
        let slave = std::fs::File::from(screen.slave);
        cmd.stdin(Stdio::from(slave.try_clone().map_err(|e| e.to_string())?))
            .stdout(Stdio::from(slave.try_clone().map_err(|e| e.to_string())?))
            .stderr(Stdio::from(slave));
        let (mut child, asking) = ssh::spawn_asking(cmd, host, Origin::Terminal, handler)?;
        let pid = child.id();
        // The controlling terminal's window size is what SIGWINCH is about:
        // ssh learns of a resize through it and reads the new size from its
        // stdin, the screen. Both are kept the same; a size set through the
        // slave side is the terminal's size like one set through the master.
        unsafe { libc::ioctl(asking.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        match command {
            Some(c) => log::info(format!("{host}: terminal opened for `{c}` (ssh pid {pid})")),
            None => log::info(format!("{host}: terminal opened (ssh pid {pid})")),
        }
        let screen = Arc::new(screen.master);
        let gone = Arc::new(AtomicBool::new(false));
        let reader = screen.clone();
        let flag = gone.clone();
        let h = host.to_string();
        thread::Builder::new()
            .name(format!("term-{host}"))
            .spawn(move || {
                let mut file = std::fs::File::from(reader.try_clone().expect("dup"));
                let mut buf = [0u8; 16384];
                loop {
                    match file.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => on_output(&buf[..n]),
                        Err(e) if e.raw_os_error() == Some(libc::EINTR) => continue,
                        // EIO: the slave side is closed, ssh is gone.
                        Err(_) => break,
                    }
                }
                let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
                flag.store(true, Ordering::SeqCst);
                log::info(format!("{h}: terminal closed (ssh exited rc={code})"));
                on_exit(code);
            })
            .map_err(|e| e.to_string())?;
        Ok(Terminal {
            host: host.to_string(),
            pid,
            screen,
            _asking: asking,
            gone,
        })
    }

    /// Keys typed in the window.
    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut f = std::fs::File::from(self.screen.try_clone()?);
        f.write_all(bytes)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let size = winsize(cols, rows);
        unsafe {
            // The screen first, so that when the SIGWINCH from the
            // controlling terminal arrives ssh already reads the new size.
            libc::ioctl(self.screen.as_raw_fd(), libc::TIOCSWINSZ, &size);
            libc::ioctl(self._asking.as_raw_fd(), libc::TIOCSWINSZ, &size);
        }
    }

    pub fn is_running(&self) -> bool {
        !self.gone.load(Ordering::SeqCst)
    }

    /// Hang up: what closing the window means. ssh exits, the reader
    /// thread reaps it and reports.
    pub fn close(&self) {
        if self.is_running() {
            unsafe { libc::kill(self.pid as i32, libc::SIGHUP) };
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.close();
    }
}
