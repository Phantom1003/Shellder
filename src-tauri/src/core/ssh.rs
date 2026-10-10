//! Running the OpenSSH client: one-off commands, `ssh -G`, the checks on a
//! master's socket, and the master itself with its pty.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::fd::OwnedFd;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::catalog::ResolvedHost;
use super::config;
use super::log;
use super::prompt::PromptHandler;
use super::pty::{self, Pty};

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

/// `-F file` only when a test config is in use; normally ssh reads
/// ~/.ssh/config on its own.
pub fn base_args() -> Vec<String> {
    let c = config::get();
    if c.config_overridden {
        vec!["-F".into(), config::path_str(&c.ssh_config_file)]
    } else {
        vec![]
    }
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
}

fn status_code(s: ExitStatus) -> i32 {
    s.code().unwrap_or(-1)
}

/// Run ssh (or scp) to completion, capturing output. `on_start` sees the
/// child's pid as soon as it runs, for a caller that may have to stop it (a
/// copy the user cancels).
pub fn run_binary(bin: &str, args: &[String], on_start: Option<&mut dyn FnMut(u32)>) -> Output {
    let mut cmd = Command::new(bin);
    cmd.args(base_args())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Output {
                status: -1,
                stdout: String::new(),
                stderr: e.to_string(),
            }
        }
    };
    if let Some(f) = on_start {
        f(child.id());
    }
    let mut err_pipe = child.stderr.take();
    let err_thread = thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let mut stdout = String::new();
    if let Some(mut p) = child.stdout.take() {
        let _ = p.read_to_string(&mut stdout);
    }
    let status = child.wait().map(status_code).unwrap_or(-1);
    let stderr = err_thread.join().unwrap_or_default();
    Output {
        status,
        stdout,
        stderr,
    }
}

pub fn run(args: &[&str]) -> Output {
    let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    run_binary(&binary("ssh"), &v, None)
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

pub fn resolve(host: &str) -> Result<ResolvedHost, String> {
    Ok(ResolvedHost::from_config(&config(host)?))
}

/// ControlPath from the user's ssh config, None when unset.
pub fn control_path(host: &str) -> Result<Option<String>, String> {
    Ok(resolve(host)?.control_path)
}

pub fn master_alive(host: &str) -> bool {
    run(&["-O", "check", host]).status == 0
}

/// pid of the master owning the host's socket ("Master running (pid=N)").
pub fn master_pid(host: &str) -> Option<u32> {
    let r = run(&["-O", "check", host]);
    if r.status != 0 {
        return None;
    }
    let re = Regex::new("pid=(\\d+)").ok()?;
    re.captures(&r.stderr)?.get(1)?.as_str().parse().ok()
}

/// Marker put on every master we spawn so a later shellder can recognise
/// one left behind by a crashed instance (`ssh -o Tag=shellder`, visible in ps).
pub const TAG: &str = "Tag=shellder";

fn ps(args: &[&str]) -> String {
    Command::new("ps")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Kill masters left behind by an earlier shellder that did not shut down
/// cleanly: tagged ssh processes whose parent is now init (ppid 1). Ones
/// that already own a socket are replaced by the daemon; this sweep catches
/// the rest (e.g. still stuck in authentication).
pub fn kill_orphaned_masters() {
    for line in ps(&["-axo", "pid=,ppid=,command="]).lines() {
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
        let cmd: String = f.collect::<Vec<_>>().join(" ");
        log::warn(format!(
            "orphaned shellder master pid {pid} from an earlier run, killing: {}",
            cmd.chars().take(80).collect::<String>()
        ));
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

/// True when `pid` is an ssh master started by shellder (this or an earlier run).
pub fn is_our_master(pid: u32) -> bool {
    ps(&["-o", "command=", "-p", &pid.to_string()]).contains(TAG)
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

/// Arguments that only concern the master process itself. Everything else
/// (HostName, User, ProxyJump, ControlPath, …) comes from ~/.ssh/config.
pub fn master_args(host: &str, resolved: &ResolvedHost, mode: IdleMode) -> Vec<String> {
    let mut args: Vec<String> = [
        "-M",
        "-o",
        TAG, // identifies our masters in ps
        "-o",
        "ControlPersist=no", // we own the process; no daemonising
        "-o",
        "NumberOfPasswordPrompts=1", // a wrong secret fails fast instead of looping
        "-o",
        "LogLevel=ERROR",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if resolved.server_alive_interval == 0 {
        // Detect a dead link so the master can be restarted; only applied
        // when the user's config does not set its own value.
        args.extend(
            [
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
            ]
            .map(String::from),
        );
    }
    match mode {
        IdleMode::None => args.extend(["-N", host].map(String::from)),
        // Force a remote pty even though stdin is a pipe; the login shell
        // then sits idle reading from it. X11 is left to the multiplexed
        // sessions, which request their own forwarding.
        IdleMode::Shell => args.extend(["-tt", "-o", "ForwardX11=no", host].map(String::from)),
        IdleMode::Cat => args.extend(["-T", "-o", "ForwardX11=no", host, "cat"].map(String::from)),
    }
    args
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

    /// Last non-empty line, trimmed.
    pub fn last_line(&self) -> Option<String> {
        self.buf
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty() && !l.starts_with("Warning: untrusted X11"))
            .map(String::from)
    }
}

/// A master process: the child, its pty, and what it said. Dropping it
/// closes the pty's slave side, which ends the reader thread.
pub struct MasterProc {
    child: Child,
    pub pid: u32,
    pub tail: Arc<Mutex<Tail>>,
    exit: Option<ExitStatus>,
    _slave: OwnedFd,
}

impl MasterProc {
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

/// A pipe whose reader logs every complete line as "host: line".
fn line_pipe<R: Read + Send + 'static>(host: String, pipe: R, tail: Option<Arc<Mutex<Tail>>>) {
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
                    }
                }
            }
        })
        .expect("thread");
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
    let Some(cp) = &resolved.control_path else {
        return Err(format!(
            "no ControlPath configured for {host} in ~/.ssh/config"
        ));
    };
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
    let pty = Pty::open()?;
    let mut cmd = Command::new(binary("ssh"));
    cmd.args(base_args())
        .args(master_args(host, resolved, mode));
    // Questions come down the pty, never through an askpass program: a
    // helper the user's environment points at would answer in our place.
    cmd.env_remove("SSH_ASKPASS")
        .env_remove("SSH_ASKPASS_REQUIRE");
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
    pty.attach(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("cannot start ssh: {e}"))?;
    let pid = child.id();
    let tail = Arc::new(Mutex::new(Tail::default()));
    if let Some(out) = child.stdout.take() {
        line_pipe(host.to_string(), out, None);
    }
    if let Some(err) = child.stderr.take() {
        line_pipe(host.to_string(), err, Some(tail.clone()));
    }
    let Pty { master, slave } = pty;
    let h = host.to_string();
    thread::Builder::new()
        .name(format!("ssh-pty-{host}"))
        .spawn(move || pty::serve(h, pid, master, handler))
        .map_err(|e| e.to_string())?;
    log::info(format!(
        "{host}: spawned master pid {pid} [{}] (socket {cp})",
        mode.title()
    ));
    Ok(MasterProc {
        child,
        pid,
        tail,
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

/// Copy paths from one side into a directory on the other. Between two
/// hosts `-3` sends the bytes through this machine, so both ends stay on
/// their own master and neither needs to reach the other. BatchMode so a
/// missing master fails at once instead of waiting for an answer nobody
/// will give; -q leaves the meter out and keeps the log to real errors.
pub fn copy(
    paths: &[String],
    from: &FileSource,
    directory: &str,
    to: &FileSource,
    on_start: Option<&mut dyn FnMut(u32)>,
) -> Output {
    fn there(source: &FileSource, path: &str) -> String {
        match source.host() {
            Some(h) => format!("{h}:{path}"),
            None => path.to_string(),
        }
    }
    if from == to {
        return Output {
            status: -1,
            stdout: String::new(),
            stderr: "a side does not copy into itself".into(),
        };
    }
    let mut args: Vec<String> = Vec::new();
    if from.host().is_some() && to.host().is_some() {
        args.push("-3".into());
    }
    args.extend(["-r", "-p", "-q", "-o", "BatchMode=yes"].map(String::from));
    args.extend(paths.iter().map(|p| there(from, p)));
    args.push(there(to, directory) + "/");
    run_binary(&binary("scp"), &args, on_start)
}
