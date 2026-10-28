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
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
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
}

pub struct Session {
    pub host: String,
    pub pid: u32,
    inner: Mutex<Inner>,
    /// The ssh process. Every wait for it and every signal sent to it
    /// happen under this lock, so that its pid is never signalled once it
    /// has been reaped (it may be another process's by then). Never held
    /// for long: a command reading the channel holds `inner`, not this.
    child: Mutex<Child>,
    dead: AtomicBool,
    /// Logged out on purpose (`close`): what fails on it then is not given
    /// to a new session.
    closed: AtomicBool,
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

/// A login under way. Whoever wants the same host's session while it runs
/// waits for it and takes its session, or its error, instead of logging
/// in a second time: another password window, and with TOTP a code the
/// server turns down as used and half a minute's wait for the next.
#[derive(Default)]
struct Login {
    outcome: Mutex<Option<Result<Arc<Session>, String>>>,
    done: Condvar,
}

static LOGINS: Mutex<Option<HashMap<String, Arc<Login>>>> = Mutex::new(None);

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
/// one there was is gone. Blocks for the login, questions included; a
/// caller that comes while another one's login for the host is under way
/// waits for that login and shares its outcome. Other hosts wait for
/// nobody.
pub fn for_host(host: &str) -> Result<Arc<Session>, String> {
    if super::ssh::is_local(host) {
        return Err("this machine has no session: its commands run here".into());
    }
    if let Some(s) = existing(host) {
        return Ok(s);
    }
    let login = {
        let mut logins = LOGINS.lock().unwrap_or_else(|e| e.into_inner());
        let logins = logins.get_or_insert_with(HashMap::new);
        match logins.get(host) {
            Some(l) => Some(l.clone()),
            None => {
                // A login that ended since the look above put its session
                // in place before it left this map.
                if let Some(s) = existing(host) {
                    return Ok(s);
                }
                logins.insert(host.to_string(), Arc::new(Login::default()));
                None
            }
        }
    };
    if let Some(login) = login {
        let mut outcome = login.outcome.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(o) = outcome.as_ref() {
                return o.clone();
            }
            outcome = login.done.wait(outcome).unwrap_or_else(|e| e.into_inner());
        }
    }
    let mut opening = Opening {
        host,
        outcome: None,
    };
    let outcome = Session::open(host, prompt_handler()).map(Arc::new);
    if let Ok(s) = &outcome {
        SESSIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(HashMap::new)
            .insert(host.to_string(), s.clone());
    }
    opening.outcome = Some(outcome.clone());
    outcome
}

/// The login `for_host` is making: when it is over, however it ends, the
/// host leaves `LOGINS` and whoever waited is given the outcome.
struct Opening<'a> {
    host: &'a str,
    outcome: Option<Result<Arc<Session>, String>>,
}

impl Drop for Opening<'_> {
    fn drop(&mut self) {
        let login = LOGINS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .and_then(|m| m.remove(self.host));
        let outcome = self
            .outcome
            .take()
            .unwrap_or_else(|| Err(format!("could not log in to {}", self.host)));
        if let Some(login) = login {
            *login.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            login.done.notify_all();
        }
    }
}

/// The host's session if one is open, without opening one.
pub fn existing(host: &str) -> Option<Arc<Session>> {
    // Out of the map before it is asked: a session found gone leaves the
    // map, which takes the lock again.
    let s = SESSIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(host))
        .cloned()?;
    s.is_alive().then_some(s)
}

/// Run `command` on the host's session, opened now when there is none,
/// with `limit` as in `Session::run_within`. A session whose ssh went
/// away while it sat idle (the link dropped) is found out when the
/// command cannot be given to it: the command has not run then, so it is
/// given to a new session, once.
pub fn run(host: &str, command: &str, limit: Option<Duration>) -> Output {
    let send = |s: Arc<Session>| s.exchange(command, limit.map(Deadline::after));
    let first = match for_host(host) {
        Ok(s) => send(s),
        Err(e) => return Output::failed(e),
    };
    match first {
        Ok(out) => out,
        Err(why) => {
            log::info(format!("{host}: {why}, logging in again"));
            match for_host(host).map(send) {
                Ok(Ok(out)) => out,
                Ok(Err(why)) | Err(why) => Output::failed(why),
            }
        }
    }
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

/// When a command has to have answered by, and the limit that was, for
/// the words.
#[derive(Clone, Copy)]
struct Deadline {
    at: Instant,
    limit: Duration,
}

impl Deadline {
    fn after(limit: Duration) -> Deadline {
        Deadline {
            at: Instant::now() + limit,
            limit,
        }
    }

    fn left(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }
}

impl Session {
    pub fn open(host: &str, handler: Arc<dyn PromptHandler>) -> Result<Session, String> {
        // No pty on either end: the channel carries bytes as they are.
        // sh, whatever the login shell is, so the commands mean the same
        // everywhere.
        let mut cmd = ssh::Client::new(host)
            .log_level("ERROR")
            .options(["-T"])
            .remote(["sh"])
            .command();
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (mut child, asking) = ssh::spawn_asking(cmd, host, Origin::Session, handler)?;
        let pid = child.id();
        let tail = Arc::new(Mutex::new(ssh::Tail::default()));
        if let Some(err) = child.stderr.take() {
            ssh::line_pipe(host.to_string(), err, Some(tail.clone()), None);
        }
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("ssh has no pipes".into());
        };
        let s = Session {
            host: host.to_string(),
            pid,
            inner: Mutex::new(Inner {
                stdin,
                stdout,
                buf: Vec::new(),
                next: 0,
            }),
            child: Mutex::new(child),
            dead: AtomicBool::new(false),
            closed: AtomicBool::new(false),
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

    /// Whether the session can take a command: not closed or lost, and
    /// its ssh still running (one that went away while the session sat
    /// idle, the link having dropped, makes it lost now).
    pub fn is_alive(&self) -> bool {
        if self.dead.load(Ordering::SeqCst) {
            return false;
        }
        if self.is_gone() {
            self.lost("ssh exited");
            return false;
        }
        true
    }

    /// True once ssh has exited, which reaps it.
    fn is_gone(&self) -> bool {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        !matches!(child.try_wait(), Ok(None))
    }

    /// Send ssh `signal`, unless it has exited (and been reaped) already.
    fn signal(&self, signal: libc::c_int) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(child.try_wait(), Ok(None)) {
            unsafe { libc::kill(self.pid as i32, signal) };
        }
    }

    /// Kill ssh, if it is still there, and reap it.
    fn kill(&self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        // std's Child sends nothing to a process it has reaped.
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Log out. A copy under way is cut short; the next call on this host
    /// opens a new session.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if !self.dead.swap(true, Ordering::SeqCst) {
            log::info(format!("{}: session closed", self.host));
        }
        self.signal(libc::SIGTERM);
    }

    /// The session is no longer usable: the connection dropped, a command
    /// did not answer in time, or a copy stopped half-way and left the
    /// shell waiting. Its ssh is killed and reaped, and it leaves the
    /// sessions, so the next call on the host logs in again.
    fn lost(&self, why: &str) -> String {
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
        self.kill();
        let mut sessions = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(m) = sessions.as_mut() {
            if m.get(&self.host)
                .is_some_and(|s| std::ptr::eq(Arc::as_ptr(s), self))
            {
                m.remove(&self.host);
            }
        }
        format!("{why}{detail}")
    }

    /// Run a command through the shell, its stdin closed, stderr on the
    /// channel with stdout. The whole output is the Output's stdout. In a
    /// subshell, so that an `exit` or a `cd` in it is its own, not the
    /// session's.
    pub fn run(&self, command: &str) -> Output {
        self.exchange(command, None).unwrap_or_else(Output::failed)
    }

    /// `run`, given up when it has not answered within `limit`, counted
    /// from now (a command of another caller still running on the session
    /// counts against it). A command given up leaves the marker protocol
    /// out of step, so the session is lost then, its ssh killed, and the
    /// Output says the command timed out; a session still busy with
    /// another command at the limit is left alone, and the Output says so.
    pub fn run_within(&self, command: &str, limit: Duration) -> Output {
        self.exchange(command, Some(Deadline::after(limit)))
            .unwrap_or_else(Output::failed)
    }

    /// What `run` and `run_within` do. Err when the command never reached
    /// the shell, its ssh being gone already: the one failure after which
    /// giving it to a new session cannot run it twice. Not for a session
    /// that was closed, which is quitting.
    fn exchange(&self, command: &str, deadline: Option<Deadline>) -> Result<Output, String> {
        let Some(mut inner) = self.lock(deadline) else {
            return Ok(Output::failed(format!(
                "{} is busy with another command: {}",
                self.host,
                ssh::timed_out(deadline.map(|d| d.limit).unwrap_or_default())
            )));
        };
        let not_sent = |why: String| {
            if self.closed.load(Ordering::SeqCst) {
                Ok(Output::failed(why))
            } else {
                Err(why)
            }
        };
        if !self.is_alive() {
            return not_sent(format!("the session to {} is gone", self.host));
        }
        let n = inner.next;
        inner.next += 1;
        let script = format!("(\n{command}\n) </dev/null 2>&1\nprintf '\\n{MARK}{n}:%s@@\\n' $?\n");
        if let Err(e) = inner.stdin.write_all(script.as_bytes()) {
            return not_sent(self.lost(&format!("cannot write to the shell: {e}")));
        }
        let mut out = Vec::new();
        Ok(
            match self.until_marker(&mut inner, n, deadline, &mut |chunk| {
                out.extend_from_slice(chunk);
                true
            }) {
                Ok(status) => Output {
                    status,
                    stdout: ssh::as_text(out),
                    stderr: String::new(),
                },
                Err(e) => Output::failed(e),
            },
        )
    }

    /// The channel, once the command before has let go of it; with a
    /// deadline, None when it has not by then.
    fn lock(&self, deadline: Option<Deadline>) -> Option<MutexGuard<'_, Inner>> {
        let Some(d) = deadline else {
            return Some(self.inner.lock().unwrap_or_else(|e| e.into_inner()));
        };
        loop {
            match self.inner.try_lock() {
                Ok(g) => return Some(g),
                Err(TryLockError::Poisoned(p)) => return Some(p.into_inner()),
                Err(TryLockError::WouldBlock) if d.left().is_zero() => return None,
                Err(TryLockError::WouldBlock) => {
                    std::thread::sleep(d.left().min(Duration::from_millis(20)))
                }
            }
        }
    }

    /// Hand the channel's bytes to `sink` up to marker `n`, and say what
    /// status the marker carried. The newline before the marker is the
    /// separator, not output. A command's own output may look like the
    /// start of a marker; one that is not finished by "@@" within a few
    /// bytes is output after all. Past the deadline the session is lost.
    fn until_marker(
        &self,
        inner: &mut Inner,
        n: u64,
        deadline: Option<Deadline>,
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
                                    return Err(self.lost("cancelled"));
                                }
                                inner.buf.drain(..at);
                                from = 0;
                            } else if &inner.buf[body + w..body + w + 3] == b"@@\n" {
                                let word = String::from_utf8_lossy(&inner.buf[body..body + w])
                                    .into_owned();
                                let status = word.parse().unwrap_or(-1);
                                if at > 0 && !sink(&inner.buf[..at]) {
                                    return Err(self.lost("cancelled"));
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
                                return Err(self.lost("cancelled"));
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
                        return Err(self.lost("cancelled"));
                    }
                    inner.buf.drain(..safe);
                    from = 0;
                }
            }
            if let Some(d) = deadline {
                if !readable(&inner.stdout, d) {
                    return Err(self.lost(&ssh::timed_out(d.limit)));
                }
            }
            match inner.stdout.read(&mut chunk) {
                Ok(0) => return Err(self.lost("the connection closed")),
                Ok(k) => inner.buf.extend_from_slice(&chunk[..k]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(self.lost(&format!("the connection dropped: {e}"))),
            }
        }
    }
}

/// Whether the channel has something to read (or has closed) before the
/// deadline.
fn readable(stdout: &ChildStdout, deadline: Deadline) -> bool {
    let mut fd = libc::pollfd {
        fd: stdout.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // In whole milliseconds, rounded up: poll's 0 would not wait.
        let ms = deadline
            .left()
            .as_micros()
            .div_ceil(1000)
            .min(i32::MAX as u128) as i32;
        match unsafe { libc::poll(&mut fd, 1, ms) } {
            0 => return false,
            n if n > 0 => return true,
            _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => {}
            // Whatever is wrong, the read says it.
            _ => return true,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
        self.kill();
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
