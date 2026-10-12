//! A shell on a host that stays open: `ssh host sh`, logged in once (its
//! questions answered on the controlling pty like a master's), then given
//! one command after another on its stdin. It is what a Files window
//! lists through when the host has no master up — on a platform without
//! ControlMaster, always — so the listings cost one login, not one each.
//! (The copies are sftp, which logs in on its own, see `ssh::copy`.)
//!
//! The protocol is a marker after every command: the command runs with
//! its output on the channel, then `printf` writes a line that only we
//! know (`@@shellder:N:status@@`), which is where its output ends and
//! what it exited with. Nothing is written to the shell before the
//! previous command's marker has arrived, so a shell that reads its input
//! in chunks (BusyBox's ash) never has more in it than the one command.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use super::log;
use super::prompt::{Origin, PromptHandler, PromptRequest};
use super::ssh;

/// What a command said and how it ended, in ssh's own shape so a caller
/// that ran commands through a master needs no second path.
pub type Output = ssh::Output;

struct Inner {
    stdin: ChildStdin,
    stdout: ChildStdout,
    /// Read from the channel, not yet handed out.
    buf: Vec<u8>,
    next: u64,
    child: Child,
}

pub struct Session {
    pub host: String,
    pub pid: u32,
    inner: Mutex<Inner>,
    dead: AtomicBool,
    tail: Arc<Mutex<ssh::Tail>>,
    _asking: OwnedFd,
}

/// Answers nothing: a session opened before the app set its handler
/// (or by a test that wants no questions) fails at the first question.
struct Nobody;

impl PromptHandler for Nobody {
    fn ask(&self, _req: &PromptRequest) -> Option<String> {
        None
    }
}

static HANDLER: Mutex<Option<Arc<dyn PromptHandler>>> = Mutex::new(None);
static SESSIONS: Mutex<Option<HashMap<String, Arc<Session>>>> = Mutex::new(None);

/// Who answers the questions of the sessions, and of the copies that log
/// in on their own, from now on: the app's model, once it is up.
pub fn set_prompt_handler(h: Arc<dyn PromptHandler>) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
}

pub fn prompt_handler() -> Arc<dyn PromptHandler> {
    HANDLER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| Arc::new(Nobody))
}

/// The host's session, opened (logged in) now when there is none or the
/// one there was is gone. Blocks for the login, questions included.
pub fn for_host(host: &str) -> Result<Arc<Session>, String> {
    {
        let guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = guard.as_ref().and_then(|m| m.get(host)) {
            if s.is_alive() {
                return Ok(s.clone());
            }
        }
    }
    let s = Arc::new(Session::open(host, prompt_handler())?);
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(HashMap::new)
        .insert(host.to_string(), s.clone());
    Ok(s)
}

/// The host's session if one is open, without opening one.
pub fn existing(host: &str) -> Option<Arc<Session>> {
    let guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .and_then(|m| m.get(host))
        .filter(|s| s.is_alive())
        .cloned()
}

/// Log out of every host: what quitting does.
pub fn close_all() {
    let all: Vec<Arc<Session>> = {
        let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .take()
            .map(|m| m.into_values().collect())
            .unwrap_or_default()
    };
    for s in &all {
        s.close();
    }
    // Told to go is not gone: an ssh still there when the app has exited
    // keeps the app "running" for macOS 26. A moment, not for ever.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !all.iter().all(|s| s.is_gone()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

const MARK: &str = "@@shellder:";

impl Session {
    pub fn open(host: &str, handler: Arc<dyn PromptHandler>) -> Result<Session, String> {
        let mut cmd = Command::new(ssh::binary("ssh"));
        cmd.args(ssh::base_args());
        if let Ok(resolved) = ssh::resolve(host) {
            cmd.args(ssh::keepalive_args(&resolved));
        }
        // No pty on either end: the channel carries bytes as they are.
        // sh, whatever the login shell is, so the commands mean the same
        // everywhere.
        cmd.args([
            "-T",
            "-o",
            "NumberOfPasswordPrompts=1",
            "-o",
            "LogLevel=ERROR",
            host,
            "sh",
        ]);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (mut child, asking) = ssh::spawn_asking(cmd, host, Origin::Session, handler)?;
        let pid = child.id();
        let tail = Arc::new(Mutex::new(ssh::Tail::default()));
        if let Some(err) = child.stderr.take() {
            ssh::line_pipe(host.to_string(), err, Some(tail.clone()), None);
        }
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let s = Session {
            host: host.to_string(),
            pid,
            inner: Mutex::new(Inner {
                stdin,
                stdout,
                buf: Vec::new(),
                next: 0,
                child,
            }),
            dead: AtomicBool::new(false),
            tail,
            _asking: asking,
        };
        // The first command is the login: its marker arrives once the
        // shell is there, and whatever a profile printed before it is not
        // ours to read.
        let first = s.run(":");
        if first.status != 0 {
            let why = first.text();
            return Err(if why.is_empty() {
                format!("could not log in to {host}")
            } else {
                why
            });
        }
        log::info(format!("{host}: session opened (ssh pid {pid})"));
        Ok(s)
    }

    pub fn is_alive(&self) -> bool {
        !self.dead.load(Ordering::SeqCst)
    }

    /// True once ssh has exited and been reaped. A command still reading
    /// the channel holds the lock; it ends when ssh does.
    fn is_gone(&self) -> bool {
        let mut inner = match self.inner.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
            Err(TryLockError::WouldBlock) => return false,
        };
        matches!(inner.child.try_wait(), Ok(Some(_)))
    }

    /// Log out. A copy under way is cut short; the next call on this host
    /// opens a new session.
    pub fn close(&self) {
        if !self.dead.swap(true, Ordering::SeqCst) {
            log::info(format!("{}: session closed", self.host));
        }
        unsafe { libc::kill(self.pid as i32, libc::SIGTERM) };
    }

    /// The session is no longer usable: the connection dropped, or a copy
    /// stopped half-way and left the shell waiting.
    fn lost(&self, inner: &mut Inner, why: &str) -> String {
        let detail = self
            .tail
            .lock()
            .ok()
            .and_then(|t| t.last_line())
            .map(|l| format!(" ({l})"))
            .unwrap_or_default();
        if !self.dead.swap(true, Ordering::SeqCst) {
            log::warn(format!("{}: session lost: {why}{detail}", self.host));
        }
        let _ = inner.child.kill();
        let _ = inner.child.wait();
        format!("{why}{detail}")
    }

    /// Run a command through the shell, its stdin closed, stderr on the
    /// channel with stdout. The whole output is the Output's stdout. In a
    /// subshell, so that an `exit` or a `cd` in it is its own, not the
    /// session's.
    pub fn run(&self, command: &str) -> Output {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let n = inner.next;
        inner.next += 1;
        let script = format!("(\n{command}\n) </dev/null 2>&1\nprintf '\\n{MARK}{n}:%s@@\\n' $?\n");
        if let Err(e) = inner.stdin.write_all(script.as_bytes()) {
            let why = self.lost(&mut inner, &format!("cannot write to the shell: {e}"));
            return failed(why);
        }
        let mut out = Vec::new();
        match self.until_marker(&mut inner, n, &mut |chunk| {
            out.extend_from_slice(chunk);
            true
        }) {
            Ok(status) => Output {
                status,
                stdout: String::from_utf8_lossy(&out).into_owned(),
                stderr: String::new(),
            },
            Err(e) => failed(e),
        }
    }

    /// Hand the channel's bytes to `sink` up to marker `n`, and say what
    /// status the marker carried. The newline before the marker is the
    /// separator, not output. A command's own output may look like the
    /// start of a marker; one that is not finished by "@@" within a few
    /// bytes is output after all.
    fn until_marker(
        &self,
        inner: &mut Inner,
        n: u64,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<i32, String> {
        let prefix = format!("\n{MARK}{n}:").into_bytes();
        let mut chunk = vec![0u8; 256 * 1024];
        let mut from = 0usize; // buf[..from] has been searched for the prefix
        loop {
            match find(&inner.buf[from..], &prefix).map(|i| i + from) {
                Some(at) => {
                    let body = at + prefix.len();
                    let word_end = inner.buf[body..].iter().position(|&b| b == b'@');
                    match word_end {
                        Some(w) if w <= 8 => {
                            if inner.buf.len() < body + w + 3 {
                                // Still arriving: output up to it, keep the rest.
                                if at > 0 && !sink(&inner.buf[..at]) {
                                    return Err(self.lost(inner, "cancelled"));
                                }
                                inner.buf.drain(..at);
                                from = 0;
                            } else if &inner.buf[body + w..body + w + 3] == b"@@\n" {
                                let word = String::from_utf8_lossy(&inner.buf[body..body + w])
                                    .into_owned();
                                let status = word.parse().unwrap_or(-1);
                                if at > 0 && !sink(&inner.buf[..at]) {
                                    return Err(self.lost(inner, "cancelled"));
                                }
                                inner.buf.drain(..body + w + 3);
                                return Ok(status);
                            } else {
                                from = at + 1; // not a marker after all
                                continue;
                            }
                        }
                        Some(_) => {
                            from = at + 1;
                            continue;
                        }
                        None if inner.buf.len() - body > 8 => {
                            from = at + 1;
                            continue;
                        }
                        None => {
                            if at > 0 && !sink(&inner.buf[..at]) {
                                return Err(self.lost(inner, "cancelled"));
                            }
                            inner.buf.drain(..at);
                            from = 0;
                        }
                    }
                }
                None => {
                    // Everything but a possible start of the prefix is output.
                    let safe = inner.buf.len().saturating_sub(prefix.len() - 1);
                    if safe > 0 && !sink(&inner.buf[..safe]) {
                        return Err(self.lost(inner, "cancelled"));
                    }
                    inner.buf.drain(..safe);
                    from = 0;
                }
            }
            match inner.stdout.read(&mut chunk) {
                Ok(0) => return Err(self.lost(inner, "the connection closed")),
                Ok(k) => inner.buf.extend_from_slice(&chunk[..k]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(self.lost(inner, &format!("the connection dropped: {e}"))),
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let _ = inner.child.kill();
        let _ = inner.child.wait();
    }
}

fn failed(why: String) -> Output {
    Output {
        status: -1,
        stdout: String::new(),
        stderr: why,
    }
}

/// Where `needle` first occurs in `hay`.
pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_bytes() {
        assert_eq!(find(b"abcabc", b"ca"), Some(2));
        assert_eq!(find(b"abc", b"abcd"), None);
        assert_eq!(find(b"", b"a"), None);
    }
}
