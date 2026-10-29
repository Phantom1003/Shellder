//! Running the OpenSSH client: one-off commands, `ssh -G`, the checks on a
//! master's socket, and the master itself with its pty.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::catalog::{self, ResolvedHost};
use super::config;
use super::log;
use super::platform;
use super::prompt::{Origin, PromptHandler};
use super::pty::{self, Pty};
use super::session;

/// The name this machine goes by where a host's would stand: a Console
/// window switched to it reads this machine and opens its shells here,
/// with no ssh in between (`remotefs::remote::shell`, `run_on_terminal`
/// and `term::Terminal` know it). No ssh alias has a blank in it, so no
/// host of the config is taken for it.
pub const LOCAL: &str = "this machine";

pub fn is_local(host: &str) -> bool {
    host == LOCAL
}

/// The OpenSSH binaries: the system's own when it has one at the usual
/// place, otherwise whatever PATH finds.
pub fn binary(name: &str) -> String {
    let fixed = format!("/usr/bin/{name}");
    if std::path::Path::new(&fixed).exists() {
        fixed
    } else {
        name.to_string()
    }
}

/// What every ssh (scp, sftp) the app starts gets: `-F file` only when a
/// test config is in use (normally ssh reads ~/.ssh/config on its own),
/// ControlPersist off, and the `TAG`. A terminal, a session or a copy that
/// finds no master up and becomes one itself, under a `ControlMaster auto`
/// in the user's config, must not fork a background master that outlives
/// it: macOS 26 holds an app as still running after it quit while a
/// process it started is alive, and says so ("Shellder is running in the
/// background"). Without ControlPersist the master goes with its last
/// session, and quitting takes the sessions with it. The tag is what a
/// later run recognises every one of them by (`kill_orphans`) when this
/// one ends without hanging them up.
pub fn base_args() -> Vec<String> {
    let c = config::get();
    let mut args = if c.config_overridden {
        vec!["-F".into(), config::path_str(&c.ssh_config_file)]
    } else {
        vec![]
    };
    args.extend(["-o", "ControlPersist=no", "-o", TAG].map(String::from));
    args
}

#[derive(Debug, Clone, Default)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn text(&self) -> String {
        format!("{}{}", self.stderr, self.stdout).trim().to_string()
    }

    /// The Output of a command that never ran, or was given up: `why` as
    /// its stderr.
    pub fn failed(why: impl Into<String>) -> Output {
        Output {
            status: -1,
            stdout: String::new(),
            stderr: why.into(),
        }
    }
}

/// What a process wrote, as text: the bytes as they are when they are
/// UTF-8, which costs no copy, and each byte that is not replaced when
/// they are not (a file name in Latin-1 loses its accent, not the listing).
pub fn as_text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// What a command given up at its deadline says, in the words a window
/// shows.
pub fn timed_out(limit: Duration) -> String {
    format!("timed out after {limit:?}")
}

fn status_code(s: ExitStatus) -> i32 {
    s.code().unwrap_or(-1)
}

/// Wait for `child`, its stdout and stderr on pipes, and gather what it
/// wrote. With a `deadline`, a child that has not finished by then is
/// stopped (`stop`: the child, or its process group) and reaped, and the
/// Output says so: status -1, and nothing of its stdout, half a listing
/// being no listing. A pipe still held by something it left behind is not
/// waited for past the deadline either.
pub(crate) fn collect(
    mut child: Child,
    deadline: Option<Duration>,
    stop: impl FnOnce(&mut Child),
) -> Output {
    let (tx, rx) = mpsc::channel::<(bool, Vec<u8>)>();
    drain(child.stderr.take(), false, tx.clone());
    let Some(limit) = deadline else {
        drop(tx);
        let mut stdout = Vec::new();
        if let Some(mut p) = child.stdout.take() {
            let _ = p.read_to_end(&mut stdout);
        }
        let status = child.wait().map(status_code).unwrap_or(-1);
        let stderr = rx.recv().map(|(_, b)| b).unwrap_or_default();
        return Output {
            status,
            stdout: as_text(stdout),
            stderr: as_text(stderr),
        };
    };
    let until = Instant::now() + limit;
    drain(child.stdout.take(), true, tx);
    let (mut stdout, mut stderr) = (None, None);
    let mut in_time = true;
    while stdout.is_none() || stderr.is_none() {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok((true, b)) => stdout = Some(b),
            Ok((false, b)) => stderr = Some(b),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                in_time = false;
                break;
            }
        }
    }
    // Both pipes are closed: the process has exited, or is about to.
    let status = loop {
        if !in_time {
            break None;
        }
        match child.try_wait() {
            Ok(Some(s)) => break Some(status_code(s)),
            Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(5)),
            Ok(None) => break None,
            Err(_) => break Some(-1),
        }
    };
    match status {
        Some(status) => Output {
            status,
            stdout: as_text(stdout.unwrap_or_default()),
            stderr: as_text(stderr.unwrap_or_default()),
        },
        None => {
            stop(&mut child);
            let _ = child.wait();
            Output::failed(timed_out(limit))
        }
    }
}

/// Read a pipe to its end on a thread of its own, and send what it held.
fn drain<R: Read + Send + 'static>(
    pipe: Option<R>,
    is_stdout: bool,
    tx: mpsc::Sender<(bool, Vec<u8>)>,
) {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut bytes);
        }
        let _ = tx.send((is_stdout, bytes));
    });
}

/// Run ssh (or scp) to completion, capturing output. `on_start` sees the
/// child's pid as soon as it runs, for a caller that may have to stop it (a
/// copy the user cancels).
pub fn run_binary(bin: &str, args: &[String], on_start: Option<&mut dyn FnMut(u32)>) -> Output {
    run_binary_fed(bin, args, None, on_start, None, None)
}

/// `run_binary` with something on the child's stdin: the batch an sftp
/// reads its commands from. Without `input` stdin is closed. With `asking`
/// the process gets an asking pty under that host's name: it logs in on
/// its own, and its questions are answered like a master's. With
/// `deadline` it is killed when it has not finished by then (`collect`):
/// ssh hangs up, and the remote command goes with the connection.
fn run_binary_fed(
    bin: &str,
    args: &[String],
    input: Option<String>,
    on_start: Option<&mut dyn FnMut(u32)>,
    asking: Option<&str>,
    deadline: Option<Duration>,
) -> Output {
    let mut cmd = Command::new(bin);
    cmd.args(base_args())
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The asking pty's slave side lives as long as the child does.
    let (mut child, _slave) = match asking {
        Some(host) => match spawn_asking(cmd, host, Origin::Session, session::prompt_handler()) {
            Ok((c, s)) => (c, Some(s)),
            Err(e) => return Output::failed(e),
        },
        None => match cmd.spawn() {
            Ok(c) => (c, None),
            Err(e) => return Output::failed(e.to_string()),
        },
    };
    if let Some(f) = on_start {
        f(child.id());
    }
    if let (Some(text), Some(mut pipe)) = (input, child.stdin.take()) {
        // On its own thread: a child that exits before reading must not
        // hang the write. The pipe closes when the thread is done with it.
        thread::spawn(move || {
            use std::io::Write;
            let _ = pipe.write_all(text.as_bytes());
        });
    }
    collect(child, deadline, |c| {
        let _ = c.kill();
    })
}

/// Run ssh with these arguments to completion. A control command (`-O
/// check`, `exit`, `forward`, `cancel`) gets `config::CONTROL_DEADLINE`:
/// it talks to a master over its socket, and a master that was stopped
/// accepts the connection and never answers, which would hold whoever
/// asked (the daemon's one thread among them) for good.
pub fn run(args: &[&str]) -> Output {
    let deadline = (args.first() == Some(&"-O")).then_some(config::CONTROL_DEADLINE);
    run_until(args, deadline)
}

/// `run`, ssh killed when it has not finished within `limit`: status -1
/// and a stderr that says it timed out.
pub fn run_within(args: &[&str], limit: Duration) -> Output {
    run_until(args, Some(limit))
}

fn run_until(args: &[&str], deadline: Option<Duration>) -> Output {
    let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    run_binary_fed(&binary("ssh"), &v, None, None, None, deadline)
}

/// `run_until` with `input` on ssh's stdin: a script for a remote `sh`,
/// which the login shell then never parses.
pub fn run_fed(args: &[&str], input: String, deadline: Option<Duration>) -> Output {
    let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    run_binary_fed(&binary("ssh"), &v, Some(input), None, None, deadline)
}

/// Resolved client configuration for `host` (`ssh -G`): lower-cased keys,
/// every value in order of appearance (identityfile repeats).
pub fn config(host: &str) -> Result<HashMap<String, Vec<String>>, String> {
    let r = run(&["-G", host]);
    if r.status != 0 {
        let msg = r.stderr.trim();
        return Err(if msg.is_empty() {
            format!("ssh -G {host} failed (rc {})", r.status)
        } else {
            msg.to_string()
        });
    }
    let mut cfg: HashMap<String, Vec<String>> = HashMap::new();
    for line in r.stdout.lines() {
        let mut parts = line.splitn(2, ' ');
        let Some(k) = parts.next() else { continue };
        cfg.entry(k.to_lowercase())
            .or_default()
            .push(parts.next().unwrap_or("").to_string());
    }
    Ok(cfg)
}

/// An `ssh -G` answer, with the dates of the config files it was read from.
struct Resolution {
    at: Instant,
    files: HashMap<String, Option<SystemTime>>,
    resolved: ResolvedHost,
}

static RESOLVED: Mutex<Option<HashMap<String, Resolution>>> = Mutex::new(None);

/// What `ssh -G` resolves the host to, asked now. The answer is kept for
/// `resolve_cached`.
pub fn resolve(host: &str) -> Result<ResolvedHost, String> {
    // The dates before ssh reads the files: a change made while it runs
    // then shows as one.
    let files = catalog::modification_dates(&catalog::files_read_by_ssh());
    let resolved = ResolvedHost::from_config(&config(host)?);
    RESOLVED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(
            host.to_string(),
            Resolution {
                at: Instant::now(),
                files,
                resolved: resolved.clone(),
            },
        );
    Ok(resolved)
}

/// `resolve`, answered from the last `ssh -G` while that is recent
/// (`config::RESOLVE_TTL`) and no file it read has changed since: what a
/// terminal, a session, an action and the master check want on every
/// start, for the keepalive and the ControlPath.
pub fn resolve_cached(host: &str) -> Result<ResolvedHost, String> {
    {
        let guard = RESOLVED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = guard.as_ref().and_then(|m| m.get(host)) {
            if r.at.elapsed() < config::RESOLVE_TTL {
                let files: Vec<String> = r.files.keys().cloned().collect();
                if catalog::modification_dates(&files) == r.files {
                    return Ok(r.resolved.clone());
                }
            }
        }
    }
    resolve(host)
}

/// ControlPath from the user's ssh config, None when unset.
pub fn control_path(host: &str) -> Result<Option<String>, String> {
    Ok(resolve(host)?.control_path)
}

/// Whether a master answers on the host's socket (`ssh -O check`), asked
/// now. The answer also serves `master_reusable` for a moment.
pub fn master_alive(host: &str) -> bool {
    let up = run(&["-O", "check", host]).status == 0;
    remember_master(host, up);
    up
}

/// The last answers `master_reusable` and `master_alive` got, by host,
/// with when each was given.
static REUSABLE: Mutex<Option<HashMap<String, (Instant, bool)>>> = Mutex::new(None);

fn remember_master(host: &str, up: bool) {
    REUSABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(host.to_string(), (Instant::now(), up));
}

/// Forget what was last found about the host's master: a command through
/// it failed, so the next one asks again.
pub fn forget_master(host: &str) {
    if let Some(m) = REUSABLE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(host);
    }
}

/// Whether a command for the host can go through its master, without a
/// login: one with a socket is up (so never on a platform without
/// ControlMaster, and never for a host without a ControlPath). The
/// listings and a copy's progress ask before every command, so the answer
/// is kept for `config::MASTER_CHECK_TTL`, and with no socket file there
/// is nothing to ask.
pub fn master_reusable(host: &str) -> bool {
    if !platform::control_master() {
        return false;
    }
    let recent = REUSABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(host).copied());
    if let Some((at, up)) = recent {
        if at.elapsed() < config::MASTER_CHECK_TTL {
            return up;
        }
    }
    if !socket_exists(host) {
        remember_master(host, false);
        return false;
    }
    master_alive(host)
}

/// Whether the host's ControlPath is there as a file. A config ssh cannot
/// read has no socket either.
fn socket_exists(host: &str) -> bool {
    resolve_cached(host)
        .ok()
        .and_then(|r| r.control_path)
        .is_some_and(|cp| std::path::Path::new(&config::expand_tilde(&cp)).exists())
}

/// pid of the master owning the host's socket ("Master running (pid=N)").
pub fn master_pid(host: &str) -> Option<u32> {
    static PID: OnceLock<Regex> = OnceLock::new();
    let r = run(&["-O", "check", host]);
    if r.status != 0 {
        return None;
    }
    let re = PID.get_or_init(|| Regex::new("pid=(\\d+)").expect("a valid pattern"));
    re.captures(&r.stderr)?.get(1)?.as_str().parse().ok()
}

/// Marker on every ssh (scp, sftp) the app spawns, through `base_args`
/// (`ssh -o Tag=shellder`, visible in ps), so a later shellder can
/// recognise one left behind by an instance that ended without hanging
/// its processes up (a kill -9, a crash, a `cargo tauri dev` rebuild).
/// It says nothing about what the process is: a master carries `-M`
/// besides when it owns a socket.
pub const TAG: &str = "Tag=shellder";

fn ps(args: &[&str]) -> String {
    Command::new("ps")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// A tagged ssh of an earlier run, as `orphans_in` reads it from ps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub pid: i32,
    /// `-M` on its command line: a master owning a socket. Every other
    /// tagged ssh (a terminal, a session, a copy, a keepalive master
    /// without a socket) is a client to hang up.
    pub master: bool,
    pub command: String,
}

/// True when the command line has `-M` as an argument of its own.
fn has_master_flag(command: &str) -> bool {
    command.split(' ').any(|a| a == "-M")
}

/// The tagged ssh processes whose parent is now init (ppid 1), from the
/// output of `ps -axo pid=,ppid=,command=`. A process of the running
/// app, or of a running earlier one, keeps its parent and is left alone.
pub fn orphans_in(ps_output: &str) -> Vec<Orphan> {
    let mut found = vec![];
    for line in ps_output.lines() {
        if !line.contains(TAG) {
            continue;
        }
        let mut f = line.split_whitespace();
        let (Some(pid), Some(ppid)) = (
            f.next().and_then(|s| s.parse::<i32>().ok()),
            f.next().and_then(|s| s.parse::<i32>().ok()),
        ) else {
            continue;
        };
        if ppid != 1 {
            continue;
        }
        let command = f.collect::<Vec<_>>().join(" ");
        found.push(Orphan {
            pid,
            master: has_master_flag(&command),
            command,
        });
    }
    found
}

/// Kill every ssh left behind by an earlier shellder that did not shut
/// down cleanly: the tagged processes whose parent is now init (ppid 1).
/// A master is terminated as before (one that already owns a socket is
/// also replaced by the daemon; this catches the rest, e.g. one still
/// stuck in authentication). A terminal, a session or a copy is hung up,
/// what closing its window would have done: the pty hangup never reaches
/// it once the app that held its pty is gone, and a remote shell on a
/// pty survives the end of its input, so left alone it waits for ever,
/// holding a session on the user's own master (whose MaxSessions it
/// fills) or a login of its own.
pub fn kill_orphans() {
    for o in orphans_in(&ps(&["-axo", "pid=,ppid=,command="])) {
        let (what, signal) = if o.master {
            ("master", libc::SIGTERM)
        } else {
            ("ssh (a terminal, session or copy)", libc::SIGHUP)
        };
        log::warn(format!(
            "orphaned shellder {what} pid {} from an earlier run, {}: {}",
            o.pid,
            if o.master { "killing" } else { "hanging up" },
            o.command.chars().take(80).collect::<String>()
        ));
        unsafe { libc::kill(o.pid, signal) };
    }
}

/// True when `pid`, the owner of a host's socket, is an ssh started by
/// shellder that the daemon may replace: a master of ours (`-M`), or any
/// tagged ssh of a run that is over (ppid 1, which the sweep at start
/// hangs up as well). A tagged ssh of this run that is not a master, a
/// Console's terminal that became the socket's master under a
/// `ControlMaster auto` in the user's config, is somebody's open window
/// and is left alone like any other master.
pub fn is_our_master(pid: u32) -> bool {
    let out = ps(&["-o", "ppid=,command=", "-p", &pid.to_string()]);
    let line = out.trim();
    if !line.contains(TAG) {
        return false;
    }
    let (ppid, command) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    ppid == "1" || has_master_flag(command)
}

/// What the master does to stay connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdleMode {
    /// `ssh -N`: no session at all. Cleanest, but some servers close it.
    None,
    /// An idle login shell on a pty: looks exactly like a person who left
    /// a terminal open (`w` shows an idle login). sshd sends it SIGHUP
    /// when the connection drops, so nothing is left behind.
    Shell,
    /// `cat` blocked on a stdin pipe held open locally: no pty, no login
    /// scripts, exits on EOF when the connection drops.
    Cat,
}

impl IdleMode {
    pub const ALL: [IdleMode; 3] = [IdleMode::None, IdleMode::Shell, IdleMode::Cat];

    pub fn raw(self) -> &'static str {
        match self {
            IdleMode::None => "none",
            IdleMode::Shell => "shell",
            IdleMode::Cat => "cat",
        }
    }

    pub fn parse(s: &str) -> Option<IdleMode> {
        IdleMode::ALL.into_iter().find(|m| m.raw() == s)
    }

    /// For the log.
    pub fn title(self) -> &'static str {
        match self {
            IdleMode::None => "no session (-N)",
            IdleMode::Shell => "idle shell (pty)",
            IdleMode::Cat => "cat on stdin",
        }
    }
}

/// Detect a dead link so the process can be restarted; only applied when
/// the user's config does not set its own value.
pub fn keepalive_args(resolved: &ResolvedHost) -> Vec<String> {
    if resolved.server_alive_interval != 0 {
        return vec![];
    }
    [
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
    ]
    .map(String::from)
    .to_vec()
}

/// Whether the host's master owns a socket other ssh commands reuse: a
/// ControlPath in its config, on a platform whose ssh has ControlMaster.
/// Without one the connection is still kept alive — for the port forwards
/// its config asks for, and to be there — but only the app knows it is up.
pub fn multiplexed(resolved: &ResolvedHost) -> bool {
    platform::control_master() && resolved.control_path.is_some()
}

/// Arguments that only concern the master process itself. Everything else
/// (HostName, User, ProxyJump, ControlPath, …) comes from ~/.ssh/config.
pub fn master_args(host: &str, resolved: &ResolvedHost, mode: IdleMode) -> Vec<String> {
    // The TAG comes with base_args, as on every ssh of the app.
    let client = Client::resolved(host, resolved);
    let client = if multiplexed(resolved) {
        // ControlPersist is off in base_args: we own the process.
        client.log_level("ERROR").options(["-M"])
    } else {
        // Without a socket to check, "Authenticated to host" on stderr
        // (a VERBOSE line) is how the daemon learns the connection is up.
        client.log_level("VERBOSE")
    };
    match mode {
        IdleMode::None => client.options(["-N"]),
        // Force a remote pty even though stdin is a pipe; the login shell
        // then sits idle reading from it. X11 is left to the multiplexed
        // sessions, which request their own forwarding.
        IdleMode::Shell => client.options(["-tt", "-o", "ForwardX11=no"]),
        IdleMode::Cat => client
            .options(["-T", "-o", "ForwardX11=no"])
            .remote(["cat"]),
    }
    .args()
}

/// The ssh client for a host, spelled the same way by everything that
/// starts one (the master, a session, a terminal, an action run on a
/// terminal): one password prompt per attempt, so that a stored secret the
/// server rejects is not offered again and again; the LogLevel that kind
/// of ssh wants, when it wants one; the keepalive, unless the user's
/// config sets its own; then that kind's own options, the host, and what
/// runs there.
#[derive(Debug, Clone)]
pub struct Client {
    host: String,
    log_level: Option<&'static str>,
    keepalive: Vec<String>,
    options: Vec<String>,
    remote: Vec<String>,
}

impl Client {
    /// For `host`, its configuration resolved (`resolve_cached`) for the
    /// keepalive. When ssh cannot resolve it the keepalive is ours, and the
    /// log says why: the ssh that follows will most likely fail the same
    /// way, and its error is the one shown.
    pub fn new(host: &str) -> Client {
        match resolve_cached(host) {
            Ok(r) => Client::resolved(host, &r),
            Err(e) => {
                log::warn(format!(
                    "{host}: ssh -G failed ({e}), keeping the connection alive the default way"
                ));
                Client::resolved(host, &ResolvedHost::default())
            }
        }
    }

    /// For `host`, as already resolved.
    pub fn resolved(host: &str, resolved: &ResolvedHost) -> Client {
        Client {
            host: host.to_string(),
            log_level: None,
            keepalive: keepalive_args(resolved),
            options: vec![],
            remote: vec![],
        }
    }

    /// `-o LogLevel=level`; without it the user's config decides.
    pub fn log_level(mut self, level: &'static str) -> Client {
        self.log_level = Some(level);
        self
    }

    /// Options of this kind of ssh (`-T`, `-tt`, `-M`, `-o Key=value`),
    /// after the ones every client gets.
    pub fn options<S: Into<String>>(mut self, options: impl IntoIterator<Item = S>) -> Client {
        self.options.extend(options.into_iter().map(Into::into));
        self
    }

    /// What runs on the host, after its name: nothing is the login shell.
    pub fn remote<S: Into<String>>(mut self, words: impl IntoIterator<Item = S>) -> Client {
        self.remote.extend(words.into_iter().map(Into::into));
        self
    }

    /// The arguments, `base_args` left out.
    pub fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec!["-o".into(), "NumberOfPasswordPrompts=1".into()];
        if let Some(level) = self.log_level {
            args.extend(["-o".into(), format!("LogLevel={level}")]);
        }
        args.extend(self.keepalive.iter().cloned());
        args.extend(self.options.iter().cloned());
        args.push(self.host.clone());
        args.extend(self.remote.iter().cloned());
        args
    }

    /// The ssh to spawn: the binary, `base_args`, then `args`.
    pub fn command(&self) -> Command {
        let mut c = Command::new(binary("ssh"));
        c.args(base_args()).args(self.args());
        c
    }
}

/// Last few KB of a master's stderr: kept so the daemon can tell *why* ssh
/// exited.
#[derive(Default)]
pub struct Tail {
    buf: String,
}

impl Tail {
    fn append(&mut self, s: &str) {
        self.buf.push_str(s);
        if self.buf.len() > 4096 {
            let cut = self.buf.len() - 4096;
            let cut = (cut..self.buf.len())
                .find(|&i| self.buf.is_char_boundary(i))
                .unwrap_or(0);
            self.buf.drain(..cut);
        }
    }

    /// Last non-empty line, trimmed, leaving out what says nothing about
    /// why: the X11 warning, and the lines VERBOSE prints on its own.
    pub fn last_line(&self) -> Option<String> {
        self.buf
            .lines()
            .map(str::trim)
            .rfind(|l| {
                !l.is_empty()
                    && !l.starts_with("Warning: untrusted X11")
                    && !l.starts_with("Transferred:")
                    && !l.starts_with("Bytes per second:")
                    && !l.starts_with("Authenticated to")
            })
            .map(String::from)
    }
}

/// A master process: the child, its pty, and what it said. Dropping it
/// closes the pty's slave side, which ends the reader thread.
pub struct MasterProc {
    child: Child,
    pub pid: u32,
    pub tail: Arc<Mutex<Tail>>,
    /// ssh said "Authenticated to host": the connection is up. What a
    /// master without a socket has instead of `ssh -O check`.
    authenticated: Arc<AtomicBool>,
    exit: Option<ExitStatus>,
    _slave: OwnedFd,
}

impl MasterProc {
    pub fn authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }

    pub fn is_running(&mut self) -> bool {
        if self.exit.is_some() {
            return false;
        }
        match self.child.try_wait() {
            Ok(Some(s)) => {
                self.exit = Some(s);
                false
            }
            Ok(None) => true,
            Err(_) => false,
        }
    }

    pub fn exit_code(&mut self) -> i32 {
        self.is_running();
        self.exit.map(status_code).unwrap_or(-1)
    }

    pub fn terminate(&self) {
        unsafe { libc::kill(self.pid as i32, libc::SIGTERM) };
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn last_line(&self) -> Option<String> {
        self.tail.lock().ok()?.last_line()
    }
}

/// A pipe whose reader logs every complete line as "host: line". A line
/// starting with `watch`'s prefix sets its flag (the "Authenticated to
/// host" of a master without a socket; the hostname keeps a ProxyJump
/// hop's own line from counting).
pub(crate) fn line_pipe<R: Read + Send + 'static>(
    host: String,
    pipe: R,
    tail: Option<Arc<Mutex<Tail>>>,
    watch: Option<(String, Arc<AtomicBool>)>,
) {
    thread::Builder::new()
        .name(format!("ssh-out-{host}"))
        .spawn(move || {
            let mut reader = BufReader::new(pipe);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                // Lines end in \n or \r (a pty's progress lines); read_until
                // on \n and split on \r afterwards.
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&buf).into_owned();
                if let Some(t) = &tail {
                    if let Ok(mut t) = t.lock() {
                        t.append(&text);
                    }
                }
                for line in text.split(['\n', '\r']) {
                    let line = line.trim();
                    if !line.is_empty() {
                        log::ssh(&host, line);
                        if let Some((prefix, flag)) = &watch {
                            if line.starts_with(prefix.as_str()) {
                                flag.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                }
            }
        })
        .expect("thread");
}

/// Start `cmd` with a pty of its own as its controlling terminal and
/// nothing else, read by a thread that answers its questions through
/// `handler`. The slave side comes back with the child and must be held for
/// as long as the child lives: closing it is what ends the reader.
pub fn spawn_asking(
    cmd: Command,
    host: &str,
    origin: Origin,
    handler: Arc<dyn PromptHandler>,
) -> Result<(Child, OwnedFd), String> {
    let (child, slave, _) = spawn_on_pty(cmd, host, origin, handler, false, None)?;
    Ok((child, slave))
}

/// `spawn_asking`, and with `terminal` the pty is the child's stdin and
/// stdout as well (sized like a small terminal: ssh gives the remote pty
/// the size it finds there), so that what the process prints and what it
/// is asked on it both come down the pty; the reader puts the former in
/// `said`. The reader's thread comes back with the child, for a caller
/// that wants `said` complete.
fn spawn_on_pty(
    mut cmd: Command,
    host: &str,
    origin: Origin,
    handler: Arc<dyn PromptHandler>,
    terminal: bool,
    said: Option<Arc<Mutex<String>>>,
) -> Result<(Child, OwnedFd, thread::JoinHandle<()>), String> {
    let pty = Pty::open()?;
    // Questions come down the pty, never through an askpass program: a
    // helper the user's environment points at would answer in our place.
    cmd.env_remove("SSH_ASKPASS")
        .env_remove("SSH_ASKPASS_REQUIRE");
    if terminal {
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe { libc::ioctl(pty.slave.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        // The copies Command keeps go when it is dropped below; ours is
        // the `slave` handed back.
        let dup = || {
            pty.slave
                .try_clone()
                .map(Stdio::from)
                .map_err(|e| e.to_string())
        };
        cmd.stdin(dup()?).stdout(dup()?);
    }
    pty.attach(&mut cmd);
    let child = cmd
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", cmd.get_program().to_string_lossy()))?;
    let pid = child.id();
    let Pty { master, slave } = pty;
    let h = host.to_string();
    let reader = thread::Builder::new()
        .name(format!("ssh-pty-{host}"))
        .spawn(move || pty::serve(h, pid, origin, master, handler, said))
        .map_err(|e| e.to_string())?;
    Ok((child, slave, reader))
}

/// Run `command` on the host on a terminal: ssh's standard streams are
/// its asking pty and the remote command gets a pty too (`-tt`), so a
/// question the host itself asks on it (sudo's, polkit's text agent's)
/// comes to `handler` like one of ssh's own, and what the command prints
/// comes back as the output, the questions and their context left out.
/// Through the master when one is up, on a login of its own otherwise;
/// `-e none`, so that no answer is taken for ssh's escape. On this
/// machine (`LOCAL`) it is `sh -c command` on the same pty: sudo's
/// question here is answered the same way.
pub fn run_on_terminal(
    host: &str,
    command: &str,
    origin: Origin,
    handler: Arc<dyn PromptHandler>,
) -> Output {
    let mut cmd = if is_local(host) {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", command]);
        c
    } else {
        Client::new(host)
            .log_level("ERROR")
            .options(["-tt", "-e", "none"])
            .remote([command])
            .command()
    };
    // No colours, no cursor movement: the output is read, not drawn.
    cmd.env("TERM", "dumb").stderr(Stdio::piped());
    let said = Arc::new(Mutex::new(String::new()));
    let (mut child, slave, reader) =
        match spawn_on_pty(cmd, host, origin, handler, true, Some(said.clone())) {
            Ok(x) => x,
            Err(e) => return Output::failed(e),
        };
    let mut err_pipe = child.stderr.take();
    let err_thread = thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut bytes);
        }
        as_text(bytes)
    });
    let status = child.wait().map(status_code).unwrap_or(-1);
    // The process is gone: let go of the slave, which hangs the reader up
    // once it has read the last of the output.
    drop(slave);
    let _ = reader.join();
    let stderr = err_thread.join().unwrap_or_default();
    let stdout = said.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Output {
        status,
        stdout,
        stderr,
    }
}

/// Spawn `ssh -M host` (with -N, an idle shell or cat) with a pty of its own
/// as the controlling terminal, read by a thread that answers its questions
/// through `handler`.
pub fn spawn_master(
    host: &str,
    resolved: &ResolvedHost,
    mode: IdleMode,
    handler: Arc<dyn PromptHandler>,
) -> Result<MasterProc, String> {
    let socket = multiplexed(resolved);
    if socket {
        let cp = resolved.control_path.as_deref().unwrap_or_default();
        let cp_path = std::path::Path::new(cp);
        if let Some(dir) = cp_path.parent() {
            let _ = std::fs::create_dir_all(dir);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        if cp_path.exists() && !master_alive(host) {
            log::info(format!("{host}: removing stale socket {cp}"));
            if let Err(e) = std::fs::remove_file(cp_path) {
                return Err(format!("cannot remove stale socket {cp}: {e}"));
            }
        }
    }
    let mut cmd = Command::new(binary("ssh"));
    cmd.args(base_args())
        .args(master_args(host, resolved, mode));
    // The idle session's stdin must stay open (never written to) so the
    // remote shell/cat keeps waiting; the pipe lives as long as the child.
    cmd.stdin(if mode == IdleMode::None {
        Stdio::null()
    } else {
        Stdio::piped()
    });
    // A login shell prints motd/profile chatter on the remote pty: keep
    // that out of the log. ssh's own diagnostics go to stderr, which is
    // also kept for the daemon to inspect.
    cmd.stdout(if mode == IdleMode::Shell {
        Stdio::null()
    } else {
        Stdio::piped()
    });
    cmd.stderr(Stdio::piped());
    let (mut child, slave) = spawn_asking(cmd, host, Origin::Master, handler)?;
    let pid = child.id();
    let tail = Arc::new(Mutex::new(Tail::default()));
    let authenticated = Arc::new(AtomicBool::new(false));
    if let Some(out) = child.stdout.take() {
        line_pipe(host.to_string(), out, None, None);
    }
    if let Some(err) = child.stderr.take() {
        let watch = (!socket).then(|| {
            (
                format!("Authenticated to {} ", resolved.hostname),
                authenticated.clone(),
            )
        });
        line_pipe(host.to_string(), err, Some(tail.clone()), watch);
    }
    log::info(format!(
        "{host}: spawned master pid {pid} [{}] ({})",
        mode.title(),
        match &resolved.control_path {
            Some(cp) if socket => format!("socket {cp}"),
            Some(_) => "no ControlMaster on this platform, kept alive without a socket".into(),
            None => "no ControlPath, kept alive without a socket".into(),
        }
    ));
    Ok(MasterProc {
        child,
        pid,
        tail,
        authenticated,
        exit: None,
        _slave: slave,
    })
}

/// Where the Files window opens the host's tree when it is there.
pub const UPLOAD_DIRECTORY: &str = "Shellder";

/// Which side of a copy: this machine or a host whose master is up.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "host", rename_all = "lowercase")]
pub enum FileSource {
    Local,
    Host(String),
}

impl FileSource {
    pub fn is_local(&self) -> bool {
        matches!(self, FileSource::Local)
    }
    pub fn host(&self) -> Option<&str> {
        match self {
            FileSource::Host(h) => Some(h),
            FileSource::Local => None,
        }
    }
    /// Stable across panes, which is how a drag says where it came from.
    pub fn id(&self) -> String {
        match self {
            FileSource::Local => "local".into(),
            FileSource::Host(h) => format!("host:{h}"),
        }
    }
    pub fn from_id(id: &str) -> FileSource {
        match id.strip_prefix("host:") {
            Some(h) => FileSource::Host(h.to_string()),
            None => FileSource::Local,
        }
    }
    pub fn title(&self) -> String {
        match self {
            FileSource::Local => "Local".into(),
            FileSource::Host(h) => h.clone(),
        }
    }
}

/// Copy paths from one side into a directory on the other. To or from this
/// machine it is sftp reading a batch, which can `resume`: a file the
/// destination already has part of is continued from where it stopped,
/// one it has whole is left alone. Between two hosts `-3` sends the bytes
/// through this machine, so both ends stay on their own master and neither
/// needs to reach the other; scp has no resume, so that copy starts over.
/// Through masters it is BatchMode, so a master that went away fails at
/// once instead of waiting for an answer nobody will give; a host without
/// one up makes the copy log in on its own, with an asking pty for its
/// questions. -q leaves the meter out and keeps the log to real errors.
pub fn copy(
    paths: &[String],
    from: &FileSource,
    directory: &str,
    to: &FileSource,
    resume: bool,
    on_start: Option<&mut dyn FnMut(u32)>,
) -> Output {
    fn there(source: &FileSource, path: &str) -> String {
        match source.host() {
            Some(h) => format!("{h}:{path}"),
            None => path.to_string(),
        }
    }
    if from == to {
        return Output::failed("a side does not copy into itself");
    }
    // sftp puts a copy into "" as the remote home, which is not where the
    // window asked whether the name was taken.
    if directory.is_empty() {
        return Output::failed("there is no folder to copy into yet");
    }
    let login = copy_logs_in(from, to);
    // sftp turns BatchMode on for a batch file (-b), which would skip the
    // questions a login needs; ssh takes the first value an option is
    // given, and ours comes first.
    let mode: &[&str] = if login {
        &["-o", "BatchMode=no", "-o", "NumberOfPasswordPrompts=1"]
    } else {
        &["-o", "BatchMode=yes"]
    };
    let mut args: Vec<String> = Vec::new();
    let (host, verb) = match (from.host(), to.host()) {
        (Some(a), Some(_)) => {
            args.push("-3".into());
            args.extend(["-r", "-p", "-q"].map(String::from));
            args.extend(mode.iter().map(|a| a.to_string()));
            // scp globs a remote source: report[1].pdf would be report1.pdf.
            args.extend(paths.iter().map(|p| there(from, &glob_escape(p))));
            args.push(there(to, directory) + "/");
            // Both hosts ask on the one pty; the prompt's own user@host
            // says which credentials each wants.
            return run_binary_fed(
                &binary("scp"),
                &args,
                None,
                on_start,
                login.then_some(a),
                None,
            );
        }
        (Some(h), None) => (h, "get"),
        (None, Some(h)) => (h, "put"),
        (None, None) => unreachable!("from == to was checked"),
    };
    if resume {
        args.push("-a".into());
    }
    args.extend(["-r", "-p", "-q"].map(String::from));
    args.extend(mode.iter().map(|a| a.to_string()));
    args.extend(["-b", "-"].map(String::from));
    args.push(host.to_string());
    let batch = sftp_batch(verb, paths, directory);
    run_binary_fed(
        &binary("sftp"),
        &args,
        Some(batch),
        on_start,
        login.then_some(host),
        None,
    )
}

/// A copy with a host side whose master is not up logs in on its own.
pub fn copy_logs_in(from: &FileSource, to: &FileSource) -> bool {
    [from, to]
        .into_iter()
        .filter_map(|s| s.host())
        .any(|h| !master_reusable(h))
}

/// The lines an sftp copy reads: one `put` or `get` per path, into
/// `directory` spelled with a slash at its end. Without it sftp takes a
/// directory that is not there (deleted while the copy waited, or a root
/// that never was one) for the name to write, and a file there for one to
/// write over: the copy lands as a file named like the folder, and exits 0.
/// With it, it fails.
pub fn sftp_batch(verb: &str, paths: &[String], directory: &str) -> String {
    let into = if directory.ends_with('/') {
        directory.to_string()
    } else {
        format!("{directory}/")
    };
    paths
        .iter()
        .map(|p| format!("{verb} {} {}\n", sftp_word(p), sftp_word(&into)))
        .collect()
}

/// A path as scp takes a remote source: its glob characters escaped, so
/// that it names the one file and no others.
pub fn glob_escape(path: &str) -> String {
    let mut w = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            w.push('\\');
        }
        w.push(c);
    }
    w
}

/// A path as one word of an sftp batch line: double quotes, with the quote
/// and the backslash escaped, are what its tokenizer takes.
pub fn sftp_word(path: &str) -> String {
    let mut w = String::with_capacity(path.len() + 2);
    w.push('"');
    for c in path.chars() {
        if c == '"' || c == '\\' {
            w.push('\\');
        }
        w.push(c);
    }
    w.push('"');
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "\
  101     1 /usr/bin/ssh -o ControlPersist=no -o Tag=shellder -o NumberOfPasswordPrompts=1 -M -o LogLevel=ERROR -tt -o ForwardX11=no work
  102     1 /usr/bin/ssh -o ControlPersist=no -o Tag=shellder -tt -o LogLevel=ERROR -t work sh -c 'echo -M; exec bash'
  103     1 /usr/bin/ssh -o ControlPersist=no -o Tag=shellder -T work sh
  104     1 /usr/bin/sftp -o ControlPersist=no -o Tag=shellder -b - work
  105  4242 /usr/bin/ssh -o ControlPersist=no -o Tag=shellder -M -N work
  106     1 /usr/bin/ssh -o ControlMaster=auto work
  107     1 ssh -o Tag=shellder-not -N work
   xx     1 /usr/bin/ssh -o Tag=shellder -N work
";

    #[test]
    fn orphans_are_the_tagged_children_of_init() {
        let found = orphans_in(PS);
        let pids: Vec<i32> = found.iter().map(|o| o.pid).collect();
        // 105 still has its parent, 106 is not ours, the last line has no pid.
        assert_eq!(pids, [101, 102, 103, 104, 107]);
        assert!(found[0].master, "-M makes a master");
        assert!(
            !found[1].master,
            "-M inside the remote command is not the flag"
        );
        assert!(!found[2].master);
        assert!(!found[3].master, "a copy is a client");
        assert!(found[0]
            .command
            .starts_with("/usr/bin/ssh -o ControlPersist=no"));
    }

    #[test]
    fn tag_is_on_every_ssh() {
        let args = base_args();
        assert!(args.windows(2).any(|w| w == ["-o", TAG]));
        assert!(args.windows(2).any(|w| w == ["-o", "ControlPersist=no"]));
    }

    fn sh(script: &str) -> Child {
        Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// Output that is not UTF-8 (a file name in Latin-1) comes back with
    /// the odd byte replaced, not as nothing, and with its status.
    #[test]
    fn output_that_is_not_utf8_is_kept() {
        let script = "printf 'caf\\351\\n'; printf 'r\\351' >&2; exit 4";
        for deadline in [None, Some(Duration::from_secs(10))] {
            let out = collect(sh(script), deadline, |c| {
                let _ = c.kill();
            });
            assert_eq!(out.status, 4, "{deadline:?}");
            assert_eq!(out.stdout, "caf\u{fffd}\n", "{deadline:?}");
            assert_eq!(out.stderr, "r\u{fffd}", "{deadline:?}");
        }
        assert_eq!(as_text(b"plain".to_vec()), "plain");
    }

    /// A command past its deadline is killed and says so, and what it left
    /// behind holding its pipes is not waited for; one that ends in time is
    /// not concerned.
    #[test]
    fn a_deadline_stops_what_does_not_finish() {
        let kill = |c: &mut Child| {
            let _ = c.kill();
        };
        let out = collect(sh("echo quick"), Some(Duration::from_secs(10)), kill);
        assert_eq!((out.status, out.stdout.as_str()), (0, "quick\n"));
        let t = Instant::now();
        let out = collect(
            sh("echo half; sleep 3 & sleep 3"),
            Some(Duration::from_millis(300)),
            kill,
        );
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
        assert_eq!(out.status, -1);
        assert!(out.stdout.is_empty(), "half an output is none");
        assert!(out.stderr.contains("timed out"), "{}", out.stderr);
    }

    /// Every client, a master's too, is spelled the same way: one password
    /// prompt, the LogLevel asked for, our keepalive unless the user's
    /// config has its own, then its own options, the host, the command.
    #[test]
    fn every_client_is_spelled_the_same_way() {
        let args = Client::resolved("work", &ResolvedHost::default())
            .log_level("ERROR")
            .options(["-T"])
            .remote(["sh"])
            .args();
        assert_eq!(
            args,
            [
                "-o",
                "NumberOfPasswordPrompts=1",
                "-o",
                "LogLevel=ERROR",
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "-T",
                "work",
                "sh"
            ]
        );
        let own = ResolvedHost {
            server_alive_interval: 30,
            ..Default::default()
        };
        assert_eq!(
            Client::resolved("work", &own).args(),
            ["-o", "NumberOfPasswordPrompts=1", "work"]
        );
        let socket = ResolvedHost {
            control_path: Some("/tmp/cm".into()),
            ..Default::default()
        };
        let args = master_args("work", &socket, IdleMode::Cat);
        assert_eq!(args[..2], ["-o", "NumberOfPasswordPrompts=1"]);
        assert_eq!(args[args.len() - 2..], ["work", "cat"]);
        assert_eq!(args.iter().any(|a| a == "-M"), platform::control_master());
        let args = master_args("work", &ResolvedHost::default(), IdleMode::None);
        assert!(!args.iter().any(|a| a == "-M"), "{args:?}");
        assert!(args.windows(2).any(|w| w == ["-o", "LogLevel=VERBOSE"]));
        assert_eq!(args.last().map(String::as_str), Some("work"));
    }

    #[test]
    fn master_flag_is_an_argument_of_its_own() {
        assert!(has_master_flag("ssh -o Tag=shellder -M -N work"));
        assert!(!has_master_flag("ssh -o Tag=shellder -tt work sh -c -Mx"));
        assert!(!has_master_flag("ssh -o Tag=shellder -tt work echo a-M"));
    }

    /// A copy goes into the directory as a directory, slash and all,
    /// whatever its name; a remote source for scp names one file.
    #[test]
    fn copies_name_their_folder_and_their_file() {
        let paths = ["/l/a \"b\".txt".to_string(), "/l/c\\d".to_string()];
        assert_eq!(
            sftp_batch("put", &paths, "/r/x y"),
            "put \"/l/a \\\"b\\\".txt\" \"/r/x y/\"\nput \"/l/c\\\\d\" \"/r/x y/\"\n"
        );
        assert_eq!(
            sftp_batch("get", &paths[..1], "/"),
            "get \"/l/a \\\"b\\\".txt\" \"/\"\n"
        );
        assert_eq!(glob_escape("/r/report[1].pdf"), "/r/report\\[1\\].pdf");
        assert_eq!(glob_escape("a*b?c\\d"), "a\\*b\\?c\\\\d");
        assert_eq!(glob_escape("/plain name"), "/plain name");
    }
}
