//! Everything the windows show, and the answers to what ssh asks. One
//! shared object: the daemon, the pty readers and the commands all talk to
//! it, and every change is pushed to the windows as one snapshot.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

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
    pub terminal: TerminalView,
    /// The key that brings a Console up from anywhere, and its host.
    pub quick_console_key: Option<String>,
    pub quick_console_host: Option<String>,
    /// How a locked host is brought back after a drop: the rule every
    /// host follows, and the hosts that have one of their own, each
    /// spelled out whole (`core::reconnect`).
    pub reconnect: reconnect::Rule,
    pub host_reconnect: BTreeMap<String, reconnect::Rule>,
    /// The height the main window's log panel was last dragged to.
    pub log_panel_height: Option<f64>,
    /// Why the quick terminal's key could not be registered (the system
    /// or another application has it), when it could not.
    pub quick_key_error: Option<String>,
}

/// The Console's terminals as Settings has them, every default spelled
/// out (`prefs::Terminal` keeps only what was chosen).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalView {
    pub font: String,
    pub line_height: f64,
    pub cursor: String,
    pub cursor_blink: bool,
    pub scrollback: u32,
    pub theme_light: String,
    pub theme_dark: String,
    pub opacity: f64,
    pub blur_strength: u32,
    pub padding: f64,
    pub copy_on_select: bool,
    pub paste_protection: bool,
    pub confirm_close: bool,
    pub notifications: bool,
    pub shell_integration: bool,
    pub webgl: bool,
}

impl TerminalView {
    pub fn of(p: &prefs::Prefs) -> TerminalView {
        let t = &p.terminal;
        TerminalView {
            font: t.font.clone().unwrap_or_default(),
            line_height: t.line_height.unwrap_or(1.15).clamp(1.0, 2.0),
            cursor: match t.cursor.as_deref() {
                Some(c @ ("bar" | "underline")) => c.to_string(),
                _ => "block".to_string(),
            },
            cursor_blink: t.cursor_blink.unwrap_or(true),
            scrollback: t.scrollback.unwrap_or(10_000).clamp(100, 1_000_000),
            theme_light: t.theme_light.clone().unwrap_or_else(|| "default".into()),
            theme_dark: t.theme_dark.clone().unwrap_or_else(|| "default".into()),
            opacity: p.terminal_opacity(),
            blur_strength: p.terminal_blur(),
            padding: t.padding.unwrap_or(2.0).clamp(0.0, 40.0),
            copy_on_select: t.copy_on_select.unwrap_or(false),
            paste_protection: t.paste_protection.unwrap_or(true),
            confirm_close: t.confirm_close.unwrap_or(true),
            notifications: t.notifications.unwrap_or(true),
            shell_integration: p.shell_integration(),
            webgl: t.webgl.unwrap_or(false),
        }
    }
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
    /// The config changed, or a reload was asked for, while one ran: it
    /// goes round once more.
    reload_again: bool,
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
    quick_key_error: Option<String>,
    prefs_seen: Option<SystemTime>,
    vault_stamp_seen: Option<SystemTime>,
}

/// What a setting reaches besides the pages, which follow the state:
/// `on_settings_changed` is given this and runs that applier alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// The pages alone (the language, a theme, the log panel's height…).
    Pages,
    /// The menu bar item, there or not.
    MenuBar,
    /// The quick terminal's key, registered with the system.
    QuickTerminal,
    /// The blur behind the terminal windows.
    TerminalEffects,
}

impl Applies {
    pub fn of(key: &str) -> Applies {
        match key {
            "keepInBackground" | "showMenuBarIcon" => Applies::MenuBar,
            "quickConsoleKey" | "quickConsoleHost" => Applies::QuickTerminal,
            "terminal.opacity" | "terminal.blurStrength" => Applies::TerminalEffects,
            _ => Applies::Pages,
        }
    }
}

/// Many marks, one round of work at a time: `wait` returns once a mark
/// came since it last returned, however many came. The work done after
/// it sees every change marked before; one marked while the work runs
/// brings the next round.
#[derive(Default)]
struct Coalescer {
    marked: Mutex<bool>,
    wake: Condvar,
}

impl Coalescer {
    fn mark(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.wake.notify_one();
    }

    fn wait(&self) {
        let mut marked = self.marked.lock().unwrap_or_else(|e| e.into_inner());
        while !*marked {
            marked = self.wake.wait(marked).unwrap_or_else(|e| e.into_inner());
        }
        *marked = false;
    }
}

/// The shortest time between two states sent to the windows.
const PUBLISH_INTERVAL: Duration = Duration::from_millis(40);

/// The settings as last read, and when the two files they come from had
/// last changed then (`settings_stamp`).
struct SettingsCache {
    stamp: (Option<SystemTime>, Option<SystemTime>),
    view: SettingsView,
}

/// When prefs.json and the login item last changed (None: not there).
fn settings_stamp() -> (Option<SystemTime>, Option<SystemTime>) {
    let login_item = std::fs::metadata(autostart::entry_file())
        .ok()
        .and_then(|m| m.modified().ok());
    (prefs::modified(), login_item)
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
    pub on_settings_changed: OnceLock<Box<dyn Fn(Applies) + Send + Sync>>,
    /// Given every state sent to the windows, the same value: the menu
    /// bar item and the Files windows follow it.
    pub on_published: OnceLock<Box<dyn Fn(Arc<Snapshot>) + Send + Sync>>,
    /// The rounds of `publish`.
    publishing: Coalescer,
    /// The settings kept from one snapshot to the next (`settings_view`).
    settings: Mutex<Option<SettingsCache>>,
    /// Held while the switched-on and the locked hosts change (`apply`).
    applying: Mutex<()>,
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
            on_published: OnceLock::new(),
            publishing: Coalescer::default(),
            settings: Mutex::new(None),
            applying: Mutex::new(()),
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

        // The windows' state: built once a round, however many changes
        // asked for it, and handed to the menu bar item and the Files
        // windows as it went to the pages.
        let me = self.clone();
        std::thread::Builder::new()
            .name("shellder-publish".into())
            .spawn(move || loop {
                me.publishing.wait();
                let snap = Arc::new(me.snapshot());
                if let Some(app) = me.app.get() {
                    let _ = app.emit("state", &*snap);
                }
                if let Some(f) = me.on_published.get() {
                    f(snap);
                }
                std::thread::sleep(PUBLISH_INTERVAL);
            })
            .expect("publisher");

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

        // Every few seconds: the ssh config, the prefs and the login item
        // the CLI may have edited, and the vault another process may have
        // written.
        let me = self.clone();
        std::thread::Builder::new()
            .name("shellder-poll".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                me.reload_catalog(false);
                me.sync_locked_from_prefs();
                me.notice_settings_writes();
                me.notice_vault_writes();
            })
            .expect("poll");
    }

    /// Stop everything the app started, and wait for it to be gone:
    /// macOS 26 holds an app as still running while a process of its is
    /// alive after it exited.
    pub fn shutdown(&self) {
        // The waiting copies too: the one cancelled would start the next.
        for f in super::files::all() {
            f.cancel_all();
        }
        super::terminals::close_all();
        session::close_all();
        if let Some(d) = self.daemon.get() {
            d.shutdown();
        }
    }

    // MARK: snapshot

    /// The state as the windows draw it. The settings come from
    /// `settings_view`, read before the model's lock is taken: under it
    /// is only what the model holds itself.
    pub fn snapshot(&self) -> Snapshot {
        let mut settings = self.settings_view();
        let c = config::get();
        let st = self.st();
        settings.error = st.settings_error.clone();
        settings.quick_key_error = st.quick_key_error.clone();
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
            settings,
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

    /// The settings as the pages show them, but for the two the model
    /// holds (`error`, `quick_key_error`, filled in by `snapshot`).
    /// Reading them parses prefs.json and looks at the login item, so
    /// they are kept from one snapshot to the next, and read again once
    /// either file changed (a setting set here, a CLI's edit) or a
    /// change made here dropped them (`forget_settings`).
    fn settings_view(&self) -> SettingsView {
        let stamp = settings_stamp();
        if let Some(c) = self.settings_cache().as_ref() {
            if c.stamp == stamp {
                return c.view.clone();
            }
        }
        let view = self.read_settings();
        *self.settings_cache() = Some(SettingsCache {
            stamp,
            view: view.clone(),
        });
        view
    }

    fn settings_cache(&self) -> std::sync::MutexGuard<'_, Option<SettingsCache>> {
        self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn forget_settings(&self) {
        *self.settings_cache() = None;
    }

    fn read_settings(&self) -> SettingsView {
        let p = prefs::load();
        let start_at_login = autostart::installed();
        let autostart_other_copy = if start_at_login {
            autostart::installed_path()
                .filter(|p| std::path::Path::new(p) != config::get().self_path)
        } else {
            None
        };
        SettingsView {
            keep_in_background: p.keep_in_background,
            show_menu_bar_icon: p.show_menu_bar_icon(),
            invert_menu_bar_icon: p.invert_menu_bar_icon,
            silent_launch: p.silent_launch,
            language: p.language(),
            launched_language: self.launched_language.clone(),
            start_at_login,
            auto_update: p.auto_update(),
            autostart_other_copy,
            error: None,
            platform: std::env::consts::OS,
            terminal: TerminalView::of(&p),
            quick_console_key: p.quick_console_key.clone(),
            quick_console_host: p.quick_console_host.clone(),
            reconnect: reconnect::default_rule(&p),
            host_reconnect: reconnect::host_rules(&p),
            log_panel_height: p.log_panel_height,
            quick_key_error: None,
        }
    }

    /// Push the state to every window: soon, not now. Every change calls
    /// this, a config reload four times in a row; the publisher thread
    /// builds the state once for all that came since its last round, at
    /// most every `PUBLISH_INTERVAL`, and a change made during a round
    /// brings another, so the last state always lands.
    pub fn publish(&self) {
        self.publishing.mark();
    }

    /// The catalog's hosts, in config order.
    pub fn host_aliases(&self) -> Vec<String> {
        self.st().hosts.iter().map(|h| h.alias.clone()).collect()
    }

    /// The host asking the question at the head of the queue when it is
    /// the host's own (an action of the Console's), not ssh's login.
    pub fn action_prompt_host(&self) -> Option<String> {
        self.st()
            .prompts
            .first()
            .filter(|p| p.view.about.is_some())
            .map(|p| p.view.host.clone())
    }

    /// Why the quick terminal's key could not be registered, or None.
    pub fn set_quick_key_error(&self, error: Option<String>) {
        {
            let mut st = self.st();
            if st.quick_key_error == error {
                return;
            }
            st.quick_key_error = error;
        }
        self.publish();
    }

    // MARK: catalog

    /// How many `ssh -G` run at once while the hosts are resolved.
    const RESOLVE_PARALLEL: usize = 8;

    /// The ssh config and the files it includes, as the last reload found
    /// them.
    fn watched_files(st: &State) -> Vec<String> {
        let mut files = st.config_files.clone();
        let main = config::path_str(&config::get().ssh_config_file);
        if !files.contains(&main) {
            files.insert(0, main);
        }
        files
    }

    /// Re-read ~/.ssh/config (and its includes) when any of them changed.
    /// A change seen, or a reload asked for, while a reload runs is not
    /// dropped: that reload goes round once more.
    pub fn reload_catalog(self: &Arc<Self>, force: bool) {
        let files = Self::watched_files(&self.st());
        let now = catalog::modification_dates(&files);
        {
            let mut st = self.st();
            if !force && now == st.mtimes && (!st.hosts.is_empty() || st.reloaded_once) {
                return;
            }
            if st.reloading {
                st.reload_again = true;
                return;
            }
            st.reloading = true;
        }
        self.publish();
        let me = self.clone();
        std::thread::spawn(move || loop {
            // The dates as the files are read, not as they are once the
            // hosts are resolved: an edit made meanwhile is a change the
            // next look sees. Looks made during the reload compare
            // against these, and what they find brings another round.
            let files = Self::watched_files(&me.st());
            let mut mtimes = catalog::modification_dates(&files);
            {
                let mut st = me.st();
                st.mtimes = mtimes.clone();
                st.reload_again = false;
            }
            let cat = catalog::load();
            // A file the config includes from now on is dated as it was
            // just read; one it no longer includes is not watched.
            let new: Vec<String> = cat
                .files
                .iter()
                .filter(|f| !mtimes.contains_key(*f))
                .cloned()
                .collect();
            mtimes.extend(catalog::modification_dates(&new));
            mtimes.retain(|f, _| cat.files.contains(f));
            // The list first, the resolutions when they are in: the window
            // shows its hosts (with "resolving…" under each new one) while
            // ssh -G runs for them, a few at a time.
            {
                let mut st = me.st();
                st.hosts = cat.entries.clone();
                st.config_files = cat.files.clone();
                st.mtimes = mtimes;
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
            let again = {
                let mut st = me.st();
                st.resolved = res;
                st.resolve_errors = errs;
                st.reloaded_once = true;
                st.reloading = st.reload_again;
                st.reload_again
            };
            me.refresh_secrets();
            me.apply(|_, _, _| {});
            log::info(format!(
                "ssh config loaded: {}",
                cat.entries
                    .iter()
                    .map(|e| e.alias.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if !again {
                break;
            }
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
        if now == self.st().locked {
            return;
        }
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
        self.apply(|st, e, l| {
            let locked = l.clone();
            for h in now.difference(&locked) {
                e.insert(h.clone());
                l.insert(h.clone());
            }
            for h in locked.difference(&now) {
                l.remove(h);
                for d in Self::dependents_in(st, h) {
                    l.remove(&d);
                }
            }
        });
    }

    /// The settings page follows the prefs and the login item another
    /// process changed (`shellder install`, an edit of prefs.json).
    fn notice_settings_writes(&self) {
        let stamp = settings_stamp();
        let stale = self
            .settings_cache()
            .as_ref()
            .is_some_and(|c| c.stamp != stamp);
        if stale {
            self.publish();
        }
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

    /// Change the switched-on and the locked hosts: `change` gets the
    /// state and the two sets as they are and edits the sets, then the two
    /// rules every path shares apply: a locked host is switched on, and
    /// both take their jump hosts along. Stores the locks, logs the
    /// changes and pushes the host list to the daemon. The push happens
    /// even when the sets did not change: at launch and after a config
    /// reload the daemon needs the (re)resolved hosts anyway.
    ///
    /// The change is made under the model's lock, one change at a time
    /// (`applying`, held until the daemon has the list): two callers at
    /// once each build on the other's sets, where copies taken before and
    /// stored after reverted each other.
    fn apply(&self, change: impl FnOnce(&State, &mut HashSet<String>, &mut HashSet<String>)) {
        let _one = self.applying.lock().unwrap_or_else(|e| e.into_inner());
        self.apply_held(change);
    }

    /// `apply`, for a caller that holds `applying` already.
    fn apply_held(&self, change: impl FnOnce(&State, &mut HashSet<String>, &mut HashSet<String>)) {
        let (specs, locked) = {
            let mut st = self.st();
            let (mut e, mut l) = (st.enabled.clone(), st.locked.clone());
            change(&st, &mut e, &mut l);
            let with_jumps = |set: &HashSet<String>| {
                let mut out = set.clone();
                for h in set {
                    out.extend(Self::jump_chain_in(&st, h));
                }
                out
            };
            let new_locked = with_jumps(&l);
            let new_enabled: HashSet<String> = with_jumps(&e).union(&new_locked).cloned().collect();
            let on: Vec<_> = new_enabled.difference(&st.enabled).cloned().collect();
            let off: Vec<_> = st.enabled.difference(&new_enabled).cloned().collect();
            let lock: Vec<_> = new_locked.difference(&st.locked).cloned().collect();
            let unlock: Vec<_> = st.locked.difference(&new_locked).cloned().collect();
            st.enabled = new_enabled;
            st.locked = new_locked;
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
            let specs = st
                .hosts
                .iter()
                .map(|h| HostSpec {
                    alias: h.alias.clone(),
                    enabled: st.enabled.contains(&h.alias),
                    locked: st.locked.contains(&h.alias),
                    resolved: st.resolved.get(&h.alias).cloned(),
                    resolve_error: st.resolve_errors.get(&h.alias).cloned(),
                })
                .collect::<Vec<_>>();
            (specs, st.locked.clone())
        };
        // The locks are remembered in the prefs, written in one go and
        // outside the model's lock: the order the file has is kept, the
        // hosts newly locked follow.
        let stored: HashSet<String> = prefs::locked_hosts().into_iter().collect();
        if stored != locked {
            let _ = prefs::update(|p| {
                p.locked_hosts.retain(|h| locked.contains(h));
                let mut added: Vec<&String> = locked
                    .iter()
                    .filter(|h| !p.locked_hosts.contains(h))
                    .collect();
                added.sort();
                p.locked_hosts.extend(added.into_iter().cloned());
            });
            self.forget_settings();
        }
        self.st().prefs_seen = prefs::modified();
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
        self.apply(|st, e, l| {
            if on {
                e.insert(host.to_string());
            } else {
                let mut gone = Self::dependents_in(st, host);
                gone.push(host.to_string());
                for g in gone {
                    e.remove(&g);
                    l.remove(&g);
                }
            }
        });
    }

    /// The lock. On: switch the host on if it is off, and lock it and its
    /// jump hosts. Off: unlock it and the hosts that jump through it (their
    /// lock would lock this one again). The connection stays as it is.
    pub fn set_locked(&self, host: &str, on: bool) {
        self.apply(|st, e, l| {
            if on {
                e.insert(host.to_string());
                l.insert(host.to_string());
            } else {
                l.remove(host);
                for d in Self::dependents_in(st, host) {
                    l.remove(&d);
                }
            }
        });
    }

    /// The daemon flipped a switch itself (failed attempt, drop of an
    /// unlocked host, Close socket, Reconnect on an off host): mirror it in
    /// the preferences and the UI. The whole snapshot is taken over, not
    /// just one host: a jump host that failed turns its dependents off in
    /// the same breath. It is asked for with `applying` held, so that no
    /// switch flipped meanwhile is missing from it: every list the model
    /// sent the daemon before is in what it answers.
    fn switch_changed_by_daemon(&self) {
        let _one = self.applying.lock().unwrap_or_else(|e| e.into_inner());
        let snap = self.daemon().snapshot();
        self.apply_held(|_, e, l| {
            *e = snap
                .iter()
                .filter(|s| s.enabled)
                .map(|s| s.host.clone())
                .collect();
            *l = snap
                .iter()
                .filter(|s| s.locked)
                .map(|s| s.host.clone())
                .collect();
        });
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
        // One look at the state, then the word: a guard taken in a match's
        // scrutinee lives through its arms, and an arm that locked again
        // deadlocked this thread on its own lock once the vault opened,
        // with every window's call behind it.
        let (refusal, readable) = {
            let st = self.st();
            let n: usize = st.secret_presence.values().map(|v| v.len()).sum();
            (st.keychain_refusal.clone(), n)
        };
        match refusal {
            Some(r) => log::warn(format!("keychain: still refused: {r}")),
            None => log::info(format!(
                "keychain: the vault opened; {readable} secrets are readable"
            )),
        }
    }

    /// Which secrets each host has, from one read of the vault made
    /// before the model's lock is taken: the read may wait on the
    /// keychain (its dialog, even), which must not hold up every window.
    pub fn refresh_secrets(&self) {
        let vault = match keychain::read(false) {
            VaultRead::Ok(v) => v,
            VaultRead::Refused(e) => {
                // Leave what was known there: the secrets have not gone
                // anywhere, this run just cannot see them.
                self.st().keychain_refusal = Some(e);
                self.publish();
                return;
            }
            _ => keychain::Vault::new(),
        };
        let stored: HashMap<&str, Vec<SecretKind>> = vault
            .iter()
            .map(|(host, kinds)| {
                let kinds = SecretKind::ALL
                    .into_iter()
                    .filter(|k| kinds.contains_key(k.raw()))
                    .collect();
                (host.as_str(), kinds)
            })
            .collect();
        {
            let mut st = self.st();
            st.keychain_refusal = None;
            st.secret_presence = st
                .hosts
                .iter()
                .map(|h| {
                    let kinds = stored.get(h.alias.as_str()).cloned().unwrap_or_default();
                    (h.alias.clone(), kinds)
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
            // The quick Console: a null key switches it off.
            "quickConsoleKey" => {
                p.quick_console_key = value.as_str().filter(|s| !s.is_empty()).map(String::from)
            }
            "quickConsoleHost" => {
                p.quick_console_host = value.as_str().filter(|s| !s.is_empty()).map(String::from)
            }
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
            // The terminals' settings ("terminal.font" …): a null puts the
            // default back.
            k if k.starts_with("terminal.") => {
                let t = &mut p.terminal;
                let text = || value.as_str().filter(|s| !s.is_empty()).map(String::from);
                match &k["terminal.".len()..] {
                    "font" => t.font = text(),
                    "lineHeight" => t.line_height = value.as_f64(),
                    "cursor" => t.cursor = text(),
                    "cursorBlink" => t.cursor_blink = value.as_bool(),
                    "scrollback" => t.scrollback = value.as_u64().map(|n| n as u32),
                    "themeLight" => t.theme_light = text(),
                    "themeDark" => t.theme_dark = text(),
                    "opacity" => t.opacity = value.as_f64(),
                    "blurStrength" => t.blur_strength = value.as_u64().map(|n| n as u32),
                    "padding" => t.padding = value.as_f64(),
                    "copyOnSelect" => t.copy_on_select = value.as_bool(),
                    "pasteProtection" => t.paste_protection = value.as_bool(),
                    "confirmClose" => t.confirm_close = value.as_bool(),
                    "notifications" => t.notifications = value.as_bool(),
                    "shellIntegration" => t.shell_integration = value.as_bool(),
                    "webgl" => t.webgl = value.as_bool(),
                    _ => error = Some(format!("unknown setting {key}")),
                }
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
        {
            let mut st = self.st();
            st.settings_error = error.clone();
            st.prefs_seen = prefs::modified();
        }
        self.forget_settings();
        // Beyond the pages, only what this setting reaches: a drag of the
        // opacity slider sets it several times a second, and the menu bar
        // or the quick terminal's key have nothing to do with it.
        let applies = Applies::of(key);
        if applies != Applies::Pages {
            if let Some(f) = self.on_settings_changed.get() {
                f(applies);
            }
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
        self.forget_settings();
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

/// What the log panel is given: lines of shellder.log, and where the next
/// call goes on from.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogTail {
    /// Just past the last complete line read: the `after` of the next call.
    pub offset: u64,
    pub lines: Vec<String>,
    /// The lines are the log's last, in place of what the panel shows:
    /// the first call, or a log started afresh (a GUI launch truncates it).
    pub reset: bool,
}

/// How much of the log's end is read at most, and how many of its lines
/// are kept: more than that is more than the panel shows.
const LOG_TAIL_BYTES: u64 = 96 * 1024;
const LOG_TAIL_LINES: usize = 600;

/// The log panel's lines: with `after` (the `offset` the last call gave),
/// the complete lines written since; without, or when the log is shorter
/// than that now, its last lines. Nothing written since costs one look
/// at the file's size.
pub fn log_tail(after: Option<u64>) -> LogTail {
    tail_of(&config::get().log_file, after)
}

fn tail_of(path: &std::path::Path, after: Option<u64>) -> LogTail {
    use std::io::{Read, Seek, SeekFrom};
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let (from, reset) = match after {
        Some(a) if a <= size => (a, false),
        _ => (0, true),
    };
    if !reset && from == size {
        return LogTail {
            offset: from,
            lines: vec![],
            reset,
        };
    }
    // A read that starts further on than `from` starts inside a line: the
    // piece before its first newline is left out. One byte more is read,
    // so a start just after a newline loses nothing.
    let start = from.max(size.saturating_sub(LOG_TAIL_BYTES));
    let cut = start > from;
    let read_from = if cut { start - 1 } else { start };
    let mut buf = Vec::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        if f.seek(SeekFrom::Start(read_from)).is_ok() {
            let _ = f.take(size - read_from).read_to_end(&mut buf);
        }
    }
    let begin = if cut {
        buf.iter()
            .position(|&b| b == b'\n')
            .map_or(buf.len(), |i| i + 1)
    } else {
        0
    };
    // Complete lines only: one still being written is read whole next time.
    let end = buf[begin..]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(begin, |i| begin + i + 1);
    let mut lines: Vec<String> = String::from_utf8_lossy(&buf[begin..end])
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    if lines.len() > LOG_TAIL_LINES {
        lines.drain(..lines.len() - LOG_TAIL_LINES);
    }
    LogTail {
        offset: read_from + end as u64,
        lines,
        reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("shellder-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn the_log_is_read_on_from_where_the_last_call_stopped() {
        let p = scratch("follow.log");
        let _ = std::fs::remove_file(&p);
        let t = tail_of(&p, None);
        assert_eq!((t.offset, t.lines.len(), t.reset), (0, 0, true));

        append(&p, "a\nb\n");
        let t = tail_of(&p, None);
        assert_eq!(t.lines, ["a", "b"]);
        assert_eq!((t.offset, t.reset), (4, true));

        // A line still being written waits for its newline.
        append(&p, "c\nd");
        let t = tail_of(&p, Some(4));
        assert_eq!(t.lines, ["c"]);
        assert_eq!((t.offset, t.reset), (6, false));
        let t = tail_of(&p, Some(6));
        assert_eq!((t.offset, t.lines.len(), t.reset), (6, 0, false));
        append(&p, "\n");
        let t = tail_of(&p, Some(6));
        assert_eq!(t.lines, ["d"]);
        assert_eq!(t.offset, 8);

        // Nothing new: the same offset back.
        let t = tail_of(&p, Some(8));
        assert_eq!((t.offset, t.lines.len(), t.reset), (8, 0, false));

        // A log started afresh, shorter than the offset: its last lines.
        std::fs::write(&p, "x\n").unwrap();
        let t = tail_of(&p, Some(8));
        assert_eq!(t.lines, ["x"]);
        assert_eq!((t.offset, t.reset), (2, true));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_long_log_gives_its_last_lines_and_never_a_piece_of_one() {
        let p = scratch("long.log");
        let text: String = (1..=20_000).map(|i| format!("line {i:05}\n")).collect();
        std::fs::write(&p, &text).unwrap();
        let size = text.len() as u64;
        for after in [None, Some(11)] {
            let t = tail_of(&p, after);
            assert_eq!(t.reset, after.is_none());
            assert_eq!(t.offset, size);
            assert_eq!(t.lines.len(), LOG_TAIL_LINES);
            assert_eq!(t.lines.last().map(String::as_str), Some("line 20000"));
            assert!(t
                .lines
                .iter()
                .all(|l| l.len() == 10 && l.starts_with("line ")));
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_setting_runs_only_its_own_applier() {
        assert_eq!(Applies::of("keepInBackground"), Applies::MenuBar);
        assert_eq!(Applies::of("showMenuBarIcon"), Applies::MenuBar);
        assert_eq!(Applies::of("quickConsoleKey"), Applies::QuickTerminal);
        assert_eq!(Applies::of("quickConsoleHost"), Applies::QuickTerminal);
        assert_eq!(Applies::of("terminal.opacity"), Applies::TerminalEffects);
        assert_eq!(
            Applies::of("terminal.blurStrength"),
            Applies::TerminalEffects
        );
        for key in [
            "logPanelHeight",
            "language",
            "invertMenuBarIcon",
            "terminal.themeDark",
            "terminal.font",
            "reconnect.first",
        ] {
            assert_eq!(Applies::of(key), Applies::Pages, "{key}");
        }
    }

    #[test]
    fn many_marks_are_few_rounds_and_the_last_is_never_missed() {
        let c = Arc::new(Coalescer::default());
        let version = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(AtomicUsize::new(0));
        let rounds = Arc::new(AtomicUsize::new(0));
        {
            let (c, version, seen, rounds) =
                (c.clone(), version.clone(), seen.clone(), rounds.clone());
            std::thread::spawn(move || loop {
                c.wait();
                seen.store(version.load(Ordering::SeqCst), Ordering::SeqCst);
                rounds.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
            });
        }
        let start = Instant::now();
        for _ in 0..500 {
            version.fetch_add(1, Ordering::SeqCst);
            c.mark();
            std::thread::sleep(Duration::from_micros(200));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while seen.load(Ordering::SeqCst) != 500 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(seen.load(Ordering::SeqCst), 500);
        // One round at most per 20 ms of the marks, however many came.
        let n = rounds.load(Ordering::SeqCst);
        let most = start.elapsed().as_millis() as usize / 20 + 1;
        assert!(n <= most && n < 500, "{n} rounds for 500 marks");
    }
}
