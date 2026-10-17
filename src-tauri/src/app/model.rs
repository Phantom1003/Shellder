//! Everything the windows show, and the answers to what ssh asks. One
//! shared object: the daemon, the pty readers and the commands all talk to
//! it, and every change is pushed to the windows as one snapshot.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::core::catalog::{self, HostEntry, ResolvedHost};
use crate::core::config;
use crate::core::daemon::{Daemon, HostSpec, HostStatus};
use crate::core::keychain::{self, SecretKind, VaultRead};
use crate::core::log;
use crate::core::prefs;
use crate::core::prompt::{self, Kind, Origin, PromptHandler, PromptRequest};
use crate::core::reconnect;
use crate::core::ssh::IdleMode;
use crate::core::totp::Totp;
use crate::core::{autostart, platform, session, ssh};

/// A question ssh asked that the stored credentials could not answer, as
/// the prompt window shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptView {
    pub id: u64,
    pub host: String,
    /// password | passphrase | totp | confirm | other
    pub kind: String,
    pub prompt: String,
    pub allow_save: bool,
    /// What is asking, when it is not ssh logging in: the Console's action
    /// the host wants a password for (`kill -TERM 1234`, `mount /dev/sdb1`).
    pub about: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptResponse {
    pub answer: Option<String>,
    pub save: bool,
}

struct PendingPrompt {
    view: PromptView,
    run_id: u32,
    reply: Sender<PromptResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    /// idle | checking | upToDate | available | downloading | installing | failed
    pub state: String,
    pub version: Option<String>,
    pub page: Option<String>,
    pub notes: Option<String>,
    pub progress: Option<f64>,
    pub message: Option<String>,
}

impl Default for UpdateView {
    fn default() -> Self {
        UpdateView {
            state: "idle".into(),
            version: None,
            page: None,
            notes: None,
            progress: None,
            message: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub keep_in_background: bool,
    pub show_menu_bar_icon: bool,
    pub invert_menu_bar_icon: bool,
    pub silent_launch: bool,
    pub language: String,
    pub launched_language: String,
    pub start_at_login: bool,
    pub auto_update: bool,
    /// The login item points at another copy of the app.
    pub autostart_other_copy: Option<String>,
    pub error: Option<String>,
    /// "macos" or "linux": the page hides what a platform cannot use.
    pub platform: &'static str,
    /// How a locked host is brought back after a drop: the rule every
    /// host follows, and the hosts that have one of their own, each
    /// spelled out whole (`core::reconnect`).
    pub reconnect: reconnect::Rule,
    pub host_reconnect: BTreeMap<String, reconnect::Rule>,
}

/// What this platform's ssh can do, for the pages: without ControlMaster
/// a master owns no socket, so a shell or a Files window logs in on its
/// own, and the ControlPath advice is beside the point.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformView {
    pub control_master: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    pub ssh_config: String,
    pub log: String,
    pub state_dir: String,
    pub home: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Constants {
    pub health_interval: u64,
    pub connect_timeout: u64,
}

/// What every window draws from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: String,
    pub hosts: Vec<HostEntry>,
    pub resolved: BTreeMap<String, ResolvedHost>,
    pub resolve_errors: BTreeMap<String, String>,
    pub statuses: BTreeMap<String, HostStatus>,
    pub enabled: Vec<String>,
    pub locked: Vec<String>,
    pub secrets: BTreeMap<String, Vec<SecretKind>>,
    pub keychain_refusal: Option<String>,
    pub config_files: Vec<String>,
    pub reloading: bool,
    pub prompt: Option<PromptView>,
    pub pending_prompts: usize,
    pub settings: SettingsView,
    pub update: UpdateView,
    pub platform: PlatformView,
    pub paths: Paths,
    pub constants: Constants,
    /// Hosts whose master is up with a socket: what other ssh commands,
    /// the shells and the Files windows go through.
    pub connected: Vec<String>,
    pub idle_modes: Vec<String>,
}

#[derive(Default)]
struct State {
    hosts: Vec<HostEntry>,
    resolved: HashMap<String, ResolvedHost>,
    resolve_errors: HashMap<String, String>,
    config_files: Vec<String>,
    mtimes: HashMap<String, Option<SystemTime>>,
    reloaded_once: bool,
    reloading: bool,
    statuses: HashMap<String, HostStatus>,
    /// The Connect switches. Not remembered across launches.
    enabled: HashSet<String>,
    /// Locked hosts: always a subset of `enabled`. Remembered in the prefs.
    locked: HashSet<String>,
    secret_presence: HashMap<String, Vec<SecretKind>>,
    keychain_refusal: Option<String>,
    prompts: Vec<PendingPrompt>,
    next_prompt_id: u64,
    /// Stored secrets handed to a particular master, keyed by
    /// "runid|host|kind": the same process asking the same question again
    /// means the server rejected the answer.
    recent_answers: HashMap<String, (Instant, u32)>,
    update: UpdateView,
    settings_error: Option<String>,
    prefs_seen: Option<SystemTime>,
    vault_stamp_seen: Option<SystemTime>,
}

pub struct Model {
    state: Mutex<State>,
    daemon: OnceLock<Daemon>,
    app: OnceLock<AppHandle>,
    worker: Mutex<Option<Sender<Job>>>,
    /// The language this process started with. Settings compares the
    /// choice against it to know when a relaunch is due.
    pub launched_language: String,
    /// Opened by the windows module when a prompt is waiting.
    pub on_prompt: OnceLock<Box<dyn Fn(bool) + Send + Sync>>,
    pub on_settings_changed: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

pub type Shared = Arc<Model>;
type Job = Box<dyn FnOnce() + Send>;

impl Model {
    pub fn new() -> Shared {
        Arc::new(Model {
            state: Mutex::new(State::default()),
            daemon: OnceLock::new(),
            app: OnceLock::new(),
            worker: Mutex::new(None),
            launched_language: prefs::load().language(),
            on_prompt: OnceLock::new(),
            on_settings_changed: OnceLock::new(),
        })
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn daemon(&self) -> &Daemon {
        self.daemon.get().expect("daemon started")
    }

    pub fn app(&self) -> Option<&AppHandle> {
        self.app.get()
    }

    /// Run `f` later, one thing at a time, off the caller's thread. What
    /// the Swift app did with DispatchQueue.main.async: the daemon and the
    /// pty readers must never wait on the model from inside their own
    /// callbacks.
    pub fn later(&self, f: impl FnOnce() + Send + 'static) {
        if let Some(tx) = self
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = tx.send(Box::new(f));
        }
    }

    // MARK: lifecycle

    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let _ = self.app.set(app);
        let (tx, rx) = mpsc::channel::<Job>();
        *self.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        std::thread::Builder::new()
            .name("shellder-model".into())
            .spawn(move || {
                for f in rx {
                    f();
                }
            })
            .expect("worker");

        {
            let p = prefs::load();
            let mut st = self.st();
            st.locked = p.locked_hosts.iter().cloned().collect();
            st.enabled = st.locked.clone();
            st.prefs_seen = prefs::modified();
            st.vault_stamp_seen = keychain::stamp_state().map(|s| s.0);
        }
        // The sessions the Files windows open ask the same questions as a
        // master, and get the same answers.
        session::set_prompt_handler(self.clone());

        let me = self.clone();
        let on_change: crate::core::daemon::OnChange = Box::new(move |snap| {
            {
                let mut st = me.st();
                st.statuses = snap.into_iter().map(|s| (s.host.clone(), s)).collect();
            }
            me.publish();
        });
        let me = self.clone();
        let on_disabled: crate::core::daemon::OnDisabled = Box::new(move |_host, _reason| {
            let me2 = me.clone();
            me.later(move || me2.switch_changed_by_daemon());
        });
        let handler: Arc<dyn PromptHandler> = self.clone();
        let _ = self
            .daemon
            .set(Daemon::start(handler, on_change, on_disabled));

        self.reload_catalog(true);

        // Every few seconds: the ssh config, the prefs the CLI may have
        // edited, and the vault another process may have written.
        let me = self.clone();
        std::thread::Builder::new()
            .name("shellder-poll".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                me.reload_catalog(false);
                me.sync_locked_from_prefs();
                me.notice_vault_writes();
            })
            .expect("poll");
    }

    /// Stop everything the app started, and wait for it to be gone:
    /// macOS 26 holds an app as still running while a process of its is
    /// alive after it exited.
    pub fn shutdown(&self) {
        for f in super::files::all() {
            f.cancel();
        }
        super::terminals::close_all();
        session::close_all();
        if let Some(d) = self.daemon.get() {
            d.shutdown();
        }
    }

    // MARK: snapshot

    pub fn snapshot(&self) -> Snapshot {
        let st = self.st();
        let p = prefs::load();
        let c = config::get();
        let mut hosts_sorted: Vec<&String> = st.hosts.iter().map(|h| &h.alias).collect();
        hosts_sorted.dedup();
        let connected: Vec<String> = st
            .hosts
            .iter()
            .map(|h| h.alias.clone())
            .filter(|a| {
                st.statuses
                    .get(a)
                    .map(|s| s.state.is_up() && s.socket)
                    .unwrap_or(false)
            })
            .collect();
        let autostart_other_copy = if autostart::installed() {
            autostart::installed_path().filter(|p| std::path::Path::new(p) != c.self_path)
        } else {
            None
        };
        Snapshot {
            version: env!("CARGO_PKG_VERSION").to_string(),
            hosts: st.hosts.clone(),
            resolved: st
                .resolved
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            resolve_errors: st
                .resolve_errors
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            statuses: st
                .statuses
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            enabled: st.enabled.iter().cloned().collect(),
            locked: st.locked.iter().cloned().collect(),
            secrets: st
                .secret_presence
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            keychain_refusal: st.keychain_refusal.clone(),
            config_files: st.config_files.clone(),
            reloading: st.reloading,
            prompt: st.prompts.first().map(|p| p.view.clone()),
            pending_prompts: st.prompts.len(),
            settings: SettingsView {
                keep_in_background: p.keep_in_background,
                show_menu_bar_icon: p.show_menu_bar_icon(),
                invert_menu_bar_icon: p.invert_menu_bar_icon,
                silent_launch: p.silent_launch,
                language: p.language(),
                launched_language: self.launched_language.clone(),
                start_at_login: autostart::installed(),
                auto_update: p.auto_update(),
                autostart_other_copy,
                error: st.settings_error.clone(),
                platform: std::env::consts::OS,
                reconnect: reconnect::default_rule(&p),
                host_reconnect: reconnect::host_rules(&p),
            },
            update: st.update.clone(),
            platform: PlatformView {
                control_master: platform::control_master(),
            },
            paths: Paths {
                ssh_config: config::path_str(&c.ssh_config_file),
                log: config::path_str(&c.log_file),
                state_dir: config::path_str(&c.state_dir),
                home: config::home().to_string(),
            },
            constants: Constants {
                health_interval: config::HEALTH_INTERVAL.as_secs(),
                connect_timeout: config::CONNECT_TIMEOUT.as_secs(),
            },
            connected,
            idle_modes: IdleMode::ALL.iter().map(|m| m.raw().to_string()).collect(),
        }
    }

    /// Push the state to every window.
    pub fn publish(&self) {
        if let Some(app) = self.app.get() {
            let _ = app.emit("state", self.snapshot());
        }
    }

    // MARK: catalog

    /// How many `ssh -G` run at once while the hosts are resolved.
    const RESOLVE_PARALLEL: usize = 8;

    /// Re-read ~/.ssh/config (and its includes) when any of them changed.
    pub fn reload_catalog(self: &Arc<Self>, force: bool) {
        {
            let mut st = self.st();
            let mut files = st.config_files.clone();
            let main = config::path_str(&config::get().ssh_config_file);
            if !files.contains(&main) {
                files.insert(0, main);
            }
            let now = catalog::modification_dates(&files);
            if !force && now == st.mtimes && (!st.hosts.is_empty() || st.reloaded_once) {
                return;
            }
            st.mtimes = now;
            if st.reloading {
                return;
            }
            st.reloading = true;
        }
        self.publish();
        let me = self.clone();
        std::thread::spawn(move || {
            let cat = catalog::load();
            // The list first, the resolutions when they are in: the window
            // shows its hosts (with "resolving…" under each new one) while
            // ssh -G runs for them, a few at a time.
            {
                let mut st = me.st();
                st.hosts = cat.entries.clone();
                st.config_files = cat.files.clone();
            }
            me.publish();
            let mut res: HashMap<String, ResolvedHost> = HashMap::new();
            let mut errs: HashMap<String, String> = HashMap::new();
            let aliases: Vec<String> = cat.entries.iter().map(|e| e.alias.clone()).collect();
            for chunk in aliases.chunks(Self::RESOLVE_PARALLEL) {
                let outcomes = std::thread::scope(|s| {
                    let handles: Vec<_> = chunk
                        .iter()
                        .map(|a| s.spawn(move || ssh::resolve(a)))
                        .collect();
                    handles
                        .into_iter()
                        .map(|h| {
                            h.join()
                                .unwrap_or_else(|_| Err("ssh -G did not finish".into()))
                        })
                        .collect::<Vec<_>>()
                });
                for (alias, outcome) in chunk.iter().zip(outcomes) {
                    match outcome {
                        Ok(r) => {
                            res.insert(alias.clone(), r);
                        }
                        Err(err) => {
                            errs.insert(alias.clone(), err);
                        }
                    }
                }
            }
            let aliases: HashSet<String> = aliases.into_iter().collect();
            for (alias, r) in res.iter_mut() {
                if let Some(hop) = r.first_jump_host() {
                    if aliases.contains(&hop) && &hop != alias {
                        r.jump_alias = Some(hop);
                    }
                }
            }
            {
                let mut st = me.st();
                st.hosts = cat.entries.clone();
                st.config_files = cat.files.clone();
                st.resolved = res;
                st.resolve_errors = errs;
                st.mtimes = catalog::modification_dates(&cat.files);
                st.reloaded_once = true;
                st.reloading = false;
            }
            me.refresh_secrets();
            let (e, l) = {
                let st = me.st();
                (st.enabled.clone(), st.locked.clone())
            };
            me.apply(e, l);
            log::info(format!(
                "ssh config loaded: {}",
                cat.entries
                    .iter()
                    .map(|e| e.alias.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        });
    }

    /// `shellder lock/unlock HOST` from a terminal edits the preferences
    /// behind our back. Pick that up with the same rules as the lock buttons.
    fn sync_locked_from_prefs(self: &Arc<Self>) {
        let modified = prefs::modified();
        {
            let mut st = self.st();
            if modified == st.prefs_seen {
                return;
            }
            st.prefs_seen = modified;
        }
        let now: HashSet<String> = prefs::locked_hosts().into_iter().collect();
        let (mut e, mut l) = {
            let st = self.st();
            if now == st.locked {
                return;
            }
            (st.enabled.clone(), st.locked.clone())
        };
        let mut sorted: Vec<&String> = now.iter().collect();
        sorted.sort();
        log::info(format!(
            "locked set changed outside the app: {}",
            sorted
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let locked = l.clone();
        for h in now.difference(&locked) {
            e.insert(h.clone());
            l.insert(h.clone());
        }
        for h in locked.difference(&now) {
            l.remove(h);
            for d in self.dependents(h) {
                l.remove(&d);
            }
        }
        self.apply(e, l);
        // The settings page follows the prefs too (a CLI `install` for one).
        self.publish();
    }

    /// Another process saved the vault: read it again.
    fn notice_vault_writes(self: &Arc<Self>) {
        let Some((modified, pid)) = keychain::stamp_state() else {
            return;
        };
        {
            let mut st = self.st();
            if st.vault_stamp_seen == Some(modified) {
                return;
            }
            st.vault_stamp_seen = Some(modified);
        }
        if pid == std::process::id() {
            return;
        }
        log::info("keychain: the vault was changed by another process, reading it again");
        keychain::invalidate();
        self.refresh_secrets();
    }

    // MARK: jump hosts

    /// The ProxyJump chain of `host` (nearest hop first), as far as it stays
    /// within the catalog.
    fn jump_chain_in(st: &State, host: &str) -> Vec<String> {
        let mut out = vec![];
        let mut seen: HashSet<String> = HashSet::from([host.to_string()]);
        let mut h = host.to_string();
        while let Some(j) = st.resolved.get(&h).and_then(|r| r.jump_alias.clone()) {
            if !seen.insert(j.clone()) {
                break;
            }
            out.push(j.clone());
            h = j;
        }
        out
    }

    /// Hosts whose ProxyJump chain passes through `host`.
    pub fn dependents(&self, host: &str) -> Vec<String> {
        let st = self.st();
        Self::dependents_in(&st, host)
    }

    fn dependents_in(st: &State, host: &str) -> Vec<String> {
        st.hosts
            .iter()
            .map(|h| h.alias.clone())
            .filter(|a| Self::jump_chain_in(st, a).iter().any(|j| j == host))
            .collect()
    }

    /// Make these the switched-on and the locked hosts, after the two rules
    /// every path shares: a locked host is switched on, and both take their
    /// jump hosts along. Stores the locks, logs the changes and pushes the
    /// host list to the daemon. The push happens even when the sets did not
    /// change: at launch and after a config reload the daemon needs the
    /// (re)resolved hosts anyway.
    fn apply(&self, e: HashSet<String>, l: HashSet<String>) {
        let specs = {
            let mut st = self.st();
            let with_jumps = |set: &HashSet<String>| {
                let mut out = set.clone();
                for h in set {
                    out.extend(Self::jump_chain_in(&st, h));
                }
                out
            };
            let new_locked = with_jumps(&l);
            let new_enabled: HashSet<String> = with_jumps(&e).union(&new_locked).cloned().collect();
            let stored: HashSet<String> = prefs::locked_hosts().into_iter().collect();
            for h in new_locked.difference(&stored) {
                prefs::set_locked(h, true);
            }
            for h in stored.difference(&new_locked) {
                prefs::set_locked(h, false);
            }
            let on: Vec<_> = new_enabled.difference(&st.enabled).cloned().collect();
            let off: Vec<_> = st.enabled.difference(&new_enabled).cloned().collect();
            let lock: Vec<_> = new_locked.difference(&st.locked).cloned().collect();
            let unlock: Vec<_> = st.locked.difference(&new_locked).cloned().collect();
            st.enabled = new_enabled;
            st.locked = new_locked;
            st.prefs_seen = prefs::modified();
            for h in st.hosts.iter().map(|h| h.alias.clone()).collect::<Vec<_>>() {
                if on.contains(&h) {
                    log::info(format!("{h}: switched on"));
                } else if off.contains(&h) {
                    log::info(format!("{h}: switched off"));
                }
                if lock.contains(&h) {
                    log::info(format!("{h}: locked"));
                } else if unlock.contains(&h) {
                    log::info(format!("{h}: unlocked"));
                }
            }
            st.hosts
                .iter()
                .map(|h| HostSpec {
                    alias: h.alias.clone(),
                    enabled: st.enabled.contains(&h.alias),
                    locked: st.locked.contains(&h.alias),
                    resolved: st.resolved.get(&h.alias).cloned(),
                    resolve_error: st.resolve_errors.get(&h.alias).cloned(),
                })
                .collect::<Vec<_>>()
        };
        self.daemon().configure(specs);
        self.publish();
    }

    // MARK: host actions

    pub fn is_enabled(&self, host: &str) -> bool {
        self.st().enabled.contains(host)
    }
    pub fn is_locked(&self, host: &str) -> bool {
        self.st().locked.contains(host)
    }

    /// The Connect switch. On: one attempt for this host, its jump hosts
    /// switched on with it. Off: close it, unlock it, and take down the
    /// hosts that jump through it, which cannot stay up without it.
    pub fn set_enabled(&self, host: &str, on: bool) {
        let (mut e, mut l) = {
            let st = self.st();
            (st.enabled.clone(), st.locked.clone())
        };
        if on {
            e.insert(host.to_string());
        } else {
            let mut gone = self.dependents(host);
            gone.push(host.to_string());
            for g in gone {
                e.remove(&g);
                l.remove(&g);
            }
        }
        self.apply(e, l);
    }

    /// The lock. On: switch the host on if it is off, and lock it and its
    /// jump hosts. Off: unlock it and the hosts that jump through it (their
    /// lock would lock this one again). The connection stays as it is.
    pub fn set_locked(&self, host: &str, on: bool) {
        let (mut e, mut l) = {
            let st = self.st();
            (st.enabled.clone(), st.locked.clone())
        };
        if on {
            e.insert(host.to_string());
            l.insert(host.to_string());
        } else {
            l.remove(host);
            for d in self.dependents(host) {
                l.remove(&d);
            }
        }
        self.apply(e, l);
    }

    /// The daemon flipped a switch itself (failed attempt, drop of an
    /// unlocked host, Close socket, Reconnect on an off host): mirror it in
    /// the preferences and the UI. The whole snapshot is taken over, not
    /// just one host: a jump host that failed turns its dependents off in
    /// the same breath.
    fn switch_changed_by_daemon(&self) {
        let snap = self.daemon().snapshot();
        let e = snap
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.host.clone())
            .collect();
        let l = snap
            .iter()
            .filter(|s| s.locked)
            .map(|s| s.host.clone())
            .collect();
        self.apply(e, l);
    }

    pub fn connect(&self, host: &str) {
        self.set_enabled(host, true)
    }
    pub fn disconnect(&self, host: &str) {
        self.daemon().disconnect(host)
    }
    pub fn reconnect(&self, host: &str) {
        self.daemon().reconnect(host)
    }
    pub fn close_socket(&self, host: &str) {
        self.daemon().close_socket(host)
    }
    pub fn set_idle_mode(&self, host: &str, mode: IdleMode) {
        self.daemon().set_idle_mode(host, mode)
    }
    pub fn connect_all(&self) {
        self.daemon().connect_all()
    }
    pub fn disconnect_all(&self) {
        self.daemon().disconnect_all()
    }

    pub fn status(&self, host: &str) -> Option<HostStatus> {
        self.st().statuses.get(host).cloned()
    }

    // MARK: secrets

    /// Ask the keychain for the vault now, cache or no cache: the button in
    /// Settings for when the dialog was dismissed at launch and every host
    /// looks like it has nothing stored.
    pub fn request_keychain_access(&self) {
        log::info("keychain: asking for the vault because Settings asked");
        keychain::read(true);
        self.refresh_secrets();
        match self.st().keychain_refusal.clone() {
            Some(r) => log::warn(format!("keychain: still refused: {r}")),
            None => {
                let n: usize = self.st().secret_presence.values().map(|v| v.len()).sum();
                log::info(format!(
                    "keychain: the vault opened; {n} secrets are readable"
                ));
            }
        }
    }

    pub fn refresh_secrets(&self) {
        let read = keychain::read(false);
        {
            let mut st = self.st();
            if let VaultRead::Refused(e) = read {
                // Leave what was known there: the secrets have not gone
                // anywhere, this run just cannot see them.
                st.keychain_refusal = Some(e);
                drop(st);
                self.publish();
                return;
            }
            st.keychain_refusal = None;
            let hosts: Vec<String> = st.hosts.iter().map(|h| h.alias.clone()).collect();
            st.secret_presence = hosts
                .into_iter()
                .map(|h| {
                    let kinds = SecretKind::ALL
                        .into_iter()
                        .filter(|k| keychain::has(&h, *k))
                        .collect();
                    (h, kinds)
                })
                .collect();
        }
        self.publish();
    }

    pub fn has_secret(&self, host: &str, kind: SecretKind) -> bool {
        self.st()
            .secret_presence
            .get(host)
            .map(|v| v.contains(&kind))
            .unwrap_or(false)
    }

    pub fn set_secret(&self, host: &str, kind: SecretKind, value: &str) -> Result<(), String> {
        if kind == SecretKind::Totp {
            Totp::parse(value)?;
        }
        keychain::set(host, kind, value)?;
        log::info(format!("{host}: {} stored", kind.raw()));
        self.refresh_secrets();
        Ok(())
    }

    pub fn remove_secret(&self, host: &str, kind: SecretKind) {
        match keychain::delete(host, kind) {
            Ok(()) => log::info(format!("{host}: {} removed", kind.raw())),
            // The vault could not be read, so nothing was written over it;
            // refresh_secrets shows the refusal.
            Err(e) => log::error(format!("{host}: could not remove the {}: {e}", kind.raw())),
        }
        self.refresh_secrets();
    }

    pub fn reveal_secret(&self, host: &str, kind: SecretKind) -> Option<String> {
        keychain::get(host, kind)
    }

    /// The live code for a host's TOTP secret, and the seconds left in its window.
    pub fn totp_code(&self, host: &str) -> Option<(String, u64)> {
        let raw = keychain::get(host, SecretKind::Totp)?;
        let t = Totp::parse(&raw).ok()?;
        Some((t.code(), t.seconds_remaining()))
    }

    // MARK: settings

    pub fn set_setting(&self, key: &str, value: serde_json::Value) -> Result<(), String> {
        let mut error = None;
        let mut rules_changed = false;
        let r = prefs::update(|p| match key {
            "keepInBackground" => {
                p.keep_in_background = value.as_bool().unwrap_or(false);
                // A silent start only makes sense for an app that lives in
                // the background; without it there would be no window.
                if !p.keep_in_background {
                    p.silent_launch = false;
                }
            }
            "showMenuBarIcon" => p.show_menu_bar_icon = Some(value.as_bool().unwrap_or(true)),
            "invertMenuBarIcon" => p.invert_menu_bar_icon = value.as_bool().unwrap_or(false),
            "silentLaunch" => p.silent_launch = value.as_bool().unwrap_or(false),
            "language" => p.language = value.as_str().map(String::from),
            "autoUpdate" => p.auto_update = Some(value.as_bool().unwrap_or(true)),
            "startAtLogin" => {
                let on = value.as_bool().unwrap_or(false);
                if on != autostart::installed() {
                    if on {
                        if let Err(e) = autostart::install(false) {
                            error = Some(e);
                        }
                    } else {
                        autostart::uninstall(false);
                    }
                }
            }
            "logPanelHeight" => p.log_panel_height = value.as_f64(),
            "transferPanelHeight" => p.transfer_panel_height = value.as_f64(),
            // The reconnect rule every host follows ("reconnect.first" …):
            // a null puts the built-in value back.
            k if k.starts_with("reconnect.") => {
                let r = &mut p.reconnect;
                match &k["reconnect.".len()..] {
                    "growth" => {
                        r.growth = value
                            .as_str()
                            .and_then(reconnect::Growth::parse)
                            .map(|g| g.raw().to_string())
                    }
                    "first" => r.first = value.as_u64(),
                    "max" => r.max = value.as_u64(),
                    "giveUp" => r.give_up = value.as_u64(),
                    _ => error = Some(format!("unknown setting {key}")),
                }
                rules_changed = true;
            }
            _ => error = Some(format!("unknown setting {key}")),
        });
        if let Err(e) = r {
            error = Some(e.to_string());
        }
        if rules_changed && error.is_none() {
            log::info(format!(
                "reconnect rule: {}",
                reconnect::default_rule(&prefs::load()).describe()
            ));
            self.daemon().rules_changed();
        }
        self.st().settings_error = error.clone();
        self.st().prefs_seen = prefs::modified();
        if let Some(f) = self.on_settings_changed.get() {
            f();
        }
        self.publish();
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// A host's own reconnect rule, or none to follow the default one.
    pub fn set_host_reconnect(&self, host: &str, rule: Option<reconnect::Partial>) {
        prefs::set_host_reconnect(host, rule);
        log::info(format!(
            "{host}: reconnect rule: {}",
            reconnect::for_host(host).describe()
        ));
        self.st().prefs_seen = prefs::modified();
        self.daemon().rules_changed();
        self.publish();
    }

    pub fn set_update(&self, u: UpdateView) {
        self.st().update = u;
        self.publish();
    }

    pub fn update_view(&self) -> UpdateView {
        self.st().update.clone()
    }

    // MARK: prompts (window side)

    pub fn answer_prompt(&self, id: u64, response: PromptResponse) {
        let pending = {
            let mut st = self.st();
            let Some(idx) = st.prompts.iter().position(|p| p.view.id == id) else {
                return;
            };
            let p = st.prompts.remove(idx);
            (p, st.prompts.is_empty())
        };
        let _ = pending.0.reply.send(response);
        if let Some(f) = self.on_prompt.get() {
            f(!pending.1);
        }
        self.publish();
    }

    // MARK: prompts (pty side, background thread)

    fn ask_user(
        &self,
        host: &str,
        kind: &str,
        prompt: &str,
        allow_save: bool,
        run_id: u32,
        about: Option<&str>,
    ) -> PromptResponse {
        let (tx, rx) = mpsc::channel();
        {
            let mut st = self.st();
            st.next_prompt_id += 1;
            let view = PromptView {
                id: st.next_prompt_id,
                host: host.to_string(),
                kind: kind.to_string(),
                prompt: prompt.to_string(),
                allow_save,
                about: about.map(String::from),
            };
            st.prompts.push(PendingPrompt {
                view,
                run_id,
                reply: tx,
            });
        }
        if let Some(f) = self.on_prompt.get() {
            f(true);
        }
        self.publish();
        rx.recv().unwrap_or_default()
    }

    /// How often this process was already given this secret (and record
    /// this one). An action's questions count by their wording: ssh's
    /// login and the host's sudo both ask for a password, on the same
    /// terminal, and the second is not the first again.
    fn repeat_count(&self, req: &PromptRequest, host: &str, kind: SecretKind) -> u32 {
        let mut st = self.st();
        let now = Instant::now();
        st.recent_answers
            .retain(|_, (t, _)| now.duration_since(*t).as_secs() < 600);
        let question = match req.origin {
            Origin::Action => prompt::last_line(&req.prompt),
            _ => "",
        };
        let k = format!("{}|{host}|{}|{question}", req.run_id, kind.raw());
        let count = st.recent_answers.get(&k).map(|v| v.1).unwrap_or(0);
        st.recent_answers.insert(k, (now, count + 1));
        count
    }

    /// Which host's keychain entry a prompt belongs to. A ProxyJump hop
    /// asks under the final destination's name, so match the user@host ssh
    /// printed in the prompt against every resolved host first.
    fn credential_alias(&self, req: &PromptRequest, secret: SecretKind) -> String {
        let st = self.st();
        let aliases: Vec<String> = st.hosts.iter().map(|h| h.alias.clone()).collect();
        let has = |a: &str| {
            st.secret_presence
                .get(a)
                .map(|v| v.contains(&secret))
                .unwrap_or(false)
        };
        if req.kind == Kind::Passphrase {
            if let Some(key) = prompt::key_path(&req.prompt) {
                let mut candidates = vec![req.host.clone()];
                candidates.extend(aliases.iter().cloned());
                for a in candidates {
                    if has(&a)
                        && (a == req.host
                            || st
                                .resolved
                                .get(&a)
                                .map(|r| r.identity_files_expanded().contains(&key))
                                .unwrap_or(false))
                    {
                        return a;
                    }
                }
            }
            return req.host.clone();
        }
        let Some((user, host)) = prompt::user_host(&req.prompt) else {
            return req.host.clone();
        };
        let matches = |a: &str| {
            st.resolved
                .get(a)
                .map(|r| r.user == user && r.hostname.to_lowercase() == host.to_lowercase())
                .unwrap_or(false)
        };
        // Prefer the requesting host if it matches and has the secret, then
        // any other host with the same user@hostname that has it, then any match.
        if matches(&req.host) && has(&req.host) {
            return req.host.clone();
        }
        if let Some(a) = aliases.iter().find(|a| matches(a) && has(a)) {
            return a.clone();
        }
        if matches(&req.host) {
            return req.host.clone();
        }
        if let Some(a) = aliases.iter().find(|a| matches(a)) {
            return a.clone();
        }
        req.host.clone()
    }
}

impl Model {
    /// Nobody answers: a master's attempt fails and its switch goes off;
    /// a shell, a session or an action just fails on its own.
    fn give_up(&self, req: &PromptRequest, reason: &str) {
        if req.origin == Origin::Master {
            self.daemon().give_up(&req.host, reason);
        }
    }

    /// The answer to a question, from the vault or the user: what `ask`
    /// does, the prompt window saying `about` when it is not ssh's own
    /// login that asks.
    fn answer(&self, req: &PromptRequest, about: Option<&str>) -> Option<String> {
        let host = &req.host;
        let verbatim = req.prompt.trim().replace('\n', " ⏎ ");
        log::prompt(format!(
            "{host}: ssh asks \"{verbatim}\"  [classified as {}]",
            req.kind.name()
        ));
        match req.kind {
            Kind::Password | Kind::Passphrase | Kind::Totp => {
                let sk = match req.kind {
                    Kind::Password => SecretKind::Password,
                    Kind::Passphrase => SecretKind::Passphrase,
                    _ => SecretKind::Totp,
                };
                let alias = self.credential_alias(req, sk);
                if let Some(refusal) = keychain::refusal() {
                    log::prompt(format!("{host}: → the keychain will not open the vault ({refusal}); asking instead of answering from it"));
                }
                if let Some(secret) = keychain::get(&alias, sk) {
                    // The same question again from the same process means
                    // the server rejected what we sent. A TOTP code may
                    // legitimately be asked once more (already-used code);
                    // anything else is a bad secret: an attempt that would
                    // offer it again is given up, except an action's,
                    // whose sudo is asked for the user's answer instead
                    // (a sudo password need not be the login's).
                    let repeats = self.repeat_count(req, &alias, sk);
                    let allowed = if sk == SecretKind::Totp { 1 } else { 0 };
                    if repeats > allowed && req.origin == Origin::Action {
                        log::prompt(format!("{host}: → the host asked for the {} again; the stored {} for {alias} was not taken, asking the user", sk.raw(), sk.raw()));
                    } else if repeats > allowed {
                        log::prompt(format!("{host}: → ssh asked for the {} again; the stored {} for {alias} was rejected by the server, not sending it again", sk.raw(), sk.raw()));
                        self.give_up(
                            req,
                            &format!("stored {} for {alias} rejected by the server", sk.raw()),
                        );
                        return None;
                    } else if sk == SecretKind::Totp {
                        match crate::core::totp::fresh(&alias, &secret) {
                            Ok(code) => {
                                let left = Totp::parse(&secret)
                                    .map(|t| t.seconds_remaining())
                                    .unwrap_or(0);
                                log::prompt(format!("{host}: → answered with a TOTP code generated from the {alias} secret ({left}s left in this window)"));
                                return Some(code);
                            }
                            Err(e) => log::error(format!("{alias}: bad TOTP secret: {e}")),
                        }
                    } else {
                        log::prompt(format!(
                            "{host}: → answered with the {} stored for {alias}",
                            sk.raw()
                        ));
                        return Some(secret);
                    }
                }
                if !keychain::has(&alias, sk) {
                    log::prompt(format!(
                        "{host}: → no {} stored for {alias}; asking the user",
                        sk.raw()
                    ));
                }
                let resp = self.ask_user(
                    &alias,
                    sk.raw(),
                    &req.prompt,
                    sk != SecretKind::Totp,
                    req.run_id,
                    about,
                );
                let Some(answer) = resp.answer else {
                    log::prompt(format!("{host}: → user cancelled; this attempt fails"));
                    self.give_up(
                        req,
                        &format!("{} prompt cancelled", sk.title().to_lowercase()),
                    );
                    return None;
                };
                if resp.save {
                    match keychain::set(&alias, sk, &answer) {
                        Ok(()) => {
                            log::prompt(format!(
                                "{host}: → user answered, saved to the keychain as {} for {alias}",
                                sk.raw()
                            ));
                            self.refresh_secrets();
                        }
                        Err(e) => log::error(format!("{alias}: could not save {}: {e}", sk.raw())),
                    }
                } else {
                    log::prompt(format!("{host}: → user answered, not saved"));
                }
                Some(answer)
            }
            Kind::Confirm => {
                log::prompt(format!("{host}: → host key confirmation; asking the user"));
                let resp = self.ask_user(host, "confirm", &req.prompt, false, req.run_id, about);
                let yes = resp.answer.as_deref() == Some("yes");
                log::prompt(format!(
                    "{host}: → user answered {}",
                    if yes { "yes" } else { "no" }
                ));
                if !yes {
                    self.give_up(req, "host key not accepted");
                }
                Some(if yes { "yes" } else { "no" }.into())
            }
            Kind::Unknown => {
                log::prompt(format!("{host}: → unrecognised prompt; asking the user"));
                let resp = self.ask_user(host, "other", &req.prompt, false, req.run_id, about);
                let Some(answer) = resp.answer else {
                    log::prompt(format!("{host}: → user cancelled; this attempt fails"));
                    self.give_up(req, "prompt cancelled");
                    return None;
                };
                log::prompt(format!("{host}: → user answered"));
                Some(answer)
            }
        }
    }

    /// The process that asked is gone: a question of its still open is
    /// dropped from the window.
    fn drop_questions_of(&self, host: &str, run_id: u32) {
        // A question still open for a master that died is moot.
        let dropped: Vec<PendingPrompt> = {
            let mut st = self.st();
            let (gone, keep): (Vec<_>, Vec<_>) =
                st.prompts.drain(..).partition(|p| p.run_id == run_id);
            st.prompts = keep;
            gone
        };
        if dropped.is_empty() {
            return;
        }
        log::prompt(format!(
            "{host}: the master is gone, {} open question(s) dropped",
            dropped.len()
        ));
        let any_left = !self.st().prompts.is_empty();
        if let Some(f) = self.on_prompt.get() {
            f(any_left);
        }
        self.publish();
    }
}

impl PromptHandler for Model {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        self.answer(req, None)
    }

    fn master_gone(&self, host: &str, run_id: u32) {
        self.drop_questions_of(host, run_id);
    }
}

/// Answers for an action of the Console run on a terminal of its own
/// (`Origin::Action`, `ssh::run_on_terminal`): the model's answers, the
/// prompt window saying what the action is (`about`: `mount /dev/sdb1`,
/// `kill -TERM 1234`), and a cancelled question ending the process, which
/// would otherwise wait on it until sudo's own timeout.
pub struct ActionPrompts {
    model: Arc<Model>,
    about: String,
    cancelled: std::sync::atomic::AtomicBool,
}

impl ActionPrompts {
    pub fn new(model: Arc<Model>, about: String) -> Arc<ActionPrompts> {
        Arc::new(ActionPrompts {
            model,
            about,
            cancelled: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Whether the user cancelled a question of it, which ended it.
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl PromptHandler for ActionPrompts {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        let answer = self.model.answer(req, Some(&self.about));
        if answer.is_none() {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::SeqCst);
            unsafe { libc::kill(req.run_id as i32, libc::SIGTERM) };
        }
        answer
    }

    fn master_gone(&self, host: &str, run_id: u32) {
        self.model.drop_questions_of(host, run_id);
    }
}

/// The tail of shellder.log, for the log panel.
pub fn log_tail() -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(&config::get().log_file) else {
        return vec![];
    };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let tail: u64 = 96 * 1024;
    if size > tail {
        let _ = f.seek(SeekFrom::Start(size - tail));
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    if size > tail && !lines.is_empty() {
        lines.remove(0); // partial first line
    }
    if lines.len() > 600 {
        lines.drain(..lines.len() - 600);
    }
    lines
}
