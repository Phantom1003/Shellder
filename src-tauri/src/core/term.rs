//! A shell on a host, shown in a window of our own: `ssh host` with a
//! pseudo-terminal for its screen and keyboard, and, as everywhere else, a
//! second pty as its controlling terminal where its questions arrive and
//! are answered. The two never mix: a password goes from the keychain (or
//! the prompt window) down the controlling terminal, the remote shell's
//! output comes up the screen pty, and nothing typed in the window ever
//! reaches a password prompt. Through the master when one is up, on its
//! own login otherwise. With a `command` (the Console's `tmux attach`)
//! ssh runs that on the host instead of the login shell, with a pty still.
//! On this machine (`ssh::LOCAL`) there is no ssh: the account's login
//! shell runs on the screen pty, which is its controlling terminal too
//! (job control wants that), and a `command` is what that shell runs. A
//! plain shell (`Launch::Shell`) can start in a directory and with the
//! shell integration (`shellint`), which on a host rides in the ssh
//! command and on this machine is a setup script in the state directory.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use super::log;
use super::prompt::{Origin, PromptHandler};
use super::pty::Pty;
use super::{shellint, ssh};

/// What a terminal runs: a command (a `tmux attach`, a `docker exec`), or
/// the account's shell, in `dir` when given, with the shell integration
/// when asked for.
#[derive(Debug, Clone, Copy)]
pub enum Launch<'a> {
    Command(&'a str),
    Shell {
        dir: Option<&'a str>,
        integration: bool,
    },
}

pub struct Terminal {
    pub host: String,
    pub pid: u32,
    /// Our side of the screen pty: what is written here is typed, what is
    /// read is drawn.
    screen: Arc<OwnedFd>,
    /// The controlling terminal's slave side, held for the process's
    /// lifetime (see `pty`); none for a shell on this machine, whose
    /// controlling terminal is the screen.
    _asking: Option<OwnedFd>,
    gone: Arc<AtomicBool>,
}

/// The account's login shell: what the passwd entry says, else `SHELL`,
/// else sh.
pub fn login_shell() -> String {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if !pw.is_null() && !(*pw).pw_shell.is_null() {
            let s = std::ffi::CStr::from_ptr((*pw).pw_shell)
                .to_string_lossy()
                .into_owned();
            if !s.is_empty() {
                return s;
            }
        }
    }
    match std::env::var("SHELL") {
        Ok(s) if !s.is_empty() => s,
        _ => "/bin/sh".into(),
    }
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
        launch: Launch<'_>,
        cols: u16,
        rows: u16,
        handler: Arc<dyn PromptHandler>,
        mut on_output: impl FnMut(&[u8]) + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<Terminal, String> {
        let screen = Pty::open()?;
        let size = winsize(cols, rows);
        unsafe { libc::ioctl(screen.master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        let local = ssh::is_local(host);
        // What ssh runs on the host: the command, the shell integration's
        // setup with the shell in it, or the plain shell in its directory
        // (nothing: the login shell).
        let command: Option<String> = match launch {
            Launch::Command(c) => Some(c.to_string()),
            Launch::Shell { .. } if local => None,
            Launch::Shell {
                dir,
                integration: true,
            } => Some(shellint::remote_command(dir)),
            Launch::Shell {
                dir: Some(d),
                integration: false,
            } => Some(shellint::plain_command(d)),
            Launch::Shell { dir: None, .. } => None,
        };
        let command = command.as_deref();
        let mut cmd = if local {
            // The login shell, as a terminal application starts it: `-l`
            // for its profile, `-c` for a command; its own directory is
            // home (or the one asked for), and a locale when the app was
            // started without one (an app launched from the Finder has
            // none set). With the integration, the setup script starts
            // the shell instead, with its files kept in the state
            // directory.
            let shell = login_shell();
            let setup = match launch {
                Launch::Shell {
                    integration: true, ..
                } => match shellint::local_setup() {
                    Ok(s) => Some(s),
                    Err(e) => {
                        log::warn(format!("shell integration not set up: {e}"));
                        None
                    }
                },
                _ => None,
            };
            let mut c = match &setup {
                Some((dir, script)) => {
                    let mut c = Command::new("/bin/sh");
                    c.arg(script)
                        .env("SHELLDER_SI_DIR", dir)
                        .env("SHELLDER_SI_KEEP", "1");
                    c
                }
                None => {
                    let mut c = Command::new(&shell);
                    c.arg("-l");
                    if let Some(cmd) = command {
                        c.args(["-c", cmd]);
                    }
                    c
                }
            };
            c.env("SHELL", &shell);
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let dir = match launch {
                Launch::Shell { dir: Some(d), .. } if std::path::Path::new(d).is_dir() => {
                    Some(std::path::PathBuf::from(d))
                }
                _ => home,
            };
            if let Some(dir) = dir {
                c.current_dir(dir);
            }
            if std::env::var_os("LANG").is_none() && std::env::var_os("LC_ALL").is_none() {
                c.env("LANG", "en_US.UTF-8");
            }
            // The screen is the shell's controlling terminal: setsid and
            // TIOCSCTTY on it, which `attach` does.
            screen.attach(&mut c);
            c
        } else {
            let mut c = Command::new(ssh::binary("ssh"));
            c.args(ssh::base_args());
            if let Ok(resolved) = ssh::resolve(host) {
                c.args(ssh::keepalive_args(&resolved));
            }
            // One failed login per attempt, as for a master: a stored secret
            // the server rejects must not be offered again and again.
            c.args(["-o", "NumberOfPasswordPrompts=1"]);
            if let Some(cmd) = command {
                // A command gets no remote pty unless asked for one.
                c.args(["-t", host, cmd]);
            } else {
                c.arg(host);
            }
            c
        };
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
        let (mut child, asking) = if local {
            let child = cmd.spawn().map_err(|e| {
                format!("cannot start {}: {e}", cmd.get_program().to_string_lossy())
            })?;
            (child, None)
        } else {
            let (child, asking) = ssh::spawn_asking(cmd, host, Origin::Terminal, handler)?;
            // The controlling terminal's window size is what SIGWINCH is
            // about: ssh learns of a resize through it and reads the new
            // size from its stdin, the screen. Both are kept the same; a
            // size set through the slave side is the terminal's size like
            // one set through the master.
            unsafe { libc::ioctl(asking.as_raw_fd(), libc::TIOCSWINSZ, &size) };
            (child, Some(asking))
        };
        let pid = child.id();
        let what = if local { "shell" } else { "ssh" };
        match launch {
            Launch::Command(c) => log::info(format!(
                "{host}: terminal opened for `{c}` ({what} pid {pid})"
            )),
            Launch::Shell { dir, integration } => log::info(format!(
                "{host}: terminal opened for the shell{}{} ({what} pid {pid})",
                dir.map(|d| format!(" in {d}")).unwrap_or_default(),
                if integration {
                    " with the shell integration"
                } else {
                    ""
                }
            )),
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
                log::info(format!("{h}: terminal closed ({what} exited rc={code})"));
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
            // A shell on this machine has the screen for both.
            libc::ioctl(self.screen.as_raw_fd(), libc::TIOCSWINSZ, &size);
            if let Some(asking) = &self._asking {
                libc::ioctl(asking.as_raw_fd(), libc::TIOCSWINSZ, &size);
            }
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
