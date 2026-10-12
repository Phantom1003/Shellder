//! The windows: one main window, Settings, the floating prompt window, and
//! one Files window and one Console window per host (a Console window can
//! be switched to another host, and then shows one its label does not
//! name: `SHOWING` says which). Also what closing
//! the last one means (background or quit, per Settings) and, on macOS,
//! whether the app has a Dock tile.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use super::model::Model;
use super::terminals;
use crate::core::log;
use crate::core::prefs;

pub const MAIN: &str = "main";
pub const SETTINGS: &str = "settings";
pub const PROMPT: &str = "prompt";

/// A kind of window's size when it is new, and the smallest the user can
/// drag it to. The minimum binds the user only: a size set from code (the
/// saved one tauri-plugin-window-state restores) goes below it unchecked,
/// so `on_window_event` puts a window that came up smaller back to the
/// default.
struct Sizes {
    default: (f64, f64),
    min: (f64, f64),
}

const MAIN_SIZES: Sizes = Sizes {
    default: (900.0, 560.0),
    min: (720.0, 420.0),
};
const SETTINGS_SIZES: Sizes = Sizes {
    default: (560.0, 640.0),
    min: (480.0, 320.0),
};
const FILES_SIZES: Sizes = Sizes {
    default: (880.0, 540.0),
    min: (800.0, 380.0),
};
const CONSOLE_SIZES: Sizes = Sizes {
    default: (1000.0, 640.0),
    min: (760.0, 420.0),
};

fn sizes_of(label: &str) -> Option<&'static Sizes> {
    if label == MAIN {
        Some(&MAIN_SIZES)
    } else if label == SETTINGS {
        Some(&SETTINGS_SIZES)
    } else if label.starts_with("files-") {
        Some(&FILES_SIZES)
    } else if label.starts_with("console-") {
        Some(&CONSOLE_SIZES)
    } else {
        None
    }
}

/// A window that came out smaller than its minimum (a saved size that went
/// wrong: nothing the user can drag gets there) is put back to the
/// default, so it never opens too small to use.
fn keep_above_minimum(window: &tauri::Window, size: tauri::PhysicalSize<u32>) {
    let Some(sizes) = sizes_of(window.label()) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let logical: tauri::LogicalSize<f64> = size.to_logical(scale);
    if logical.width + 0.5 < sizes.min.0 || logical.height + 0.5 < sizes.min.1 {
        log::warn(format!(
            "{}: window came up {}x{}, below its minimum; using the default size",
            window.label(),
            logical.width.round(),
            logical.height.round()
        ));
        let _ = window.set_size(tauri::LogicalSize::new(sizes.default.0, sizes.default.1));
    }
}

/// A window label from a host alias: labels allow fewer characters than
/// ssh aliases.
fn host_label(prefix: &str, host: &str) -> String {
    let safe: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{prefix}-{safe}")
}

pub fn files_label(host: &str) -> String {
    host_label("files", host)
}

pub fn console_label(host: &str) -> String {
    host_label("console", host)
}

fn own_windows(app: &AppHandle) -> Vec<WebviewWindow> {
    app.webview_windows().into_values().collect()
}

/// Whether a window other than `except` is still open: on screen, or
/// minimized to the Dock. A minimized Console or Files window is one the
/// user means to come back to, so it keeps the app in the Dock (an
/// accessory app's minimized windows vanish from it) and closing the main
/// window beside it is not closing the last one.
fn any_open(app: &AppHandle, except: Option<&str>) -> bool {
    own_windows(app).iter().any(|w| {
        Some(w.label()) != except
            && (w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false))
    })
}

/// Dock tile only while one of our windows is open; otherwise the app is
/// an accessory: no Dock tile, menu bar item only. Not a setting.
#[cfg(target_os = "macos")]
fn apply_activation_policy(app: &AppHandle, window_visible: bool) {
    use tauri::ActivationPolicy;
    let _ = app.set_activation_policy(if window_visible {
        ActivationPolicy::Regular
    } else {
        ActivationPolicy::Accessory
    });
}

#[cfg(not(target_os = "macos"))]
fn apply_activation_policy(_app: &AppHandle, _window_visible: bool) {}

/// The windows whose page has drawn itself (`window_ready`), and the ones
/// asked on screen before that. A window is built hidden and stays so
/// until its page says it is ready: shown earlier, the web view's blank
/// first frame would be what the user sees of the launch. A page that
/// never says so (a script error) still gets its window after
/// `READY_FALLBACK`.
static READY: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static WANTED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
const READY_FALLBACK: std::time::Duration = std::time::Duration::from_secs(3);

fn set(m: &Mutex<Option<HashSet<String>>>) -> std::sync::MutexGuard<'_, Option<HashSet<String>>> {
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(HashSet::new());
    }
    g
}

fn is_ready(label: &str) -> bool {
    set(&READY).as_ref().is_some_and(|s| s.contains(label))
}

/// The page of `label` has drawn itself: show the window if it was asked
/// for. Runs on the main thread.
pub fn page_ready(app: &AppHandle, label: &str) {
    set(&READY).as_mut().map(|s| s.insert(label.to_string()));
    let wanted = set(&WANTED).as_mut().is_some_and(|s| s.remove(label));
    if wanted {
        if let Some(w) = app.get_webview_window(label) {
            present(app, &w);
        }
    }
}

fn forget(label: &str) {
    set(&READY).as_mut().map(|s| s.remove(label));
    unwant(label);
    showing(|m| m.remove(label));
}

/// Which host each Console window shows, by label. A Console window
/// opens on one host, which names its label, and can be switched to
/// another (`switch_console`), after which the label no longer says: this
/// does, and `show_console` looks here before building a window for a
/// host that has one.
static SHOWING: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn showing<R>(f: impl FnOnce(&mut HashMap<String, String>) -> R) -> R {
    let mut g = SHOWING.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// The Console window showing `host`, if one does.
fn console_showing(app: &AppHandle, host: &str) -> Option<WebviewWindow> {
    let label = showing(|m| {
        m.iter()
            .find(|(_, h)| h.as_str() == host)
            .map(|(l, _)| l.clone())
    })?;
    app.get_webview_window(&label)
}

/// The window is put away before its page was ready: do not show it when
/// the page gets there.
fn unwant(label: &str) {
    set(&WANTED).as_mut().map(|s| s.remove(label));
}

/// Bring a window on screen: first restore the Dock tile (so the app can
/// become active), then order the window front. A window whose page is
/// not drawn yet is shown when it is (`page_ready`).
fn present(app: &AppHandle, w: &WebviewWindow) {
    let label = w.label().to_string();
    if !is_ready(&label) {
        set(&WANTED).as_mut().map(|s| s.insert(label.clone()));
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(READY_FALLBACK);
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                if set(&WANTED).as_mut().is_some_and(|s| s.remove(&label)) {
                    log::warn(format!(
                        "{label}: page not ready after {}s, showing the window anyway",
                        READY_FALLBACK.as_secs()
                    ));
                    set(&READY).as_mut().map(|s| s.insert(label.clone()));
                    if let Some(w) = a.get_webview_window(&label) {
                        present(&a, &w);
                    }
                }
            });
        });
        return;
    }
    apply_activation_policy(app, true);
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
}

/// The page's background (app.css `--bg`) for the system appearance, so
/// what the window shows before and between the page's frames (a resize
/// outrunning the web view) is not white.
fn background(theme: Option<tauri::Theme>) -> tauri::window::Color {
    match theme {
        Some(tauri::Theme::Dark) => tauri::window::Color(0x1e, 0x1e, 0x1e, 0xff),
        _ => tauri::window::Color(0xf5, 0xf5, 0xf7, 0xff),
    }
}

fn paint(w: &WebviewWindow) {
    let _ = w.set_background_color(Some(background(w.theme().ok())));
}

/// A new window: built hidden, painted in the page's background colour, and
/// told the language so the page can fetch its translation while it asks
/// the app for the state.
fn build(b: WebviewWindowBuilder<'_, tauri::Wry, AppHandle>) -> tauri::Result<WebviewWindow> {
    let lang = serde_json::to_string(&prefs::load().language()).unwrap_or_else(|_| "\"en\"".into());
    let w = b
        .visible(false)
        .initialization_script(format!("window.__SHELLDER_LANG = {lang};"))
        .build()?;
    paint(&w);
    Ok(w)
}

fn sized<'a>(
    b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    sizes: &Sizes,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    b.inner_size(sizes.default.0, sizes.default.1)
        .min_inner_size(sizes.min.0, sizes.min.1)
}

fn builder<'a>(
    app: &'a AppHandle,
    label: &str,
    page: &str,
    title: &str,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let b = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into())).title(title);
    // No title bar on macOS: the page runs up under the traffic lights, and
    // app.css keeps its top clear of them under the mac class. Elsewhere
    // the window manager draws the title bar above the page.
    #[cfg(target_os = "macos")]
    let b = b
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .initialization_script("document.documentElement.classList.add('mac')");
    b
}

pub fn show_main(app: &AppHandle) {
    let w = match app.get_webview_window(MAIN) {
        Some(w) => w,
        None => {
            match build(sized(builder(app, MAIN, "index.html", "Shellder"), &MAIN_SIZES).center()) {
                Ok(w) => w,
                Err(e) => {
                    log::error(format!("cannot open the main window: {e}"));
                    return;
                }
            }
        }
    };
    present(app, &w);
}

/// A click on the Dock tile, or the bundle launched again: a window on
/// screen needs nothing; a window minimized to the Dock (a Console, say)
/// comes back out, not the main window beside it; only an app with no
/// window open gets its main window.
pub fn reopen(app: &AppHandle) {
    let open = own_windows(app);
    if open.iter().any(|w| w.is_visible().unwrap_or(false)) {
        return;
    }
    if let Some(w) = open.iter().find(|w| w.is_minimized().unwrap_or(false)) {
        present(app, w);
        return;
    }
    show_main(app);
}

/// The menu bar icon: bring the window up, or put it away when it is
/// already in front. Away means what closing it means (background or
/// quit, per Settings), except that a click never quits: it hides.
pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
            if !any_open(app, None) {
                apply_activation_policy(app, false);
            }
            return;
        }
    }
    show_main(app);
}

pub fn show_settings(app: &AppHandle) {
    let w = match app.get_webview_window(SETTINGS) {
        Some(w) => w,
        None => match build(
            sized(
                builder(app, SETTINGS, "settings.html", "Shellder Settings"),
                &SETTINGS_SIZES,
            )
            .center(),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::error(format!("cannot open the settings window: {e}"));
                return;
            }
        },
    };
    present(app, &w);
}

/// The Files window of one host: its tree next to this machine's.
pub fn show_files(app: &AppHandle, host: &str) {
    let label = files_label(host);
    let w = match app.get_webview_window(&label) {
        Some(w) => w,
        None => {
            let page = format!("files.html?host={}", urlencode(host));
            match build(
                sized(
                    builder(app, &label, &page, &format!("Files — {host}")),
                    &FILES_SIZES,
                )
                .center(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    log::error(format!("{host}: cannot open the Files window: {e}"));
                    return;
                }
            }
        }
    };
    present(app, &w);
}

/// The Console window of one host: what it is doing, and its tmux
/// sessions, one of which the window can open (the page measures its
/// terminal and asks for it with `term_open`).
pub fn show_console(app: &AppHandle, host: &str) {
    let w = match console_showing(app, host) {
        Some(w) => w,
        None => {
            // The label is the host's, unless a window switched to another
            // host still has it (or another host's alias comes to the
            // same label): then the next free one.
            let base = console_label(host);
            let mut label = base.clone();
            let mut n = 1;
            while app.get_webview_window(&label).is_some() {
                n += 1;
                label = format!("{base}-{n}");
            }
            let page = format!("console.html?host={}", urlencode(host));
            match build(sized(builder(app, &label, &page, host), &CONSOLE_SIZES).center()) {
                Ok(w) => {
                    showing(|m| m.insert(label, host.to_string()));
                    w
                }
                Err(e) => {
                    log::error(format!("{host}: cannot open the Console window: {e}"));
                    return;
                }
            }
        }
    };
    present(app, &w);
}

/// The Console window `label` switches to `host`: its terminals are hung
/// up (a tmux or screen session lives on) and its page starts again on
/// that host, in the same window, where it is and as big as it is. A
/// window already showing the host is brought up instead, this one left
/// as it is.
pub fn switch_console(app: &AppHandle, label: &str, host: &str) {
    if !label.starts_with("console-") {
        return;
    }
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    if let Some(other) = console_showing(app, host) {
        if other.label() != label {
            present(app, &other);
        }
        return;
    }
    showing(|m| m.insert(label.to_string(), host.to_string()));
    let _ = w.set_title(host);
    // The page switches in place: its tabs stay, each on the host it
    // runs on (`term_open` takes the host), the column reads the new one.
    if let Err(e) = app.emit_to(label, "console-host", serde_json::json!({ "host": host })) {
        log::error(format!("{label}: cannot switch to {host}: {e}"));
        return;
    }
    log::info(format!("{label}: switched to {host}"));
}

/// An interactive `ssh host` in the user's own terminal, through the
/// `ssh://` URL handler (Terminal unless another application claims it).
/// That ssh is not ours: it asks its questions there, and goes through
/// the master when one is up.
pub fn open_in_terminal_app(app: &AppHandle, host: &str) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = format!("ssh://{host}");
    match app.opener().open_url(&url, None::<&str>) {
        Ok(()) => {
            log::info(format!("{host}: shell opened in the ssh:// handler"));
            Ok(())
        }
        Err(e) => {
            log::warn(format!("{host}: could not open a shell: {e}"));
            Err(e.to_string())
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The floating prompt window: shown while a question waits, put away when
/// none does. It works before the main window exists and while it is closed.
pub fn present_prompt(app: &AppHandle, pending: bool) {
    if !pending {
        unwant(PROMPT);
        if let Some(w) = app.get_webview_window(PROMPT) {
            let _ = w.hide();
            if !any_open(app, None) {
                apply_activation_policy(app, false);
            }
        }
        return;
    }
    let w = match app.get_webview_window(PROMPT) {
        Some(w) => w,
        None => {
            let b = WebviewWindowBuilder::new(app, PROMPT, WebviewUrl::App("prompt.html".into()))
                .title("Shellder — ssh is asking")
                .inner_size(500.0, 380.0)
                .resizable(false)
                .always_on_top(true)
                .center();
            match build(b) {
                Ok(w) => w,
                Err(e) => {
                    log::error(format!("cannot open the prompt window: {e}"));
                    return;
                }
            }
        }
    };
    present(app, &w);
}

/// What closing a window means. Closing the last one sends the app to the
/// background (it leaves the Dock and lives in the menu bar) or quits,
/// per Settings. The prompt window never closes on its own: it is hidden
/// when nothing is asked.
pub fn on_window_event(model: &Arc<Model>, window: &tauri::Window, event: &WindowEvent) {
    match event {
        // A window that is gone comes back as a new one, with a new page.
        WindowEvent::Destroyed => forget(window.label()),
        WindowEvent::Resized(size) => keep_above_minimum(window, *size),
        // The page follows the appearance by itself; the window behind it
        // has to be told.
        WindowEvent::ThemeChanged(theme) => {
            let _ = window.set_background_color(Some(background(Some(*theme))));
        }
        _ => {}
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        let label = window.label().to_string();
        if label == PROMPT {
            api.prevent_close();
            return;
        }
        if label.starts_with("console-") {
            terminals::close_window(&label);
        }
        let last = !any_open(app, Some(&label));
        let keep = prefs::load().keep_in_background;
        if label == MAIN {
            // The main window is kept (hidden) so its state survives.
            api.prevent_close();
            let _ = window.hide();
        }
        if last {
            if keep {
                apply_activation_policy(app, false);
            } else {
                quit(model, app);
            }
        }
    }
}

/// Stop the masters, then leave.
pub fn quit(model: &Arc<Model>, app: &AppHandle) {
    let model = model.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        model.shutdown();
        app.exit(0);
    });
}
