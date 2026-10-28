//! Keeps one ControlMaster per switched-on host alive. All state lives on
//! one thread; the UI reads snapshots through `on_change`.
//!
//! A host whose config has a ControlPath (on a platform with ControlMaster)
//! gets an `ssh -M` master other ssh commands reuse; any other host gets
//! the same connection without the socket, kept alive for the port
//! forwards its config asks for.
//!
//! Switch semantics: turning a host on is one connection attempt. If that
//! attempt fails the switch goes back off (`on_disabled`) and nothing is
//! retried. A connection that succeeded stays up until it drops, and a drop
//! turns the switch off as well, unless the host is locked: then drops are
//! retried with back-off, and only a link that keeps dying right after
//! connecting gives up after a few tries. Whatever turns the switch off
//! clears the lock.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::catalog::ResolvedHost;
use super::config;
use super::log;
use super::prefs;
use super::prompt::PromptHandler;
use super::reconnect;
use super::ssh::{self, IdleMode, MasterProc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HostState {
    /// Switch off, nothing running.
    Off,
    Up {
        pid: u32,
    },
    Connecting,
    /// A master we did not start owns the socket.
    Foreign,
    /// Dropped after a successful connection; retrying.
    Waiting {
        retry_in: i64,
    },
    WaitingForJump {
        jump: String,
    },
    /// Cannot even try (no ControlPath, config error).
    Error {
        message: String,
    },
}

impl HostState {
    pub fn is_up(&self) -> bool {
        matches!(self, HostState::Up { .. } | HostState::Foreign)
    }
    pub fn is_running(&self) -> bool {
        matches!(self, HostState::Up { .. } | HostState::Connecting)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub host: String,
    /// The Connect switch, as the daemon sees it.
    pub enabled: bool,
    /// Reconnect after drops, connect at launch.
    pub locked: bool,
    pub state: HostState,
    /// Unix seconds since the master came up (or was started).
    pub since: Option<u64>,
    pub last_error: Option<String>,
    pub quick_failures: u32,
    /// How the master stays connected.
    pub idle_mode: IdleMode,
    /// Switched-on hosts whose ProxyJump goes through this one. The master is
    /// kept up for them even while its own switch is off.
    pub needed_by: Vec<String>,
    /// The master owns a socket other ssh commands reuse (a ControlPath,
    /// on a platform with ControlMaster). Without one the connection is
    /// kept alive for its port forwards, and only the app knows it is up.
    pub socket: bool,
}

impl HostStatus {
    /// Off because an attempt failed or the link dropped: the dot stays red
    /// and the reason is shown, until the switch goes on again.
    pub fn failed(&self) -> bool {
        self.state == HostState::Off && self.last_error.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSpec {
    pub alias: String,
    pub enabled: bool,
    pub locked: bool,
    pub resolved: Option<ResolvedHost>,
    pub resolve_error: Option<String>,
}

fn unix(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct Master {
    spec: HostSpec,
    proc_: Option<MasterProc>,
    started: Instant,
    started_at: SystemTime,
    established_at: Option<SystemTime>,
    established: bool,
    /// The wait the last failure scheduled, and when it was.
    backoff: Duration,
    failed_at: Option<Instant>,
    next_try: Option<Instant>,
    /// The first failure of the series under way: the drop a locked host
    /// has been recovering from since, which its rule's give-up counts
    /// from.
    dropped_at: Option<Instant>,
    last_check: Instant,
    next_probe: Option<Instant>,
    quick_failures: u32,
    foreign: bool,
    last_error: Option<String>,
    fatal: Option<String>,
    waiting_for_jump: Option<String>,
    /// Hosts (switched on, or themselves needed) whose first ProxyJump hop is
    /// this host. Recomputed before every tick.
    needed_by: Vec<String>,
    /// `wanted` as of the previous tick, to notice the implicit transitions.
    was_wanted: bool,
    /// True once a connection succeeded since the switch was turned on.
    ever_established: bool,
    idle_mode: IdleMode,
}

impl Master {
    /// Whether this host's master owns a socket (see `ssh::multiplexed`).
    fn socket(&self) -> bool {
        self.spec
            .resolved
            .as_ref()
            .map(ssh::multiplexed)
            .unwrap_or(false)
    }

    /// Whether `ssh -O check` could find a master at all: not without the
    /// socket file, and then the check (an ssh process, on this thread)
    /// is spared. The ControlPath is the one `ssh -G` printed, its tokens
    /// expanded, as `ssh::spawn_master` takes it; one still holding a `%`
    /// says nothing about the file, and is checked.
    fn socket_present(&self) -> bool {
        let Some(cp) = self
            .spec
            .resolved
            .as_ref()
            .and_then(|r| r.control_path.as_deref())
        else {
            return false;
        };
        if cp.contains('%') {
            return true;
        }
        std::path::Path::new(&config::expand_tilde(cp)).exists()
    }
}

impl Master {
    fn new(spec: HostSpec) -> Master {
        let idle_mode = prefs::idle_mode(&spec.alias)
            .and_then(|m| IdleMode::parse(&m))
            .unwrap_or(IdleMode::None);
        Master::with_idle_mode(spec, idle_mode)
    }

    fn with_idle_mode(spec: HostSpec, idle_mode: IdleMode) -> Master {
        Master {
            spec,
            proc_: None,
            started: Instant::now(),
            started_at: SystemTime::now(),
            established_at: None,
            established: false,
            backoff: Duration::ZERO,
            failed_at: None,
            next_try: None,
            dropped_at: None,
            last_check: Instant::now(),
            next_probe: None,
            quick_failures: 0,
            foreign: false,
            last_error: None,
            fatal: None,
            waiting_for_jump: None,
            needed_by: vec![],
            was_wanted: false,
            ever_established: false,
            idle_mode,
        }
    }

    fn host(&self) -> &str {
        &self.spec.alias
    }

    fn running(&mut self) -> bool {
        self.proc_.as_mut().map(|p| p.is_running()).unwrap_or(false)
    }

    /// Kept connected: switched on, or a jump host some wanted host goes through.
    fn wanted(&self) -> bool {
        self.spec.enabled || !self.needed_by.is_empty()
    }

    /// Kept connected only because other hosts route through it.
    fn implicit(&self) -> bool {
        !self.spec.enabled && !self.needed_by.is_empty()
    }

    /// A failure now is a drop to recover from, not a failed attempt: the
    /// host is locked and has been connected since the switch went on.
    /// Otherwise a failure turns the switch off.
    fn retries(&self) -> bool {
        self.spec.locked && self.ever_established
    }

    /// Schedule the next attempt after a failure, by the host's reconnect
    /// rule (`core::reconnect`, read afresh so a change in Settings
    /// counts from the next failure on). A quick failure (an attempt that
    /// got nowhere, a link that died right after coming up) lengthens the
    /// wait; a drop after a connection that held starts the waits over.
    /// True when the rule says to give up: the connection has been down
    /// longer than it allows.
    fn failed(&mut self, quick: bool) -> bool {
        let rule = reconnect::for_host(self.host());
        if quick {
            self.quick_failures += 1;
        } else {
            self.quick_failures = 0;
        }
        let now = Instant::now();
        let since = *self.dropped_at.get_or_insert(now);
        self.backoff = rule.wait(self.quick_failures);
        self.failed_at = Some(now);
        self.next_try = Some(now + self.backoff);
        rule.gives_up()
            .map(|limit| now.duration_since(since) >= limit)
            .unwrap_or(false)
    }

    /// The reason a host switches off when its rule gives up.
    fn gave_up(&self, why: &str) -> String {
        let down = self
            .dropped_at
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        format!("gave up after {}: {why}", duration(down))
    }

    /// The wait under way, counted again under the rule as it is now (a
    /// change in Settings while a host waits).
    fn reschedule(&mut self) {
        let (Some(at), Some(_)) = (self.failed_at, self.next_try) else {
            return;
        };
        let wait = reconnect::for_host(self.host()).wait(self.quick_failures);
        if wait != self.backoff {
            self.backoff = wait;
            self.next_try = Some(at + wait);
        }
    }

    /// Fresh start (switch turned on, Reconnect pressed).
    fn reset(&mut self) {
        self.quick_failures = 0;
        self.backoff = Duration::ZERO;
        self.failed_at = None;
        self.next_try = None;
        self.dropped_at = None;
        self.fatal = None;
        self.last_error = None;
        self.waiting_for_jump = None;
        self.ever_established = false;
    }

    fn stop(&mut self) {
        finish_stops(self.begin_stop().into_iter().collect());
    }

    /// The first half of a stop: the process taken away and sent SIGTERM,
    /// handed back to be waited for (`finish_stops`), so that several
    /// masters stopping together are waited for together.
    fn begin_stop(&mut self) -> Option<MasterProc> {
        let p = self.proc_.take();
        self.established = false;
        self.established_at = None;
        let mut p = p?;
        if !p.is_running() {
            return None;
        }
        log::info(format!("{}: stopping master pid {}", self.host(), p.pid));
        p.terminate();
        // Commands stop going through it now, not when the answer kept
        // about it runs out.
        ssh::forget_master(self.host());
        Some(p)
    }

    fn status(&mut self) -> HostStatus {
        let running = self.running();
        let state = if running {
            if self.established {
                HostState::Up {
                    pid: self.proc_.as_ref().map(|p| p.pid).unwrap_or(0),
                }
            } else {
                HostState::Connecting
            }
        } else if self.foreign {
            HostState::Foreign
        } else if !self.wanted() {
            HostState::Off
        } else if let Some(f) = &self.fatal {
            HostState::Error { message: f.clone() }
        } else if let Some(j) = &self.waiting_for_jump {
            HostState::WaitingForJump { jump: j.clone() }
        } else {
            let retry_in = self
                .next_try
                .map(|t| {
                    t.saturating_duration_since(Instant::now())
                        .as_secs_f64()
                        .ceil() as i64
                })
                .unwrap_or(0);
            HostState::Waiting {
                retry_in: retry_in.max(0),
            }
        };
        HostStatus {
            host: self.spec.alias.clone(),
            enabled: self.spec.enabled,
            locked: self.spec.locked,
            state,
            since: self.established_at.map(unix).or(if running {
                Some(unix(self.started_at))
            } else {
                None
            }),
            last_error: self.last_error.clone(),
            quick_failures: self.quick_failures,
            idle_mode: self.idle_mode,
            needed_by: self.needed_by.clone(),
            socket: self.socket(),
        }
    }
}

/// The second half of a stop: wait for masters sent SIGTERM to exit,
/// five seconds for all of them together rather than for each, and kill
/// the ones still there after that.
fn finish_stops(mut procs: Vec<MasterProc>) {
    if procs.is_empty() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while procs.iter_mut().any(|p| p.is_running()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    for p in &mut procs {
        if p.is_running() {
            p.kill();
        }
    }
}

/// Masters stopped together (`begin_stop`, then `finish_stops`), by name.
fn stop_together(masters: &mut HashMap<String, Master>, hosts: &[String]) {
    let procs = hosts
        .iter()
        .filter_map(|h| masters.get_mut(h)?.begin_stop())
        .collect();
    finish_stops(procs);
}

/// At most this many `ssh -O check` runs a tick, for the health checks of
/// masters that are up and the probes of hosts that are not (each one an
/// ssh process the daemon's thread waits for): a long host list is gone
/// through over a few ticks instead of all in one, and its checks stay
/// spread out from then on.
const CHECKS_PER_TICK: usize = 4;

/// How long `Daemon::snapshot` waits for the daemon's thread to answer
/// before it takes the snapshot published last.
const SNAPSHOT_WAIT: Duration = Duration::from_secs(5);

/// An attempt that failed, or a connection that dropped, as
/// `Inner::attempt_failed` takes it.
struct Failure {
    /// Got nowhere, or died right after coming up: lengthens the wait.
    quick: bool,
    /// Why, as a host that tries again says it.
    why: String,
    /// Why, as the switch goes off over it, when that is said otherwise
    /// ("could not connect" where a reconnection says "reconnect failed").
    off: Option<String>,
    /// A connection that came up and died at once: a locked host gives
    /// up for this reason too after `MAX_QUICK_FAILURES` of them in a row.
    keeps_dropping: Option<String>,
}

impl Failure {
    fn new(quick: bool, why: impl Into<String>) -> Failure {
        Failure {
            quick,
            why: why.into(),
            off: None,
            keeps_dropping: None,
        }
    }

    fn off(mut self, why: impl Into<String>) -> Failure {
        self.off = Some(why.into());
        self
    }

    fn keeps_dropping(mut self, why: impl Into<String>) -> Failure {
        self.keeps_dropping = Some(why.into());
        self
    }
}

enum Cmd {
    Configure(Vec<HostSpec>),
    Reconnect(String),
    Disconnect(String),
    CloseSocket(String),
    GiveUp(String, String),
    SetIdleMode(String, IdleMode),
    /// The reconnect rules changed: hosts waiting for a retry wait by the
    /// new ones.
    RulesChanged,
    ConnectAll,
    DisconnectAll,
    Snapshot(Sender<Vec<HostStatus>>),
    Shutdown(Sender<()>),
}

pub type OnChange = Box<dyn Fn(Vec<HostStatus>) + Send>;
pub type OnDisabled = Box<dyn Fn(String, Option<String>) + Send>;

/// Handle to the daemon thread. Cheap to clone.
#[derive(Clone)]
pub struct Daemon {
    tx: Sender<Cmd>,
    /// The snapshot the daemon published last, for a `snapshot` its
    /// thread is too busy to answer.
    published: Arc<Mutex<Vec<HostStatus>>>,
}

impl Daemon {
    pub fn start(
        handler: Arc<dyn PromptHandler>,
        on_change: OnChange,
        on_disabled: OnDisabled,
    ) -> Daemon {
        let (tx, rx) = mpsc::channel();
        let published = Arc::new(Mutex::new(vec![]));
        let shared = published.clone();
        thread::Builder::new()
            .name("shellder-daemon".into())
            .spawn(move || {
                let mut d = Inner {
                    masters: HashMap::new(),
                    order: vec![],
                    last_snapshot: vec![],
                    published: shared,
                    checks_left: CHECKS_PER_TICK,
                    handler,
                    on_change,
                    on_disabled,
                };
                d.run(rx);
            })
            .expect("daemon thread");
        Daemon { tx, published }
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    /// Replace the host list. Called whenever ~/.ssh/config or the enabled
    /// set changes; existing masters keep running when nothing relevant changed.
    pub fn configure(&self, specs: Vec<HostSpec>) {
        self.send(Cmd::Configure(specs))
    }
    /// Stop and start again (switches an off host on, keeps the lock as it
    /// is). Takes over a socket that belongs to a master started outside
    /// shellder.
    pub fn reconnect(&self, host: &str) {
        self.send(Cmd::Reconnect(host.into()))
    }
    /// Disconnect and turn the switch off (which clears the lock). Hosts that
    /// reach the network through this one (ProxyJump) go down with it.
    pub fn disconnect(&self, host: &str) {
        self.send(Cmd::Disconnect(host.into()))
    }
    /// `ssh -O exit`: asks whoever owns the socket to shut down, and turns
    /// the switch off.
    pub fn close_socket(&self, host: &str) {
        self.send(Cmd::CloseSocket(host.into()))
    }
    /// A prompt was cancelled or a host key declined: this attempt failed.
    pub fn give_up(&self, host: &str, reason: &str) {
        self.send(Cmd::GiveUp(host.into(), reason.into()))
    }
    pub fn set_idle_mode(&self, host: &str, mode: IdleMode) {
        self.send(Cmd::SetIdleMode(host.into(), mode))
    }
    pub fn rules_changed(&self) {
        self.send(Cmd::RulesChanged)
    }
    pub fn connect_all(&self) {
        self.send(Cmd::ConnectAll)
    }
    pub fn disconnect_all(&self) {
        self.send(Cmd::DisconnectAll)
    }
    /// Every host's status, from the daemon's thread. One that does not
    /// answer within `SNAPSHOT_WAIT` (stopping masters, an ssh that hangs)
    /// is not waited for: the snapshot it published last stands in, which
    /// is never older than its last change of a switch.
    pub fn snapshot(&self) -> Vec<HostStatus> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Snapshot(tx));
        match rx.recv_timeout(SNAPSHOT_WAIT) {
            Ok(snap) => snap,
            Err(e) => {
                log::warn(format!(
                    "daemon: no snapshot from its thread ({}); taking the one it published last",
                    match e {
                        RecvTimeoutError::Timeout =>
                            format!("busy for more than {}s", SNAPSHOT_WAIT.as_secs()),
                        RecvTimeoutError::Disconnected => "it has stopped".to_string(),
                    }
                ));
                self.published
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
            }
        }
    }
    /// Stop every master. Returns when done.
    pub fn shutdown(&self) {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Shutdown(tx));
        let _ = rx.recv_timeout(Duration::from_secs(30));
    }
}

struct Inner {
    masters: HashMap<String, Master>,
    order: Vec<String>,
    last_snapshot: Vec<HostStatus>,
    /// The handle's copy of the snapshot (`Daemon::published`).
    published: Arc<Mutex<Vec<HostStatus>>>,
    /// `ssh -O check` runs this tick may still make (`CHECKS_PER_TICK`).
    checks_left: usize,
    handler: Arc<dyn PromptHandler>,
    on_change: OnChange,
    on_disabled: OnDisabled,
}

impl Inner {
    fn run(&mut self, rx: Receiver<Cmd>) {
        config::ensure_dirs();
        log::info(format!("daemon starting (pid {})", std::process::id()));
        ssh::kill_orphans();
        loop {
            match rx.recv_timeout(config::POLL_INTERVAL) {
                Ok(Cmd::Shutdown(done)) => {
                    log::info("daemon stopping");
                    let procs = self
                        .masters
                        .values_mut()
                        .filter_map(Master::begin_stop)
                        .collect();
                    finish_stops(procs);
                    let _ = done.send(());
                    return;
                }
                Ok(cmd) => {
                    self.handle(cmd);
                    self.tick_all();
                }
                Err(RecvTimeoutError::Timeout) => self.tick_all(),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Configure(specs) => self.configure(specs),
            Cmd::Reconnect(host) => {
                let Some(m) = self.masters.get_mut(&host) else {
                    return;
                };
                m.stop();
                if m.foreign {
                    let r = ssh::run(&["-O", "exit", &host]);
                    log::info(format!(
                        "{host}: closing external master (ssh -O exit rc {})",
                        r.status
                    ));
                    m.foreign = false;
                }
                m.reset();
                if !m.wanted() {
                    m.spec.enabled = true;
                    self.share();
                    (self.on_disabled)(host.clone(), None); // model mirrors the switch state
                }
                log::info(format!("{host}: reconnect requested"));
            }
            Cmd::Disconnect(host) => {
                if self.masters.contains_key(&host) {
                    self.stop_with_dependents(&host);
                }
            }
            Cmd::CloseSocket(host) => {
                if self.masters.contains_key(&host) {
                    let r = ssh::run(&["-O", "exit", &host]);
                    let msg = r.text();
                    log::info(format!(
                        "{host}: ssh -O exit -> rc {}{}",
                        r.status,
                        if msg.is_empty() {
                            String::new()
                        } else {
                            format!(" ({msg})")
                        }
                    ));
                    self.stop_with_dependents(&host);
                }
            }
            Cmd::GiveUp(host, reason) => {
                if let Some(m) = self.masters.get_mut(&host) {
                    m.stop();
                    self.turn_off(&host, Some(reason));
                }
            }
            Cmd::RulesChanged => {
                for m in self.masters.values_mut() {
                    m.reschedule();
                }
            }
            Cmd::SetIdleMode(host, mode) => {
                let Some(m) = self.masters.get_mut(&host) else {
                    return;
                };
                if m.idle_mode == mode {
                    return;
                }
                m.idle_mode = mode;
                prefs::set_idle_mode(
                    &host,
                    if mode == IdleMode::None {
                        None
                    } else {
                        Some(mode.raw())
                    },
                );
                log::info(format!("{host}: idle mode set to {}", mode.title()));
                if m.running() {
                    m.stop();
                    m.reset();
                    m.ever_established = false;
                }
            }
            Cmd::ConnectAll => {
                for m in self.masters.values_mut() {
                    if m.spec.enabled && !m.running() {
                        m.reset();
                    }
                }
                log::info("reconnect all requested");
            }
            Cmd::DisconnectAll => {
                let hosts: Vec<String> = self
                    .order
                    .iter()
                    .filter(|h| self.masters.get(*h).is_some_and(|m| m.spec.enabled))
                    .cloned()
                    .collect();
                stop_together(&mut self.masters, &hosts);
                for h in hosts {
                    if let Some(m) = self.masters.get_mut(&h) {
                        m.foreign = false;
                    }
                    self.turn_off(&h, None);
                }
                log::info("disconnect all requested");
            }
            Cmd::Snapshot(tx) => {
                let _ = tx.send(self.snapshot());
            }
            Cmd::Shutdown(_) => unreachable!(),
        }
    }

    fn configure(&mut self, specs: Vec<HostSpec>) {
        for spec in &specs {
            if let Some(m) = self.masters.get_mut(&spec.alias) {
                let was = m.spec.clone();
                m.spec = spec.clone();
                if was.enabled && !spec.enabled {
                    // Stopped by reconcile_jump_hosts in the tick below,
                    // unless a switched-on host still jumps through it.
                } else if !was.enabled && spec.enabled {
                    if m.running() {
                        log::info(format!(
                            "{}: switched on, already connected as a jump host",
                            spec.alias
                        ));
                    } else {
                        m.reset();
                        log::info(format!("{}: switched on, connecting", spec.alias));
                    }
                } else if was.resolved != spec.resolved || was.resolve_error != spec.resolve_error {
                    m.fatal = None;
                }
            } else {
                self.masters
                    .insert(spec.alias.clone(), Master::new(spec.clone()));
            }
        }
        let keep: HashSet<&String> = specs.iter().map(|s| &s.alias).collect();
        let gone: Vec<String> = self
            .masters
            .keys()
            .filter(|k| !keep.contains(k))
            .cloned()
            .collect();
        stop_together(&mut self.masters, &gone);
        for alias in gone {
            self.masters.remove(&alias);
        }
        self.order = specs.into_iter().map(|s| s.alias).collect();
    }

    fn snapshot(&mut self) -> Vec<HostStatus> {
        let order = self.order.clone();
        order
            .iter()
            .filter_map(|h| self.masters.get_mut(h).map(|m| m.status()))
            .collect()
    }

    /// Hand the snapshot as it is now to the handle (`Daemon::published`):
    /// before every switch the daemon flips itself, whose callback asks
    /// for a snapshot at once, so that one this thread is too busy to
    /// answer is not older than the switch.
    fn share(&mut self) {
        let snap = self.snapshot();
        *self.published.lock().unwrap_or_else(|e| e.into_inner()) = snap;
    }

    fn stop_with_dependents(&mut self, host: &str) {
        let dependents: Vec<String> = self
            .dependents(host)
            .into_iter()
            .filter(|d| self.masters.get(d).is_some_and(|m| m.wanted()))
            .collect();
        let mut stopping = dependents.clone();
        stopping.push(host.to_string());
        stop_together(&mut self.masters, &stopping);
        for d in dependents {
            log::info(format!(
                "{d}: disconnected together with its jump host {host}"
            ));
            if let Some(m) = self.masters.get_mut(&d) {
                m.foreign = false;
            }
            self.turn_off(&d, None);
        }
        if let Some(m) = self.masters.get_mut(host) {
            m.foreign = false;
            m.next_probe = None;
        }
        self.turn_off(host, None);
    }

    /// Masters whose ProxyJump chain passes through `host` (nearest hop first).
    fn dependents(&self, host: &str) -> Vec<String> {
        let mut out = vec![];
        let mut seen: HashSet<String> = HashSet::from([host.to_string()]);
        let mut frontier = vec![host.to_string()];
        while let Some(j) = frontier.pop() {
            for h in &self.order {
                let Some(d) = self.masters.get(h) else {
                    continue;
                };
                if seen.contains(h) {
                    continue;
                }
                if d.spec
                    .resolved
                    .as_ref()
                    .and_then(|r| r.jump_alias.as_deref())
                    == Some(j.as_str())
                {
                    seen.insert(h.clone());
                    out.push(h.clone());
                    frontier.push(h.clone());
                }
            }
        }
        out
    }

    /// Switch off and unlock. A reason means a failure did it, None a user
    /// action (which also drops a stale error from an earlier attempt).
    fn turn_off(&mut self, host: &str, reason: Option<String>) {
        self.switch_off(host, reason, &mut HashSet::new())
    }

    /// An attempt failed, or a connection that held dropped: the one way
    /// every such failure is taken. The next attempt is scheduled by the
    /// host's rule whatever happens next, since a host its switch goes off
    /// for may still be wanted as a jump host and must then wait before
    /// it tries again. A host that retries (`Master::retries`) says why
    /// and when it tries again, unless its rule gives up, or the link
    /// keeps dying right after coming up; any other host's switch goes
    /// off.
    fn attempt_failed(&mut self, host: &str, f: Failure) {
        self.fail(host, f, &mut HashSet::new())
    }

    /// `attempt_failed`, `seen` holding the hosts this failure has reached
    /// already (see `switch_off`).
    fn fail(&mut self, host: &str, f: Failure, seen: &mut HashSet<String>) {
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        let gave_up = m.failed(f.quick);
        if !m.retries() {
            self.switch_off(host, Some(f.off.unwrap_or(f.why)), seen);
        } else if let Some(why) = f
            .keeps_dropping
            .filter(|_| m.quick_failures >= config::MAX_QUICK_FAILURES)
        {
            self.switch_off(host, Some(why), seen);
        } else if gave_up {
            let why = m.gave_up(&f.why);
            self.switch_off(host, Some(why), seen);
        } else {
            log::warn(format!(
                "{host}: {}; retry in {}s",
                f.why,
                m.backoff.as_secs()
            ));
            m.last_error = Some(f.why);
        }
    }

    /// `turn_off`. A failure passes on to the hosts held back for this
    /// one as their jump host; `seen` holds the hosts it has reached, so
    /// that a ProxyJump loop does not pass it round for ever.
    fn switch_off(&mut self, host: &str, reason: Option<String>, seen: &mut HashSet<String>) {
        seen.insert(host.to_string());
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        if let Some(r) = &reason {
            m.last_error = Some(r.clone());
            log::warn(format!(
                "{host}: {} — {r}",
                if m.spec.enabled {
                    "switched off"
                } else {
                    "giving up"
                }
            ));
            // Hosts held back for this jump host fail the same way. Ones
            // already connected through it notice the drop themselves,
            // and one that failed for a reason of its own (a config
            // error, a loop) keeps that reason.
            let deps = self.dependents(host);
            for d in deps {
                if seen.contains(&d) {
                    continue;
                }
                let Some(dm) = self.masters.get_mut(&d) else {
                    continue;
                };
                let direct = dm
                    .spec
                    .resolved
                    .as_ref()
                    .and_then(|r| r.jump_alias.as_deref())
                    == Some(host);
                if !(dm.wanted() && !dm.running() && !dm.foreign && dm.fatal.is_none() && direct) {
                    continue;
                }
                self.fail(
                    &d,
                    Failure::new(true, format!("jump host {host}: {r}")),
                    seen,
                );
            }
        }
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        if reason.is_none() {
            m.last_error = None;
        }
        m.ever_established = false;
        m.waiting_for_jump = None;
        m.spec.locked = false;
        if !m.spec.enabled {
            return;
        }
        m.spec.enabled = false;
        self.share();
        (self.on_disabled)(host.to_string(), reason);
    }

    /// The ProxyJump chain from `host` when it comes back to a host it has
    /// passed (A through B, B through A): no master in it can ever come
    /// up, each waiting for the next. The chain is followed through the
    /// hosts this daemon knows; one that names itself as its jump is left
    /// to ssh, as everywhere else here.
    fn jump_loop(&self, host: &str) -> Option<Vec<String>> {
        let mut chain = vec![host.to_string()];
        let mut at = host.to_string();
        loop {
            let next = self
                .masters
                .get(&at)?
                .spec
                .resolved
                .as_ref()?
                .jump_alias
                .clone()?;
            if next == at || !self.masters.contains_key(&next) {
                return None;
            }
            let looped = chain.contains(&next);
            chain.push(next.clone());
            if looped {
                return Some(chain);
            }
            at = next;
        }
    }

    /// Work out which hosts are needed as jump hosts and start or stop the
    /// masters that are kept up only for that reason.
    fn reconcile_jump_hosts(&mut self) {
        let mut needed: HashMap<String, Vec<String>> = HashMap::new();
        let mut visited = HashSet::new();
        let mut stack: Vec<String> = self
            .order
            .iter()
            .filter(|h| {
                self.masters
                    .get(*h)
                    .map(|m| m.spec.enabled)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        while let Some(h) = stack.pop() {
            if !visited.insert(h.clone()) {
                continue;
            }
            let Some(m) = self.masters.get(&h) else {
                continue;
            };
            if let Some(j) = m.spec.resolved.as_ref().and_then(|r| r.jump_alias.clone()) {
                if j != h && self.masters.contains_key(&j) {
                    needed.entry(j.clone()).or_default().push(h.clone());
                    stack.push(j);
                }
            }
        }
        let order = self.order.clone();
        let mut stopping = vec![];
        for h in &order {
            let by = needed.get(h).cloned().unwrap_or_default();
            let needed_by: Vec<String> = order.iter().filter(|x| by.contains(x)).cloned().collect();
            let Some(m) = self.masters.get_mut(h) else {
                continue;
            };
            // Up only for others until now, as the log says when it goes.
            let was_jump_host = m.implicit();
            m.needed_by = needed_by;
            let wanted = m.wanted();
            if wanted && !m.was_wanted && m.implicit() {
                m.reset();
                log::info(format!(
                    "{h}: needed as the jump host for {}, connecting",
                    m.needed_by.join(", ")
                ));
            } else if !wanted && m.was_wanted {
                if m.running() && was_jump_host {
                    log::info(format!("{h}: no longer needed as a jump host, stopping"));
                }
                stopping.extend(m.begin_stop());
                m.foreign = false;
                m.next_probe = None;
            }
            m.was_wanted = wanted;
        }
        finish_stops(stopping);
    }

    fn tick_all(&mut self) {
        self.checks_left = CHECKS_PER_TICK;
        self.reconcile_jump_hosts();
        let order = self.order.clone();
        for h in &order {
            self.tick(h);
        }
        let snap = self.snapshot();
        *self.published.lock().unwrap_or_else(|e| e.into_inner()) = snap.clone();
        if snap != self.last_snapshot {
            self.last_snapshot = snap.clone();
            (self.on_change)(snap);
        }
    }

    fn tick(&mut self, host: &str) {
        let now = Instant::now();
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        if m.proc_.is_none() {
            if m.wanted() {
                self.tick_start(host, now);
            } else {
                self.tick_idle(host, now);
            }
        } else if m.running() {
            self.tick_running(host, now);
        } else {
            self.tick_exited(host, now);
        }
    }

    /// A master whose process runs: still logging in, or up and checked
    /// every `HEALTH_INTERVAL`.
    fn tick_running(&mut self, host: &str, now: Instant) {
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        if !m.established {
            // Still authenticating: the socket only appears once auth
            // succeeds, so its absence is not a fault yet. A master
            // without a socket says so itself ("Authenticated to").
            let up = if m.socket() {
                m.socket_present() && ssh::master_alive(host)
            } else {
                m.proc_.as_ref().map(|p| p.authenticated()).unwrap_or(false)
            };
            if up {
                m.established = true;
                m.established_at = Some(SystemTime::now());
                m.ever_established = true;
                m.quick_failures = 0;
                m.backoff = Duration::ZERO;
                m.failed_at = None;
                m.dropped_at = None;
                m.last_check = now;
                m.last_error = None;
                let pid = m.proc_.as_ref().map(|p| p.pid).unwrap_or(0);
                log::info(format!(
                    "{host}: master established (pid {pid}) [{}]",
                    m.idle_mode.title()
                ));
            } else if now.duration_since(m.started) > config::CONNECT_TIMEOUT {
                let pid = m.proc_.as_ref().map(|p| p.pid).unwrap_or(0);
                log::warn(format!(
                    "{host}: master pid {pid} did not come up in {}s, killing",
                    config::CONNECT_TIMEOUT.as_secs()
                ));
                m.stop();
                self.attempt_failed(
                    host,
                    Failure::new(true, "timed out while reconnecting")
                        .off("timed out while connecting"),
                );
            }
            return;
        }
        // A master without a socket has no check but its own process:
        // ServerAlive ends it when the link is dead.
        if !m.socket() || now.duration_since(m.last_check) < config::HEALTH_INTERVAL {
            return;
        }
        // Without its socket file the master is of use to nobody, and no
        // check is needed to say so.
        let alive = if !m.socket_present() {
            false
        } else if self.checks_left == 0 {
            return; // a later tick
        } else {
            self.checks_left -= 1;
            ssh::master_alive(host)
        };
        m.last_check = now;
        if !alive {
            let pid = m.proc_.as_ref().map(|p| p.pid).unwrap_or(0);
            log::warn(format!("{host}: master pid {pid} unresponsive, killing"));
            m.stop();
            self.attempt_failed(host, Failure::new(false, "master stopped responding"));
        }
    }

    /// A master whose process has exited: why, and what follows.
    fn tick_exited(&mut self, host: &str, now: Instant) {
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        let Some(mut p) = m.proc_.take() else {
            return;
        };
        let life = now.duration_since(m.started);
        let rc = p.exit_code();
        let was_established = m.established;
        let last_line = p.last_line();
        m.established = false;
        let established_at = m.established_at.take();
        if !m.wanted() {
            log::info(format!(
                "{host}: master exited rc={rc} after {}s",
                life.as_secs()
            ));
            return;
        }
        if !was_established {
            // An attempt that got nowhere. One that reconnects after a
            // drop (the network still down?) keeps trying, as long as the
            // rule allows.
            log::info(format!(
                "{host}: master exited rc={rc} after {}s without coming up",
                life.as_secs()
            ));
            let why = last_line.unwrap_or_else(|| format!("ssh exited with status {rc}"));
            self.attempt_failed(
                host,
                Failure::new(true, format!("reconnect failed: {why}"))
                    .off(format!("could not connect: {why}")),
            );
            return;
        }
        let uptime = established_at
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let detail = last_line.map(|l| format!(" — {l}")).unwrap_or_default();
        // Not locked, the connection lasted as long as it lasted; locked,
        // it is brought back.
        log::info(format!(
            "{host}: master exited rc={rc} after {}s ({uptime}s up){}",
            life.as_secs(),
            if m.spec.locked { "" } else { ", not locked" }
        ));
        self.attempt_failed(
            host,
            Failure::new(
                life < config::SHORT_LIFE,
                format!(
                    "connection dropped after {} (rc {rc}){detail}",
                    duration(life)
                ),
            )
            .keeps_dropping(format!(
                "connection keeps dropping right after connecting (rc {rc}){detail}"
            )),
        );
    }

    /// A host nobody wants connected: still notice a master the user
    /// started interactively, which owns its socket. There is nothing to
    /// ask about while the socket file is not there.
    fn tick_idle(&mut self, host: &str, now: Instant) {
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        m.waiting_for_jump = None;
        if m.next_probe.is_some_and(|t| now < t) {
            return;
        }
        if !m.socket() || !m.socket_present() {
            m.foreign = false;
        } else if self.checks_left == 0 {
            return; // a later tick
        } else {
            self.checks_left -= 1;
            m.foreign = ssh::master_alive(host);
        }
        m.next_probe = Some(now + config::HEALTH_INTERVAL);
    }

    /// A host that is wanted and has no process: its next attempt, once
    /// its wait is over and its jump host is up.
    fn tick_start(&mut self, host: &str, now: Instant) {
        let Some(m) = self.masters.get_mut(host) else {
            return;
        };
        if m.next_try.is_some_and(|t| now < t) {
            return;
        }

        // A chain of jump hosts that comes back on itself would leave
        // every host in it waiting for the next for ever: said once, as a
        // config error is, until the config or the switch changes.
        if let Some(chain) = self.jump_loop(host) {
            let f = format!("ProxyJump goes round in a loop: {}", chain.join(" → "));
            let m = self.masters.get_mut(host).expect("master");
            if m.fatal.as_deref() != Some(f.as_str()) {
                m.fatal = Some(f.clone());
                self.turn_off(host, Some(f));
            }
            return;
        }

        // Bring the jump host up first (reconcile_jump_hosts made it wanted)
        // so the ProxyJump hop goes through our master instead of starting a
        // throw-away one of its own. A jump host that cannot be managed at
        // all (fatal: config error) is left to ssh.
        let m = self.masters.get_mut(host).expect("master");
        let jump = m.spec.resolved.as_ref().and_then(|r| r.jump_alias.clone());
        if let Some(j) = jump {
            if j != host {
                let hold = match self.masters.get_mut(&j) {
                    Some(jm) => jm.wanted() && jm.fatal.is_none() && !jm.status().state.is_up(),
                    None => false,
                };
                let m = self.masters.get_mut(host).expect("master");
                if hold {
                    if m.waiting_for_jump.is_none() {
                        log::info(format!("{host}: waiting for jump host {j}"));
                    }
                    m.waiting_for_jump = Some(j);
                    return;
                }
            }
        }
        let m = self.masters.get_mut(host).expect("master");
        m.waiting_for_jump = None;

        if let Some(err) = m.spec.resolve_error.clone() {
            let f = format!("ssh config error: {err}");
            m.fatal = Some(f.clone());
            self.turn_off(host, Some(f));
            return;
        }
        let Some(resolved) = m.spec.resolved.clone() else {
            m.next_try = Some(now + config::POLL_INTERVAL); // still resolving
            return;
        };
        if let Some(pid) = (ssh::multiplexed(&resolved) && m.socket_present())
            .then(|| ssh::master_pid(host))
            .flatten()
        {
            if ssh::is_our_master(pid) {
                // Left behind by an earlier shellder that did not shut down
                // cleanly (crash, kill -9). Replace it with one we control.
                log::warn(format!(
                    "{host}: socket owned by an orphaned shellder master (pid {pid}); replacing it"
                ));
                let r = ssh::run(&["-O", "exit", host]);
                if r.status != 0 {
                    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                }
                thread::sleep(Duration::from_millis(500));
            } else {
                // Some other master (e.g. ControlMaster=auto from an interactive
                // ssh) owns the socket; leave it alone until it goes away.
                if !m.foreign {
                    log::info(format!(
                        "{host}: socket owned by another master (pid {pid}), not spawning"
                    ));
                }
                m.foreign = true;
                m.ever_established = true;
                m.next_try = Some(now + config::HEALTH_INTERVAL);
                return;
            }
        }
        m.foreign = false;
        match ssh::spawn_master(host, &resolved, m.idle_mode, self.handler.clone()) {
            Ok(p) => {
                m.proc_ = Some(p);
                m.started = now;
                m.started_at = SystemTime::now();
                m.last_check = now;
                m.established = false;
                m.established_at = None;
                m.fatal = None;
            }
            Err(e) => {
                log::error(format!("{host}: {e}"));
                m.proc_ = None;
                self.attempt_failed(host, Failure::new(true, e));
            }
        }
    }
}

/// "5s", "3m 2s", "2h 5m", "1d 3h".
pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {}s", s / 60, s % 60)
    } else if s < 86400 {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d {}h", s / 86400, (s % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    //! The daemon's own decisions, on masters made here: nothing in these
    //! starts an ssh, reads the prefs or writes the log.
    use super::*;

    struct Nobody;

    impl PromptHandler for Nobody {
        fn ask(&self, _req: &crate::core::prompt::PromptRequest) -> Option<String> {
            None
        }
    }

    fn spec(alias: &str, jump: Option<&str>, control_path: Option<&str>) -> HostSpec {
        HostSpec {
            alias: alias.into(),
            enabled: false,
            locked: false,
            resolved: Some(ResolvedHost {
                hostname: format!("{alias}.example"),
                jump_alias: jump.map(String::from),
                control_path: control_path.map(String::from),
                ..ResolvedHost::default()
            }),
            resolve_error: None,
        }
    }

    fn inner(specs: Vec<HostSpec>) -> Inner {
        Inner {
            order: specs.iter().map(|s| s.alias.clone()).collect(),
            masters: specs
                .into_iter()
                .map(|s| (s.alias.clone(), Master::with_idle_mode(s, IdleMode::None)))
                .collect(),
            last_snapshot: vec![],
            published: Arc::new(Mutex::new(vec![])),
            checks_left: CHECKS_PER_TICK,
            handler: Arc::new(Nobody),
            on_change: Box::new(|_| {}),
            on_disabled: Box::new(|_, _| {}),
        }
    }

    #[test]
    fn a_jump_chain_that_comes_back_on_itself_is_a_loop() {
        let d = inner(vec![
            spec("a", Some("b"), None),
            spec("b", Some("a"), None),
            spec("c", Some("a"), None),
            spec("d", Some("d"), None),
            spec("e", Some("elsewhere"), None),
            spec("f", Some("c"), None),
            spec("g", None, None),
        ]);
        let chain = |h: &str| d.jump_loop(h).map(|c| c.join(" "));
        assert_eq!(chain("a").as_deref(), Some("a b a"));
        assert_eq!(chain("b").as_deref(), Some("b a b"));
        // Into a loop it is not part of.
        assert_eq!(chain("c").as_deref(), Some("c a b a"));
        assert_eq!(chain("f").as_deref(), Some("f c a b a"));
        // Through itself (left to ssh), out of the list, or no jump.
        assert_eq!(chain("d"), None);
        assert_eq!(chain("e"), None);
        assert_eq!(chain("g"), None);
    }

    #[test]
    fn no_socket_file_no_check() {
        let dir = std::env::temp_dir().join(format!("shellder-daemon-cp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let there = dir.join("cm-there");
        std::fs::write(&there, b"").unwrap();
        let missing = dir.join("cm-missing");
        let master = |cp: Option<&str>| Master::with_idle_mode(spec("h", None, cp), IdleMode::None);
        assert!(master(there.to_str()).socket_present());
        assert!(!master(missing.to_str()).socket_present());
        assert!(!master(None).socket_present());
        // Not expanded by ssh: nothing to tell from the name, so checked.
        assert!(master(Some("/tmp/cm-%C")).socket_present());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
