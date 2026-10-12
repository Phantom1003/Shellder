//! Running the OpenSSH client: one-off commands, `ssh -G`, the checks on a
//! master's socket, and the master itself with its pty.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::catalog::ResolvedHost;
use super::config;
use super::log;
use super::platform;
use super::prompt::{Origin, PromptHandler};
use super::pty::{self, Pty};
use super::session;

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
/// and ControlPersist off. A terminal, a session or a copy that finds no
/// master up and becomes one itself, under a `ControlMaster auto` in the
/// user's config, must not fork a background master that outlives it:
/// macOS 26 holds an app as still running after it quit while a process
/// it started is alive, and says so ("Shellder is running in the
/// background"). Without ControlPersist the master goes with its last
/// session, and quitting takes the sessions with it.
pub fn base_args() -> Vec<String> {
    let c = config::get();
    let mut args = if c.config_overridden {
        vec!["-F".into(), config::path_str(&c.ssh_config_file)]
    } else {
        vec![]
    };
    args.extend(["-o", "ControlPersist=no"].map(String::from));
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
}

fn status_code(s: ExitStatus) -> i32 {
    s.code().unwrap_or(-1)
}

/// Run ssh (or scp) to completion, capturing output. `on_start` sees the
/// child's pid as soon as it runs, for a caller that may have to stop it (a
/// copy the user cancels).
pub fn run_binary(bin: &str, args: &[String], on_start: Option<&mut dyn FnMut(u32)>) -> Output {
    run_binary_fed(bin, args, None, on_start, None)
}

/// `run_binary` with something on the child's stdin: the batch an sftp
/// reads its commands from. Without `input` stdin is closed. With `asking`
/// the process gets an asking pty under that host's name: it logs in on
/// its own, and its questions are answered like a master's.
fn run_binary_fed(
    bin: &str,
    args: &[String],
    input: Option<String>,
    on_start: Option<&mut dyn FnMut(u32)>,
    asking: Option<&str>,
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
    let failed = |e: String| Output {
        status: -1,
        stdout: String::new(),
        stderr: e,
    };
    // The asking pty's slave side lives as long as the child does.
    let (mut child, _slave) = match asking {
        Some(host) => match spawn_asking(cmd, host, Origin::Session, session::prompt_handler()) {
            Ok((c, s)) => (c, Some(s)),
            Err(e) => return failed(e),
        },
        None => match cmd.spawn() {
            Ok(c) => (c, None),
            Err(e) => return failed(e.to_string()),
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

/// Whether a command for the host can go through its master, without a
/// login: one with a socket is up (so never on a platform without
/// ControlMaster, and never for a host without a ControlPath).
pub fn master_reusable(host: &str) -> bool {
    platform::control_master() && master_alive(host)
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
    let mut args: Vec<String> = [
        "-o",
        TAG, // identifies our masters in ps
        "-o",
        "NumberOfPasswordPrompts=1", // a wrong secret fails fast instead of looping
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if multiplexed(resolved) {
        args.extend(
            [
                "-M", // ControlPersist is off in base_args: we own the process
                "-o",
                "LogLevel=ERROR",
            ]
            .map(String::from),
        );
    } else {
        // Without a socket to check, "Authenticated to host" on stderr
        // (a VERBOSE line) is how the daemon learns the connection is up.
        args.extend(["-o", "LogLevel=VERBOSE"].map(String::from));
    }
    args.extend(keepalive_args(resolved));
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
/// `-e none`, so that no answer is taken for ssh's escape.
pub fn run_on_terminal(
    host: &str,
    command: &str,
    origin: Origin,
    handler: Arc<dyn PromptHandler>,
) -> Output {
    let mut cmd = Command::new(binary("ssh"));
    cmd.args(base_args());
    if let Ok(resolved) = resolve(host) {
        cmd.args(keepalive_args(&resolved));
    }
    cmd.args([
        "-tt",
        "-e",
        "none",
        "-o",
        "NumberOfPasswordPrompts=1",
        "-o",
        "LogLevel=ERROR",
        host,
        command,
    ]);
    // No colours, no cursor movement: the output is read, not drawn.
    cmd.env("TERM", "dumb").stderr(Stdio::piped());
    let said = Arc::new(Mutex::new(String::new()));
    let (mut child, slave, reader) =
        match spawn_on_pty(cmd, host, origin, handler, true, Some(said.clone())) {
            Ok(x) => x,
            Err(e) => {
                return Output {
                    status: -1,
                    stdout: String::new(),
                    stderr: e,
                }
            }
        };
    let mut err_pipe = child.stderr.take();
    let err_thread = thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
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
        return Output {
            status: -1,
            stdout: String::new(),
            stderr: "a side does not copy into itself".into(),
        };
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
            args.extend(paths.iter().map(|p| there(from, p)));
            args.push(there(to, directory) + "/");
            // Both hosts ask on the one pty; the prompt's own user@host
            // says which credentials each wants.
            return run_binary_fed(&binary("scp"), &args, None, on_start, login.then_some(a));
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
    let batch: String = paths
        .iter()
        .map(|p| format!("{verb} {} {}\n", sftp_word(p), sftp_word(directory)))
        .collect();
    run_binary_fed(
        &binary("sftp"),
        &args,
        Some(batch),
        on_start,
        login.then_some(host),
    )
}

/// A copy with a host side whose master is not up logs in on its own.
pub fn copy_logs_in(from: &FileSource, to: &FileSource) -> bool {
    [from, to]
        .into_iter()
        .filter_map(|s| s.host())
        .any(|h| !master_reusable(h))
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
