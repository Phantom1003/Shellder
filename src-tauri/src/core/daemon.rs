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
use std::sync::Arc;
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
}

impl Master {
    fn new(spec: HostSpec) -> Master {
        let idle_mode = prefs::idle_mode(&spec.alias)
            .and_then(|m| IdleMode::parse(&m))
            .unwrap_or(IdleMode::None);
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
        if let Some(mut p) = self.proc_.take() {
            if p.is_running() {
                log::info(format!("{}: stopping master pid {}", self.host(), p.pid));
                p.terminate();
                let deadline = Instant::now() + Duration::from_secs(5);
                while p.is_running() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(100));
                }
                if p.is_running() {
                    p.kill();
                }
            }
        }
        self.established = false;
        self.established_at = None;
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
}

impl Daemon {
    pub fn start(
        handler: Arc<dyn PromptHandler>,
        on_change: OnChange,
        on_disabled: OnDisabled,
    ) -> Daemon {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("shellder-daemon".into())
            .spawn(move || {
                let mut d = Inner {
                    masters: HashMap::new(),
                    order: vec![],
                    last_snapshot: vec![],
                    handler,
                    on_change,
                    on_disabled,
                };
                d.run(rx);
            })
            .expect("daemon thread");
        Daemon { tx }
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
    pub fn snapshot(&self) -> Vec<HostStatus> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Snapshot(tx));
        rx.recv().unwrap_or_default()
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
    handler: Arc<dyn PromptHandler>,
    on_change: OnChange,
    on_disabled: OnDisabled,
}

impl Inner {
    fn run(&mut self, rx: Receiver<Cmd>) {
        config::ensure_dirs();
        log::info(format!("daemon starting (pid {})", std::process::id()));
        ssh::kill_orphaned_masters();
        loop {
            match rx.recv_timeout(config::POLL_INTERVAL) {
                Ok(Cmd::Shutdown(done)) => {
                    log::info("daemon stopping");
                    for m in self.masters.values_mut() {
                        m.stop();
                    }
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
                let hosts: Vec<String> = self.order.clone();
                for h in hosts {
                    let Some(m) = self.masters.get_mut(&h) else {
                        continue;
                    };
                    if m.spec.enabled {
                        m.stop();
                        m.foreign = false;
                        self.turn_off(&h, None);
                    }
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
        for alias in gone {
            if let Some(mut m) = self.masters.remove(&alias) {
                m.stop();
            }
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

    fn stop_with_dependents(&mut self, host: &str) {
        for d in self.dependents(host) {
            let Some(m) = self.masters.get_mut(&d) else {
                continue;
            };
            if !m.wanted() {
                continue;
            }
            log::info(format!(
                "{d}: disconnected together with its jump host {host}"
            ));
            m.stop();
            m.foreign = false;
            self.turn_off(&d, None);
        }
        if let Some(m) = self.masters.get_mut(host) {
            m.stop();
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
            // already connected through it notice the drop themselves.
            let deps = self.dependents(host);
            for d in deps {
                let Some(dm) = self.masters.get_mut(&d) else {
                    continue;
                };
                let direct = dm
                    .spec
                    .resolved
                    .as_ref()
                    .and_then(|r| r.jump_alias.as_deref())
                    == Some(host);
                if !(dm.wanted() && !dm.running() && !dm.foreign && direct) {
                    continue;
                }
                let why = format!("jump host {host}: {r}");
                if dm.retries() {
                    dm.last_error = Some(why.clone());
                    if dm.failed(true) {
                        let why = dm.gave_up(&why);
                        self.turn_off(&d, Some(why));
                    } else {
                        log::warn(format!("{d}: {why}, retry in {}s", dm.backoff.as_secs()));
                    }
                } else {
                    self.turn_off(&d, Some(why));
                }
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
        (self.on_disabled)(host.to_string(), reason);
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
        for h in &order {
            let by = needed.get(h).cloned().unwrap_or_default();
            let needed_by: Vec<String> = order.iter().filter(|x| by.contains(x)).cloned().collect();
            let Some(m) = self.masters.get_mut(h) else {
                continue;
            };
            m.needed_by = needed_by;
            let wanted = m.wanted();
            if wanted && !m.was_wanted && m.implicit() {
                m.reset();
                log::info(format!(
                    "{h}: needed as the jump host for {}, connecting",
                    m.needed_by.join(", ")
                ));
            } else if !wanted && m.was_wanted {
                if m.running() && !by.is_empty() {
                    log::info(format!("{h}: no longer needed as a jump host, stopping"));
                }
                m.stop();
                m.foreign = false;
                m.next_probe = None;
            }
            m.was_wanted = wanted;
        }
    }

    fn tick_all(&mut self) {
        self.reconcile_jump_hosts();
        let order = self.order.clone();
        for h in &order {
            self.tick(h);
        }
        let snap = self.snapshot();
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
        if m.proc_.is_some() {
            if m.running() {
                if !m.established {
                    // Still authenticating: the socket only appears once auth
                    // succeeds, so its absence is not a fault yet. A master
                    // without a socket says so itself ("Authenticated to").
                    let up = if m.socket() {
                        ssh::master_alive(host)
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
                        if m.retries() {
                            let why = "timed out while reconnecting";
                            m.last_error = Some(why.into());
                            if m.failed(true) {
                                let why = m.gave_up(why);
                                self.turn_off(host, Some(why));
                            }
                        } else {
                            m.failed(true);
                            self.turn_off(host, Some("timed out while connecting".into()));
                        }
                    }
                } else if m.socket() && now.duration_since(m.last_check) >= config::HEALTH_INTERVAL
                {
                    // A master without a socket has no check but its own
                    // process: ServerAlive ends it when the link is dead.
                    m.last_check = now;
                    if !ssh::master_alive(host) {
                        let pid = m.proc_.as_ref().map(|p| p.pid).unwrap_or(0);
                        log::warn(format!("{host}: master pid {pid} unresponsive, killing"));
                        m.stop();
                        if m.spec.locked {
                            m.last_error = Some("master stopped responding".into());
                            m.failed(false);
                        } else {
                            self.turn_off(host, Some("master stopped responding".into()));
                        }
                    }
                }
                return;
            }
            // exited
            let life = now.duration_since(m.started);
            let mut p = m.proc_.take().expect("proc");
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
                let why = last_line.unwrap_or_else(|| format!("ssh exited with status {rc}"));
                if m.retries() {
                    // Reconnect after a drop failed (network still down?):
                    // keep trying, as long as the rule allows.
                    m.last_error = Some(format!("reconnect failed: {why}"));
                    if m.failed(true) {
                        let why = m.gave_up(&format!("reconnect failed: {why}"));
                        self.turn_off(host, Some(why));
                    } else {
                        log::warn(format!(
                            "{host}: reconnect failed rc={rc}; retry in {}s",
                            m.backoff.as_secs()
                        ));
                    }
                } else {
                    m.failed(true);
                    self.turn_off(host, Some(format!("could not connect: {why}")));
                }
                return;
            }
            let uptime = established_at
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let detail = last_line.map(|l| format!(" — {l}")).unwrap_or_default();
            if !m.spec.locked {
                // Not locked: the connection lasted as long as it lasted.
                log::info(format!(
                    "{host}: master exited rc={rc} after {}s ({uptime}s up), not locked",
                    life.as_secs()
                ));
                self.turn_off(
                    host,
                    Some(format!(
                        "connection dropped after {} (rc {rc}){detail}",
                        duration(life)
                    )),
                );
                return;
            }
            let gave_up = m.failed(life < config::SHORT_LIFE);
            if m.quick_failures >= config::MAX_QUICK_FAILURES {
                self.turn_off(
                    host,
                    Some(format!(
                        "connection keeps dropping right after connecting (rc {rc}){detail}"
                    )),
                );
            } else if gave_up {
                let why = m.gave_up(&format!(
                    "connection dropped after {} (rc {rc}){detail}",
                    duration(life)
                ));
                self.turn_off(host, Some(why));
            } else {
                m.last_error = Some(format!(
                    "connection dropped after {} (rc {rc}){detail}",
                    duration(life)
                ));
                log::warn(format!(
                    "{host}: master exited rc={rc} after {}s ({uptime}s up); retry in {}s",
                    life.as_secs(),
                    m.backoff.as_secs()
                ));
            }
            return;
        }

        if !m.wanted() {
            // Idle host: still notice a master the user started interactively.
            m.waiting_for_jump = None;
            if m.next_probe.map(|t| now >= t).unwrap_or(true) {
                m.next_probe = Some(now + config::HEALTH_INTERVAL);
                m.foreign = m.socket() && ssh::master_alive(host);
            }
            return;
        }
        if m.next_try.map(|t| now < t).unwrap_or(false) {
            return;
        }

        // Bring the jump host up first (reconcile_jump_hosts made it wanted)
        // so the ProxyJump hop goes through our master instead of starting a
        // throw-away one of its own. A jump host that cannot be managed at
        // all (fatal: config error) is left to ssh.
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
        if let Some(pid) = ssh::multiplexed(&resolved)
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
                if m.retries() {
                    m.last_error = Some(e.clone());
                    if m.failed(true) {
                        let why = m.gave_up(&e);
                        self.turn_off(host, Some(why));
                    }
                } else {
                    self.turn_off(host, Some(e));
                }
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
