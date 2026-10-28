//! App preferences (which hosts are locked, UI options): one JSON file in
//! the state directory, never ssh configuration. Written whole, read fresh
//! on every access so the CLI and the app see each other's changes.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use super::config;
use super::log;
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
    /// how much what is behind it is blurred (macOS): 0 (not at all)
    /// to 5.
    pub opacity: Option<f64>,
    pub blur_strength: Option<u32>,
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
    pub fn terminal_blur(&self) -> u32 {
        self.terminal.blur_strength.unwrap_or(0).min(5)
    }
}

/// What reading the file found.
#[derive(Debug, PartialEq)]
enum Found {
    Prefs(Box<Prefs>),
    /// No file yet: the defaults, and nothing to keep.
    Missing,
    /// A file that cannot be read or does not parse (a hand edit gone
    /// wrong, a newer copy's format), and why. The defaults stand in for
    /// it, but it is not written over unseen: `write` keeps it aside.
    Unreadable(String),
}

impl Found {
    fn unreadable(&self) -> Option<&str> {
        match self {
            Found::Unreadable(why) => Some(why),
            _ => None,
        }
    }

    fn prefs(self) -> Prefs {
        match self {
            Found::Prefs(p) => *p,
            _ => Prefs::default(),
        }
    }
}

/// Read the file. Of a file that does not parse, only the place where
/// it goes wrong is said, never what is in it.
fn read(path: &Path) -> Found {
    match std::fs::read(path) {
        Ok(data) => match serde_json::from_slice(&data) {
            Ok(p) => Found::Prefs(p),
            Err(e) => Found::Unreadable(format!(
                "does not parse (line {}, column {})",
                e.line(),
                e.column()
            )),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Found::Missing,
        Err(e) => Found::Unreadable(format!("cannot be read ({e})")),
    }
}

/// Where a file that could not be read is kept when a change replaces it.
fn unreadable_path(path: &Path) -> PathBuf {
    path.with_extension("json.unreadable")
}

/// Said once until the file is written again: `load` runs for every
/// reconnect rule and every window, and one line says it all.
static WARNED: AtomicBool = AtomicBool::new(false);

pub fn load() -> Prefs {
    let path = &config::get().prefs_file;
    let found = read(path);
    if let Some(why) = found.unreadable() {
        if !WARNED.swap(true, Ordering::Relaxed) {
            log::warn(format!(
                "prefs: {} {why}; the defaults stand in for it, and the next change keeps it as {}",
                config::path_str(path),
                config::path_str(&unreadable_path(path))
            ));
        }
    }
    found.prefs()
}

/// One writer at a time in this process: `update` is a read, a change and
/// a write, and the commands of every window, the daemon and the threads
/// that save a size once the dragging stops call it at the same time;
/// without the lock one of two changes made together would be lost. Not
/// reentrant: what `update` is given must not save the prefs itself.
static WRITER: Mutex<()> = Mutex::new(());

fn writer() -> MutexGuard<'static, ()> {
    WRITER.lock().unwrap_or_else(|e| e.into_inner())
}

/// A file that could not be read and was moved aside by a write: why it
/// could not be read, and where it is now.
type KeptAside = Option<(String, PathBuf)>;

/// Write the file whole through a temporary file of this write's own (two
/// writes through one name would truncate each other's). A file that
/// could not be read (`unreadable` says why) is moved aside first, so
/// that nothing is lost unseen.
fn write(path: &Path, p: &Prefs, unreadable: Option<&str>) -> std::io::Result<KeptAside> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "prefs.json".into());
    let tmp = path.with_file_name(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let data = serde_json::to_vec_pretty(p).map_err(std::io::Error::other)?;
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&data)?;
        f.sync_all()?;
        let mut kept = None;
        if let Some(why) = unreadable {
            let aside = unreadable_path(path);
            match std::fs::rename(path, &aside) {
                Ok(()) => kept = Some((why.to_string(), aside)),
                // Gone meanwhile: nothing left to keep.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(std::io::Error::new(
                        e.kind(),
                        format!("cannot keep the unreadable prefs aside: {e}"),
                    ))
                }
            }
        }
        std::fs::rename(&tmp, path)?;
        Ok(kept)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// After a write: the file is readable again, and one kept aside is said.
fn written(path: &Path, kept: KeptAside) {
    WARNED.store(false, Ordering::Relaxed);
    if let Some((why, aside)) = kept {
        log::warn(format!(
            "prefs: {} {why}; kept as {} and replaced",
            config::path_str(path),
            config::path_str(&aside)
        ));
    }
}

pub fn save(p: &Prefs) -> std::io::Result<()> {
    let _w = writer();
    config::ensure_dirs();
    let path = &config::get().prefs_file;
    let kept = write(path, p, read(path).unreadable())?;
    written(path, kept);
    Ok(())
}

/// Read, change, write, as one: no other write of this process comes in
/// between.
pub fn update(change: impl FnOnce(&mut Prefs)) -> std::io::Result<()> {
    config::ensure_dirs();
    let path = &config::get().prefs_file;
    let kept = update_at(path, change)?;
    written(path, kept);
    Ok(())
}

fn update_at(path: &Path, change: impl FnOnce(&mut Prefs)) -> std::io::Result<KeptAside> {
    let _w = writer();
    let found = read(path);
    let unreadable = found.unreadable().map(String::from);
    let mut p = found.prefs();
    change(&mut p);
    write(path, &p, unreadable.as_deref())
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

#[cfg(test)]
mod tests {
    //! The file's reading and writing, on files of their own: nothing here
    //! touches the state directory.
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("shellder-prefs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_a_written_one_reads_back() {
        let dir = scratch("roundtrip");
        let path = dir.join("prefs.json");
        assert_eq!(read(&path), Found::Missing);
        let mut p = Prefs::default();
        p.locked_hosts.push("a".into());
        assert_eq!(write(&path, &p, None).unwrap(), None);
        assert_eq!(read(&path), Found::Prefs(Box::new(p)));
        assert_eq!(entries(&dir), ["prefs.json"], "no temporary file left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that does not parse is not the defaults written over it: the
    /// first change keeps it aside, and nothing of it is said but where it
    /// goes wrong.
    #[test]
    fn an_unreadable_file_is_kept_aside_by_the_next_change() {
        let dir = scratch("unreadable");
        let path = dir.join("prefs.json");
        let broken = br#"{"lockedHosts": ["s3cret-host""#;
        std::fs::write(&path, broken).unwrap();
        let found = read(&path);
        let why = found.unreadable().expect("unreadable").to_string();
        assert!(why.contains("line 1, column"), "{why}");
        assert!(!why.contains("s3cret"), "{why}");
        assert_eq!(found.prefs(), Prefs::default());

        let kept = update_at(&path, |p| p.keep_in_background = true).unwrap();
        let (said, aside) = kept.expect("kept aside");
        assert_eq!(said, why);
        assert_eq!(aside, dir.join("prefs.json.unreadable"));
        assert_eq!(std::fs::read(&aside).unwrap(), broken);
        let Found::Prefs(p) = read(&path) else {
            panic!("not written");
        };
        assert!(p.keep_in_background);
        // Readable now: the next change keeps nothing more.
        assert_eq!(update_at(&path, |p| p.silent_launch = true).unwrap(), None);
        assert_eq!(std::fs::read(&aside).unwrap(), broken);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Changes made at the same time all land: one is not read before
    /// another is written and then written over it, and two writes do not
    /// share a temporary file.
    #[test]
    fn changes_made_together_all_land() {
        let dir = scratch("together");
        let path = dir.join("prefs.json");
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for i in 0..10 {
                        update_at(&path, |p| p.locked_hosts.push(format!("h{t}-{i}"))).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let Found::Prefs(p) = read(&path) else {
            panic!("unreadable after the changes");
        };
        assert_eq!(p.locked_hosts.len(), 80);
        assert_eq!(entries(&dir), ["prefs.json"], "no temporary file left");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
