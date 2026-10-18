//! App preferences (which hosts are locked, UI options): one JSON file in
//! the state directory, never ssh configuration. Written whole, read fresh
//! on every access so the CLI and the app see each other's changes.

use std::collections::BTreeMap;
use std::io::Write;

use serde::{Deserialize, Serialize};

use super::config;
use super::reconnect;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Locked hosts: connected again at launch and reconnected after drops.
    /// The Connect switch itself is not remembered.
    pub locked_hosts: Vec<String>,
    /// Closing the last window leaves the app running (masters kept up,
    /// reachable from the menu bar); off, the default, means it quits.
    pub keep_in_background: bool,
    /// Menu bar item; only meaningful while the app keeps running in the
    /// background.
    pub show_menu_bar_icon: Option<bool>,
    /// Draw the menu bar picture light instead of dark. macOS tints the
    /// picture itself; a Linux panel shows it as it is, and a dark panel
    /// needs the light one.
    pub invert_menu_bar_icon: bool,
    /// Start without a window when the login item starts the app.
    pub silent_launch: bool,
    /// Language of the interface, a BCP 47 name with a translation in the
    /// bundle ("en", "zh-Hans"). English unless chosen otherwise.
    pub language: Option<String>,
    /// Look for a newer release at launch and once a day.
    pub auto_update: Option<bool>,
    /// How the master keeps its connection alive on servers that drop
    /// session-less (-N) connections; remembered per host across runs.
    pub idle_modes: BTreeMap<String, String>,
    /// How a locked host is brought back after a drop (`core::reconnect`):
    /// the rule every host follows, and the hosts with a rule of their
    /// own. Only the fields chosen are kept.
    pub reconnect: reconnect::Partial,
    pub host_reconnect: BTreeMap<String, reconnect::Partial>,
    /// How each side of a Files window sorts its rows ("left"/"right" to
    /// "modified descending").
    pub file_sorts: BTreeMap<String, String>,
    pub log_panel_height: Option<f64>,
    pub transfer_panel_height: Option<f64>,
    /// The Console's terminals: their look and manners (see `Terminal`).
    pub terminal: Terminal,
    /// A key that brings a Console up from anywhere and hides it again
    /// (`quick_console_key`, in the global shortcut plugin's words:
    /// "Super+Shift+KeyK"), and whose host it shows (`quick_console_host`,
    /// this machine without).
    pub quick_console_key: Option<String>,
    pub quick_console_host: Option<String>,
    /// The quick terminal's size as shares of the screen's width and
    /// height, as the user last dragged it (the whole width and a third
    /// of the height without).
    pub quick_width: Option<f64>,
    pub quick_height: Option<f64>,
}

/// How the Console's terminals look and behave. Every field is optional:
/// unset means the default, which `TerminalView` in `app::model` spells
/// out for the pages, so an older prefs file needs no migration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Terminal {
    /// The letters' family; the page's monospace stack without.
    pub font: Option<String>,
    pub line_height: Option<f64>,
    /// "block", "bar" or "underline".
    pub cursor: Option<String>,
    pub cursor_blink: Option<bool>,
    pub scrollback: Option<u32>,
    /// The colours (ui/themes.js names them) for a light and a dark
    /// terminal.
    pub theme_light: Option<String>,
    pub theme_dark: Option<String>,
    /// How see-through the terminal area is, 0.3 to 1 (opaque), and
    /// whether what is behind it is blurred (macOS).
    pub opacity: Option<f64>,
    pub blur: Option<bool>,
    /// The space between the terminal and its edges, in pixels.
    pub padding: Option<f64>,
    pub copy_on_select: Option<bool>,
    /// Ask before pasting several lines into a shell that is not reading
    /// a bracketed paste.
    pub paste_protection: Option<bool>,
    /// Ask before closing a tab in which a command is running.
    pub confirm_close: Option<bool>,
    /// Tell of a long command's end and of a bell in a window that is
    /// not in front.
    pub notifications: Option<bool>,
    /// Start a shell with the integration (`core::shellint`).
    pub shell_integration: Option<bool>,
    /// Draw with WebGL (off: the DOM renderer, whose letters WebKit
    /// draws itself, crisper than a glyph atlas).
    pub webgl: Option<bool>,
}

impl Prefs {
    pub fn language(&self) -> String {
        self.language.clone().unwrap_or_else(|| "en".into())
    }
    pub fn show_menu_bar_icon(&self) -> bool {
        self.show_menu_bar_icon.unwrap_or(true)
    }
    pub fn auto_update(&self) -> bool {
        self.auto_update.unwrap_or(true)
    }
    pub fn shell_integration(&self) -> bool {
        self.terminal.shell_integration.unwrap_or(true)
    }
    /// The terminal area see-through: below 1 the Console window is built
    /// transparent.
    pub fn terminal_opacity(&self) -> f64 {
        self.terminal.opacity.unwrap_or(1.0).clamp(0.3, 1.0)
    }
}

pub fn load() -> Prefs {
    let path = &config::get().prefs_file;
    match std::fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

pub fn save(p: &Prefs) -> std::io::Result<()> {
    config::ensure_dirs();
    let path = &config::get().prefs_file;
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(p).map_err(std::io::Error::other)?;
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Read, change, write.
pub fn update(change: impl FnOnce(&mut Prefs)) -> std::io::Result<()> {
    let mut p = load();
    change(&mut p);
    save(&p)
}

pub fn locked_hosts() -> Vec<String> {
    load().locked_hosts
}

pub fn set_locked(host: &str, on: bool) {
    let _ = update(|p| {
        p.locked_hosts.retain(|h| h != host);
        if on {
            p.locked_hosts.push(host.to_string());
        }
    });
}

pub fn idle_mode(host: &str) -> Option<String> {
    load().idle_modes.get(host).cloned()
}

pub fn set_idle_mode(host: &str, mode: Option<&str>) {
    let _ = update(|p| match mode {
        Some(m) => {
            p.idle_modes.insert(host.to_string(), m.to_string());
        }
        None => {
            p.idle_modes.remove(host);
        }
    });
}

pub fn set_host_reconnect(host: &str, rule: Option<reconnect::Partial>) {
    let _ = update(|p| match rule {
        Some(r) if !r.is_empty() => {
            p.host_reconnect.insert(host.to_string(), r);
        }
        _ => {
            p.host_reconnect.remove(host);
        }
    });
}

/// When the file last changed, to notice an edit from another process.
pub fn modified() -> Option<std::time::SystemTime> {
    std::fs::metadata(&config::get().prefs_file)
        .ok()?
        .modified()
        .ok()
}
