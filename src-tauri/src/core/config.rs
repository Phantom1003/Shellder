//! Paths and tunables shared by the app and the CLI. shellder never writes
//! ssh configuration: the only files it owns live in its state directory
//! (lock, prefs, TOTP markers, the log) plus, optionally, a login item.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

pub const APP: &str = "shellder";
pub const LABEL: &str = "local.shellder";
/// GitHub repository whose Releases the updater watches.
pub const RELEASE_REPO: &str = "Phantom1003/Shellder";

// Reconnect back-off (seconds): the built-in defaults of the reconnect
// rule (`reconnect::Rule::DEFAULT`), which the prefs may change.
pub const BACKOFF_MIN: Duration = Duration::from_secs(5);
pub const BACKOFF_MAX: Duration = Duration::from_secs(600);
/// A master that dies this soon after spawning counts as a failed attempt.
pub const SHORT_LIFE: Duration = Duration::from_secs(45);
/// After this many consecutive quick failures jump straight to BACKOFF_MAX
/// (a wrong password hammering the server gets you fail2ban'd).
pub const MAX_QUICK_FAILURES: u32 = 3;
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Re-verify an established master with `ssh -O check` this often.
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(60);
/// Time a fresh master gets to authenticate (TOTP window waits, PAM delays,
/// the user typing into a prompt).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);

pub struct Config {
    pub home: PathBuf,
    pub ssh_dir: PathBuf,
    pub ssh_config_file: PathBuf,
    pub config_overridden: bool,
    pub state_dir: PathBuf,
    /// `SHELLDER_STATE_DIR` was set: an isolated copy, whose window
    /// positions go in the state dir too.
    pub state_dir_overridden: bool,
    pub log_file: PathBuf,
    pub prefs_file: PathBuf,
    pub lock_file: PathBuf,
    /// Touched by every vault write so a running app notices a save made
    /// by another process (the CLI, a second copy).
    pub vault_stamp: PathBuf,
    pub update_api: String,
    /// Path of this very executable.
    pub self_path: PathBuf,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Set in the environment of a process started by `app::restart` (a
/// language change or an installed update on a platform where Tauri's own
/// restart is used): the new process starts while the old one is still on
/// its way out, so this is the one launch that may wait for the instance
/// lock. Any other launch that finds the lock taken exits at once.
pub const RELAUNCH_ENV: &str = "SHELLDER_RELAUNCH";

/// Test hooks: SHELLDER_SSH_CONFIG points shellder at another ssh config
/// (passed to ssh as -F), SHELLDER_STATE_DIR relocates everything it owns,
/// SHELLDER_UPDATE_API points the updater at a local release JSON.
pub fn get() -> &'static Config {
    CONFIG.get_or_init(|| {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let ssh_dir = home.join(".ssh");
        let (ssh_config_file, config_overridden) = match env::var_os("SHELLDER_SSH_CONFIG") {
            Some(p) => (PathBuf::from(p), true),
            None => (ssh_dir.join("config"), false),
        };
        let state_dir_overridden = env::var_os("SHELLDER_STATE_DIR").is_some();
        let state_dir = env::var_os("SHELLDER_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| default_state_dir(&home));
        let self_path = env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| PathBuf::from(env::args().next().unwrap_or_default()));
        Config {
            log_file: state_dir.join(format!("{APP}.log")),
            prefs_file: state_dir.join("prefs.json"),
            lock_file: state_dir.join("app.lock"),
            vault_stamp: state_dir.join("vault.stamp"),
            update_api: env::var("SHELLDER_UPDATE_API").unwrap_or_else(|_| {
                format!("https://github.com/{RELEASE_REPO}/releases/latest/download/latest.json")
            }),
            home,
            ssh_dir,
            ssh_config_file,
            config_overridden,
            state_dir,
            state_dir_overridden,
            self_path,
        }
    })
}

fn default_state_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support").join(APP)
    } else {
        env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"))
            .join(APP)
    }
}

pub fn ensure_dirs() {
    let dir = &get().state_dir;
    let _ = std::fs::create_dir_all(dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
}

pub fn home() -> &'static str {
    get().home.to_str().unwrap_or("/")
}

pub fn expand_tilde(p: &str) -> String {
    if p == "~" {
        return home().to_string();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return format!("{}/{}", home(), rest);
    }
    p.to_string()
}

pub fn abbreviate_home(p: &str) -> String {
    let h = home();
    match p.strip_prefix(h) {
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => p.to_string(),
    }
}

pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
