//! The windows: one main window, Settings, the floating prompt window, and
//! one Files window and one Console window per host (a Console window can
//! be switched to another host, and then shows one its label does not
//! name: `SHOWING` says which). Also what closing
//! the last one means (background or quit, per Settings) and, on macOS,
//! whether the app has a Dock tile.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use super::model::Model;
use super::terminals;
use crate::core::log;
use crate::core::prefs;
use crate::core::ssh;

pub const MAIN: &str = "main";
pub const SETTINGS: &str = "settings";
pub const PROMPT: &str = "prompt";
/// The quick terminal: one window, hidden rather than closed.
pub const QUICK: &str = "quick";
/// The Console's settings window: the terminals, the quick terminal, the
/// themes, the keys.
pub const TERMPREFS: &str = "termprefs";
/// How much of the screen the quick terminal takes until it is dragged.
const QUICK_WIDTH: f64 = 1.0;
const QUICK_HEIGHT: f64 = 0.33;

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
const TERMPREFS_SIZES: Sizes = Sizes {
    default: (620.0, 760.0),
    min: (520.0, 400.0),
};

fn sizes_of(label: &str) -> Option<&'static Sizes> {
    if label == MAIN {
        Some(&MAIN_SIZES)
    } else if label == SETTINGS {
        Some(&SETTINGS_SIZES)
    } else if label == TERMPREFS {
        Some(&TERMPREFS_SIZES)
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
    // A window the snap keys have sized (`snap`) has no minimum any more:
    // a quarter of a small screen is narrower than the Console's, and
    // was put back to the default here as soon as it was placed.
    if unbounded(|u| u.contains(window.label())) {
        return;
    }
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

/// The windows whose minimum size `snap` lifted, by label.
static UNBOUNDED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn unbounded<T>(f: impl FnOnce(&mut HashSet<String>) -> T) -> T {
    let mut u = UNBOUNDED.lock().unwrap_or_else(|e| e.into_inner());
    f(u.get_or_insert_with(HashSet::new))
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

/// A Console window's title until its page sets its own (which it
/// translates): the host, or Local for this machine.
fn console_title(host: &str) -> &str {
    if crate::core::ssh::is_local(host) {
        "Local"
    } else {
        host
    }
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
const READY_FALLBACK: Duration = Duration::from_secs(3);

type Job = Box<dyn FnOnce() + Send>;

/// What is to be done a while from now, by key: one thread waits for all
/// of it (`after`), where a thread each slept through its own wait.
static DUE: Mutex<Vec<(String, Instant, Job)>> = Mutex::new(Vec::new());
static DUE_CHANGED: Condvar = Condvar::new();
static TIMER: Once = Once::new();

/// Run `job` on the timer thread once `delay` has passed (a job that
/// touches a window hops to the main thread itself). A job already
/// waiting under `key` is replaced: with `restart` the wait starts over,
/// the debounce of a drag; without, the new job keeps the old one's time.
fn after(key: &str, delay: Duration, restart: bool, job: impl FnOnce() + Send + 'static) {
    TIMER.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("shellder-timer".into())
            .spawn(run_timer);
        if let Err(e) = spawned {
            log::error(format!("no timer thread: {e}"));
        }
    });
    let mut due = DUE.lock().unwrap_or_else(|e| e.into_inner());
    let mut at = Instant::now() + delay;
    if let Some(i) = due.iter().position(|(k, _, _)| k == key) {
        let (_, was, _) = due.remove(i);
        if !restart {
            at = was;
        }
    }
    due.push((key.to_string(), at, Box::new(job)));
    DUE_CHANGED.notify_one();
}

fn run_timer() {
    let mut due = DUE.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let now = Instant::now();
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut *due)
            .into_iter()
            .partition(|(_, at, _)| *at <= now);
        *due = waiting;
        if !ready.is_empty() {
            drop(due);
            for (_, _, job) in ready {
                job();
            }
            due = DUE.lock().unwrap_or_else(|e| e.into_inner());
            continue;
        }
        due = match due.iter().map(|(_, at, _)| *at).min() {
            Some(next) => {
                DUE_CHANGED
                    .wait_timeout(due, next.saturating_duration_since(now))
                    .unwrap_or_else(|e| e.into_inner())
                    .0
            }
            None => DUE_CHANGED.wait(due).unwrap_or_else(|e| e.into_inner()),
        };
    }
}

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
    blurred(|m| m.remove(label));
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

/// The Console window showing `host`, if one does (not the quick
/// terminal, which is in the map too but is nobody's Console).
fn console_showing(app: &AppHandle, host: &str) -> Option<WebviewWindow> {
    let label = showing(|m| {
        m.iter()
            .find(|(l, h)| h.as_str() == host && l.starts_with("console-"))
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
        after(
            &format!("ready:{label}"),
            READY_FALLBACK,
            false,
            move || {
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
            },
        );
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

/// A window with terminals (a Console, the quick terminal) is built
/// transparent and not painted: the page paints the bars and the column,
/// and the terminals' background with the opacity Settings give it, so
/// that a change of opacity reaches every window at once. What is
/// behind a see-through terminal area is blurred on macOS when asked
/// (`apply_terminal_effects`, at build and with every change). The click
/// that brings such a window to the front reaches the page, which gives
/// the terminal under it the keyboard (`watchFirstClick` in console.js):
/// AppKit keeps that click to itself otherwise, and WebKit gives the
/// keyboard back to whatever had it, the column or a field, so a click
/// on a terminal took two.
fn glass(
    b: WebviewWindowBuilder<'_, tauri::Wry, AppHandle>,
) -> WebviewWindowBuilder<'_, tauri::Wry, AppHandle> {
    b.transparent(true).accept_first_mouse(true)
}

/// Whether the window is one of those.
fn has_terminals(label: &str) -> bool {
    label.starts_with("console-") || label == QUICK
}

/// The blur behind the terminal windows as Settings have it now: as
/// strong as asked while the terminals are see-through, off otherwise.
/// Five steps, no more than 24 (the blur of old, when it was on or
/// off): past that the backdrop is a smear that looks like no
/// see-through at all. On macOS it is the window server's own blur of
/// what is behind the window (what Ghostty, iTerm2 and Alacritty use),
/// not an AppKit material, which tints and all but hides the backdrop
/// in the light appearance. A window already given that radius is left
/// alone: a drag of the opacity slider within see-through comes here
/// several times a second. Runs on the main thread.
pub fn apply_terminal_effects(app: &AppHandle) {
    let p = prefs::load();
    let radius = if p.terminal_opacity() < 1.0 {
        [0, 4, 8, 12, 18, 24][p.terminal_blur() as usize]
    } else {
        0
    };
    for w in own_windows(app) {
        if !has_terminals(w.label()) {
            continue;
        }
        if blurred(|m| m.insert(w.label().to_string(), radius)) == Some(radius) {
            continue;
        }
        #[cfg(target_os = "macos")]
        {
            set_window_blur(&w, radius);
            match_corners(&w);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = &w;
    }
}

/// The blur radius each terminal window was last given, by label
/// (`apply_terminal_effects`); forgotten with the window.
static BLURRED: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

fn blurred<T>(f: impl FnOnce(&mut HashMap<String, i64>) -> T) -> T {
    let mut m = BLURRED.lock().unwrap_or_else(|e| e.into_inner());
    f(m.get_or_insert_with(HashMap::new))
}

/// A window with terminals gets its corner radius set again, the value
/// AppKit gave it: on macOS 26 the window server clips the window's
/// pixels to the system corner (`_cornerRadius`, 16 points on a titled
/// window) but blurs what is behind the window (`set_window_blur`)
/// within the corner of old, a circle of 10, so a see-through terminal
/// had a lighter sliver of blurred backdrop outside its corner at the
/// bottom of the window. The radius set through AppKit's own private
/// setter reaches the window server for both, and the pixels and the
/// blur end on one curve, a circle of that radius rather than the
/// system's continuous one. Nothing in full screen (no corners there)
/// and nothing on a window without a radius (the quick terminal is
/// borderless, square); a system without the setter is left as it is.
/// A Console in its immersive mode, borderless too, gets a small corner
/// of its own (`IMMERSIVE_RADIUS`): its terminal runs to the window's
/// edges, and a cell's square corner sits well beside a small curve
/// where it clashed with the system's.
#[cfg(target_os = "macos")]
fn match_corners(w: &WebviewWindow) {
    if w.is_fullscreen().unwrap_or(false) {
        return;
    }
    let Some(r) = corner_radius(w) else {
        return;
    };
    let r = if immersive(|m| m.contains(w.label())) {
        IMMERSIVE_RADIUS
    } else {
        r
    };
    if r > 0.0 {
        set_corner_radius(w, r);
    }
}

/// The corner radius of an immersive Console window, in points.
#[cfg(target_os = "macos")]
const IMMERSIVE_RADIUS: f64 = 6.0;

/// The Console windows in immersive mode, by label.
#[cfg(target_os = "macos")]
static IMMERSIVE: Mutex<Option<HashSet<String>>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn immersive<T>(f: impl FnOnce(&mut HashSet<String>) -> T) -> T {
    let mut m = IMMERSIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(m.get_or_insert_with(HashSet::new))
}

/// The window's corner radius as AppKit has it (`_cornerRadius`), or
/// None where the private getter or setter is missing.
#[cfg(target_os = "macos")]
fn corner_radius(w: &WebviewWindow) -> Option<f64> {
    use objc2::runtime::AnyObject;
    let ns = w.ns_window().ok()?;
    unsafe {
        let win: &AnyObject = &*(ns as *mut AnyObject);
        let gets: bool = objc2::msg_send![win, respondsToSelector: objc2::sel!(_cornerRadius)];
        let sets: bool = objc2::msg_send![win, respondsToSelector: objc2::sel!(_setCornerRadius:)];
        if !gets || !sets {
            return None;
        }
        Some(objc2::msg_send![win, _cornerRadius])
    }
}

/// The window's corner radius set through AppKit's private setter, which
/// reaches the window server for the pixels and the blur alike. Only
/// after `corner_radius` has said the setter is there.
#[cfg(target_os = "macos")]
fn set_corner_radius(w: &WebviewWindow, r: f64) {
    use objc2::runtime::AnyObject;
    let Ok(ns) = w.ns_window() else {
        return;
    };
    unsafe {
        let win: &AnyObject = &*(ns as *mut AnyObject);
        let _: () = objc2::msg_send![win, _setCornerRadius: r];
    }
}

/// A window that comes up over whatever is on screen, a full-screen
/// window included: the prompt window and the quick terminal. On macOS
/// a window joins a full-screen window's space only with
/// `NSWindowCollectionBehaviorFullScreenAuxiliary`: without it, shown
/// while a Console (or any application) was full screen, the prompt
/// window came up on the desktop's space and the system switched over
/// to it, the full-screen Console sliding away and not coming back once
/// the question was answered. With it, and `CanJoinAllSpaces`
/// (`visible_on_all_workspaces`), the window is shown on the space the
/// user is in, over the full-screen window, at its floating level.
/// Set on the main thread, where AppKit's windows are touched.
#[cfg(target_os = "macos")]
fn over_full_screen(w: &WebviewWindow) {
    let win = w.clone();
    let _ = w.run_on_main_thread(move || {
        use objc2::runtime::AnyObject;
        const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
        const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
        let Ok(ns) = win.ns_window() else {
            return;
        };
        unsafe {
            let obj: &AnyObject = &*(ns as *mut AnyObject);
            let b: usize = objc2::msg_send![obj, collectionBehavior];
            let _: () = objc2::msg_send![obj, setCollectionBehavior: b | CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY];
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn over_full_screen(_w: &WebviewWindow) {}

/// The window server's two private calls the blur is made with, looked
/// up when first wanted rather than linked: they are not API, and a
/// macOS without one would refuse to launch an app linked against it,
/// where looked up it only goes without the blur.
#[cfg(target_os = "macos")]
struct WindowServer {
    main_connection: unsafe extern "C" fn() -> *const std::ffi::c_void,
    set_blur_radius: unsafe extern "C" fn(*const std::ffi::c_void, isize, i64) -> i32,
}

#[cfg(target_os = "macos")]
fn window_server() -> Option<&'static WindowServer> {
    static FOUND: std::sync::OnceLock<Option<WindowServer>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let find = |name: &std::ffi::CStr| unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
            let main = find(c"CGSMainConnectionID");
            let blur = find(c"CGSSetWindowBackgroundBlurRadius");
            if main.is_null() || blur.is_null() {
                log::warn("this macOS has not got the window server's blur: see-through terminals are not blurred");
                return None;
            }
            // The two symbols, with the signatures every terminal that
            // has the look calls them with.
            unsafe {
                Some(WindowServer {
                    main_connection: std::mem::transmute::<
                        *mut std::ffi::c_void,
                        unsafe extern "C" fn() -> *const std::ffi::c_void,
                    >(main),
                    set_blur_radius: std::mem::transmute::<
                        *mut std::ffi::c_void,
                        unsafe extern "C" fn(*const std::ffi::c_void, isize, i64) -> i32,
                    >(blur),
                })
            }
        })
        .as_ref()
}

/// The window server's blur of what is behind the window, `radius` 0 for
/// none: CoreGraphics' private window-server calls, as the terminals
/// that have the look make them.
#[cfg(target_os = "macos")]
fn set_window_blur(w: &WebviewWindow, radius: i64) {
    let Some(ws) = window_server() else {
        return;
    };
    let Ok(ns) = w.ns_window() else {
        return;
    };
    let wid: isize = unsafe {
        let obj: &objc2::runtime::AnyObject = &*(ns as *mut objc2::runtime::AnyObject);
        objc2::msg_send![obj, windowNumber]
    };
    let r = unsafe { (ws.set_blur_radius)((ws.main_connection)(), wid, radius) };
    if r != 0 {
        log::warn(format!("{}: window blur not set ({r})", w.label()));
    }
}

/// A new window: built hidden, painted in the page's background colour, and
/// told the language so the page can fetch its translation while it asks
/// the app for the state.
fn build(b: WebviewWindowBuilder<'_, tauri::Wry, AppHandle>) -> tauri::Result<WebviewWindow> {
    let w = build_unpainted(b)?;
    paint(&w);
    Ok(w)
}

/// `build` without the paint: a transparent window stays so.
fn build_unpainted(
    b: WebviewWindowBuilder<'_, tauri::Wry, AppHandle>,
) -> tauri::Result<WebviewWindow> {
    let lang = serde_json::to_string(&prefs::load().language()).unwrap_or_else(|_| "\"en\"".into());
    let mut b = b
        .visible(false)
        .initialization_script(format!("window.__SHELLDER_LANG = {lang};"));
    // A test hook (AGENTS.md, "Isolation hooks"): with SHELLDER_PROBE set
    // to a URL, every page loads `probe.js` from there once it is drawn,
    // so that a script can read the real web view (computed styles, what
    // a wheel does) and report back, where no screen is to be had.
    if let Ok(url) = std::env::var("SHELLDER_PROBE")
        .ok()
        .filter(|_| cfg!(debug_assertions))
        .ok_or(())
    {
        if let Ok(url) = serde_json::to_string(&url) {
            b = b.initialization_script(format!(
                "window.addEventListener('DOMContentLoaded', () => {{ const s = document.createElement('script'); s.src = {url} + '/probe.js?t=' + Date.now(); document.head.append(s); }});"
            ));
        }
    }
    b.build()
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

/// The window `label` on screen, built from `make` first when it is not
/// open; `failed` says in the log what could not be opened.
fn show_window<'a>(
    app: &'a AppHandle,
    label: &str,
    failed: &str,
    make: impl FnOnce() -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
) {
    let w = match app.get_webview_window(label) {
        Some(w) => w,
        None => match build(make()) {
            Ok(w) => w,
            Err(e) => {
                log::error(format!("{failed}: {e}"));
                return;
            }
        },
    };
    present(app, &w);
}

pub fn show_main(app: &AppHandle) {
    show_window(app, MAIN, "cannot open the main window", || {
        sized(builder(app, MAIN, "index.html", "Shellder"), &MAIN_SIZES).center()
    });
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
    show_window(app, SETTINGS, "cannot open the settings window", || {
        sized(
            builder(app, SETTINGS, "settings.html", "Shellder Settings"),
            &SETTINGS_SIZES,
        )
        .center()
    });
}

/// The Console's settings window: the terminals' look and manners, the
/// quick terminal, the theme files, the keys.
pub fn show_console_settings(app: &AppHandle) {
    show_window(
        app,
        TERMPREFS,
        "cannot open the Console settings window",
        || {
            sized(
                builder(app, TERMPREFS, "consoleprefs.html", "Console Settings"),
                &TERMPREFS_SIZES,
            )
            .center()
        },
    );
}

/// The Files window of one host: its tree next to this machine's.
pub fn show_files(app: &AppHandle, host: &str) {
    let label = files_label(host);
    let failed = format!("{host}: cannot open the Files window");
    show_window(app, &label, &failed, || {
        let page = format!("files.html?host={}", urlencode(host));
        sized(
            builder(app, &label, &page, &format!("Files — {host}")),
            &FILES_SIZES,
        )
        .center()
    });
}

/// The Console window of one host: what it is doing, and its tmux
/// sessions, one of which the window can open (the page measures its
/// terminal and asks for it with `term_open`).
pub fn show_console(app: &AppHandle, host: &str) {
    match console_showing(app, host) {
        Some(w) => present(app, &w),
        None => new_console(app, host),
    }
}

/// One more Console window on the host, whether or not it has one (⌘N
/// in a Console window). The label is the host's, unless a window still
/// has it (one switched to another host, another host's alias coming
/// to the same label, or the host's own): then the next free one.
pub fn new_console(app: &AppHandle, host: &str) {
    // An earlier run's terminals, left when it ended without hanging them
    // up, would hold their sessions on the user's master and refuse this
    // window's tabs theirs (MaxSessions): hang them up first, off the
    // thread that opens the window (one `ps`).
    std::thread::spawn(ssh::kill_orphans);
    let base = console_label(host);
    let mut label = base.clone();
    let mut n = 1;
    while app.get_webview_window(&label).is_some() {
        n += 1;
        label = format!("{base}-{n}");
    }
    let page = format!("console.html?host={}", urlencode(host));
    match build_unpainted(glass(
        sized(
            builder(app, &label, &page, console_title(host)),
            &CONSOLE_SIZES,
        )
        .center(),
    )) {
        Ok(w) => {
            showing(|m| m.insert(label, host.to_string()));
            apply_terminal_effects(app);
            present(app, &w);
        }
        Err(e) => log::error(format!("{host}: cannot open the Console window: {e}")),
    }
}

/// A Console's immersive mode (`console_immersive`): the page shows the
/// terminals alone, and the window gives up its title bar, whose buttons
/// lay over the page's top bar on macOS and would lie over a terminal
/// now; back out of it, the title bar returns. Elsewhere the window
/// manager's title bar stays: it is what moves the window.
pub fn set_immersive(w: &WebviewWindow, on: bool) {
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = w.set_decorations(!on) {
            log::warn(format!("{}: cannot change the decorations: {e}", w.label()));
        }
        immersive(|m| {
            if on {
                m.insert(w.label().to_string());
            } else {
                m.remove(w.label());
            }
        });
        // The small corner of an immersive window, once the decorations
        // have gone (tao takes them on the main queue, asynchronously).
        if on {
            let w = w.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let w2 = w.clone();
                let _ = w.run_on_main_thread(move || match_corners(&w2));
            });
        }
        // Decorations back are a plain title bar (tao sets a mask of its
        // own, without the full-size content view the window was built
        // with), which would push the page down by its height: the
        // overlay style is put back once that mask has landed (tao sets
        // it on the main queue, asynchronously; the style change reads
        // the mask as it is then).
        // The frame as it was: the title bar's coming and going has moved
        // the content rectangle, and the window with it.
        if !on {
            let frame = w.outer_position().ok().zip(w.outer_size().ok());
            let w = w.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let w2 = w.clone();
                let _ = w.run_on_main_thread(move || {
                    if let Err(e) = w2.set_title_bar_style(tauri::TitleBarStyle::Overlay) {
                        log::warn(format!(
                            "{}: cannot put the title bar back: {e}",
                            w2.label()
                        ));
                    }
                    if let Some((pos, size)) = frame {
                        let _ = w2.set_size(size);
                        let _ = w2.set_position(pos);
                    }
                    // The title bar back is a corner computed afresh.
                    match_corners(&w2);
                });
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (w, on);
}

/// A span on one axis of a screen's work area: the half at its start
/// (left, top), the whole of it, or the half at its end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Span {
    Start,
    Whole,
    End,
}

/// A rectangle in logical points: a work area or a window's frame. The
/// screens' own points, not pixels, so that screens of different scale
/// factors lie in one plane and a window keeps its size across them.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn of(m: &tauri::Monitor) -> Rect {
        let s = m.scale_factor();
        let p: tauri::LogicalPosition<f64> = m.work_area().position.to_logical(s);
        let z: tauri::LogicalSize<f64> = m.work_area().size.to_logical(s);
        Rect {
            x: p.x,
            y: p.y,
            w: z.width,
            h: z.height,
        }
    }

    /// The two halves of an axis of length `len` from `at`: the start's
    /// length, and where the end's begins.
    fn halves(at: f64, len: f64) -> (f64, f64) {
        let first = (len / 2.0).floor();
        (first, at + first)
    }

    /// Which span of the area's axis (`at`, `len`) a window's (`p`, `n`)
    /// fills, within a few points, or None when it is free there.
    fn span(at: f64, len: f64, p: f64, n: f64) -> Option<Span> {
        let near = |a: f64, b: f64| (a - b).abs() <= 6.0;
        let (first, second) = Rect::halves(at, len);
        if near(p, at) && near(n, len) {
            Some(Span::Whole)
        } else if near(p, at) && near(n, first) {
            Some(Span::Start)
        } else if near(p, second) && near(n, len - first) {
            Some(Span::End)
        } else {
            None
        }
    }

    /// The start and length of a span of the axis (`at`, `len`).
    fn place(at: f64, len: f64, span: Span) -> (f64, f64) {
        let (first, second) = Rect::halves(at, len);
        match span {
            Span::Start => (at, first),
            Span::Whole => (at, len),
            Span::End => (second, len - first),
        }
    }
}

/// The screen `step` places after `from` (-1 the one before, 1 the one
/// after) among `all`, the screens taken from left to right and, side
/// by side, from top to bottom, wrapping round at the ends. None with a
/// single screen.
fn next_screen(from: &Rect, all: &[Rect], step: i32) -> Option<Rect> {
    let mut order: Vec<Rect> = all.to_vec();
    order.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let n = order.len();
    if n < 2 {
        return None;
    }
    let at = order.iter().position(|r| r == from)?;
    Some(order[(at as i32 + step).rem_euclid(n as i32) as usize])
}

/// The window's screen, its frame and the span it fills on each axis
/// (None where it is free), in logical points; None full screen or
/// with no screen to read.
fn where_now(w: &WebviewWindow) -> Option<(Rect, Rect, Option<Span>, Option<Span>)> {
    if w.is_fullscreen().unwrap_or(false) {
        return None;
    }
    let m = w.current_monitor().ok().flatten()?;
    let area = Rect::of(&m);
    let scale = w.scale_factor().unwrap_or(1.0);
    let pos: tauri::LogicalPosition<f64> = w.outer_position().ok()?.to_logical(scale);
    let size: tauri::LogicalSize<f64> = w.outer_size().ok()?.to_logical(scale);
    let frame = Rect {
        x: pos.x,
        y: pos.y,
        w: size.width,
        h: size.height,
    };
    let now_h = Rect::span(area.x, area.w, frame.x, frame.w);
    let now_v = Rect::span(area.y, area.h, frame.y, frame.h);
    Some((area, frame, now_h, now_v))
}

/// The window on the screen before (`step` -1) or after (1) its own,
/// the screens taken from left to right and round at the ends
/// (`window_screen`): where it was, the same half or quarter, or, free,
/// its size and its distance from the screen's top left corner, held
/// within the screen. Nothing with a single screen, or full screen.
pub fn to_screen(w: &WebviewWindow, step: i32) {
    let Some((area, frame, now_h, now_v)) = where_now(w) else {
        return;
    };
    let screens: Vec<Rect> = w
        .available_monitors()
        .map(|ms| ms.iter().map(Rect::of).collect())
        .unwrap_or_default();
    let Some(target) = next_screen(&area, &screens, step) else {
        return;
    };
    let axis = |span: Option<Span>, at: f64, len: f64, p: f64, n: f64, from: f64| -> (f64, f64) {
        match span {
            Some(s) => Rect::place(at, len, s),
            None => {
                let n = n.min(len);
                ((p - from + at).min(at + len - n).max(at), n)
            }
        }
    };
    let (x, width) = axis(now_h, target.x, target.w, frame.x, frame.w, area.x);
    let (y, height) = axis(now_v, target.y, target.h, frame.y, frame.h, area.y);
    if w.is_maximized().unwrap_or(false) {
        let _ = w.unmaximize();
    }
    lift_minimum(w);
    place(
        w,
        Rect {
            x,
            y,
            w: width,
            h: height,
        },
    );
}

/// The window's minimum size taken off, once, before the snap keys
/// size it: a quarter of a small screen is narrower than the Console's
/// minimum, which would hold the window back, and `keep_above_minimum`
/// would then put it back to the default size.
fn lift_minimum(w: &WebviewWindow) {
    if unbounded(|u| u.insert(w.label().to_string())) {
        let _ = w.set_min_size::<tauri::LogicalSize<f64>>(None);
    }
}

/// The window in a half of its screen's work area, a quarter, or the
/// whole of it (the snap-* keys). `h` "left" or "right": the window
/// takes that half, keeping its height (a top half becomes the top
/// left quarter); in that half already, nothing. `v` "top" or
/// "bottom": one step that way on the vertical axis, bottom half to
/// the full height to the top half (so a quarter goes back to its
/// half, then on to the other quarter), keeping its width; at the top
/// already, nothing. Both "full": the whole work area, and pressed
/// there again, back to the frame it had. Where the window is now is
/// read off its frame, within a few points; a window placed anywhere
/// else counts as whole on both axes. The window's minimum size is
/// lifted first, so a quarter of a small screen fits. Nothing happens
/// full screen; another screen is `to_screen`.
pub fn snap(w: &WebviewWindow, h: Option<&str>, v: Option<&str>) {
    // The frame a window had before it was given the whole screen, by
    // label: what snap-fill goes back to when pressed again there.
    static FREE: std::sync::Mutex<Option<HashMap<String, Rect>>> = std::sync::Mutex::new(None);
    let Some((area, frame, now_h, now_v)) = where_now(w) else {
        return;
    };
    let mut free = FREE.lock().unwrap_or_else(|e| e.into_inner());
    let free = free.get_or_insert_with(HashMap::new);
    let fill = h == Some("full") && v == Some("full");
    if fill && now_h == Some(Span::Whole) && now_v == Some(Span::Whole) {
        if let Some(r) = free.remove(w.label()) {
            place(w, r);
        }
        return;
    }
    let mut span_h = now_h.unwrap_or(Span::Whole);
    let mut span_v = now_v.unwrap_or(Span::Whole);
    if fill {
        free.insert(w.label().to_string(), frame);
        span_h = Span::Whole;
        span_v = Span::Whole;
    } else if let Some(side) = h {
        let want = if side == "left" {
            Span::Start
        } else {
            Span::End
        };
        if now_h == Some(want) {
            return;
        }
        span_h = want;
    } else if let Some(way) = v {
        let up = way == "top";
        span_v = match (now_v, up) {
            (Some(Span::End), true) | (Some(Span::Start), false) => Span::Whole,
            (Some(Span::Start), true) | (Some(Span::End), false) => return,
            (_, true) => Span::Start,
            (_, false) => Span::End,
        };
    } else {
        return;
    }
    let (x, width) = Rect::place(area.x, area.w, span_h);
    let (y, height) = Rect::place(area.y, area.h, span_v);
    if w.is_maximized().unwrap_or(false) {
        let _ = w.unmaximize();
    }
    lift_minimum(w);
    place(
        w,
        Rect {
            x,
            y,
            w: width,
            h: height,
        },
    );
}

/// The window at a frame in logical points: its size first, then its
/// position, so a size the window manager holds back never leaves it
/// over an edge.
fn place(w: &WebviewWindow, r: Rect) {
    let _ = w.set_size(tauri::LogicalSize::new(r.w, r.h));
    let _ = w.set_position(tauri::LogicalPosition::new(r.x, r.y));
}

/// The quick terminal (the key in Settings, from any application): a
/// window with nothing but a shell on `host` (the Console page in its
/// quick mode: no bar, no column, no tab row), without decorations,
/// over every other window, as wide as the screen the pointer is on and
/// a third of its height, dropped in from its top edge; the key again,
/// Esc at the prompt or the keyboard going elsewhere take it away, and
/// the shell lives on, hidden, for the next time. Built once; a change
/// of host in Settings replaces it.
pub fn toggle_quick_terminal(app: &AppHandle, host: &str) {
    if let Some(w) = app.get_webview_window(QUICK) {
        let same = showing(|m| m.get(QUICK).map(|h| h == host).unwrap_or(false));
        if !same {
            // Another host in Settings: the page switches in place (its
            // shells are hung up and one starts on the new host), the way
            // a Console switches; the window stays.
            showing(|m| m.insert(QUICK.to_string(), host.to_string()));
            if let Err(e) = app.emit_to(QUICK, "console-host", serde_json::json!({ "host": host }))
            {
                log::error(format!("quick terminal: cannot switch to {host}: {e}"));
            }
            drop_quick(app, &w);
            return;
        } else if w.is_visible().unwrap_or(false) {
            hide_quick(app);
            return;
        } else {
            drop_quick(app, &w);
            return;
        }
    }
    let page = format!("console.html?host={}&quick=1", urlencode(host));
    match build_unpainted(glass(
        builder(app, QUICK, &page, "Shellder")
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .resizable(true)
            .inner_size(800.0, 300.0),
    )) {
        Ok(w) => {
            showing(|m| m.insert(QUICK.to_string(), host.to_string()));
            over_full_screen(&w);
            apply_terminal_effects(app);
            drop_quick(app, &w);
        }
        Err(e) => log::error(format!("{host}: cannot open the quick terminal: {e}")),
    }
}

/// The quick terminal on the screen the pointer is on, along its top
/// edge and centred, the size it was last dragged to (as shares of the
/// screen, so another screen gets the same look): placed just above the
/// edge, shown, and slid down over a moment.
fn drop_quick(app: &AppHandle, w: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else {
        present(app, w);
        return;
    };
    let area = m.work_area();
    let p = prefs::load();
    let wf = p.quick_width.unwrap_or(QUICK_WIDTH).clamp(0.2, 1.0);
    let hf = p.quick_height.unwrap_or(QUICK_HEIGHT).clamp(0.1, 1.0);
    let width = (area.size.width as f64 * wf) as u32;
    let height = (area.size.height as f64 * hf) as u32;
    let x = area.position.x + (area.size.width.saturating_sub(width) / 2) as i32;
    let top = area.position.y;
    // Placing, until the slide is done: the `Resized` the size change
    // sends arrives later, and is not the user's drag.
    QUICK_PLACING.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = w.set_size(tauri::PhysicalSize::new(width, height));
    let _ = w.set_position(tauri::PhysicalPosition::new(x, top - height as i32));
    present(app, w);
    // The slide: a few steps down over about a tenth of a second.
    let app = app.clone();
    let label = w.label().to_string();
    std::thread::spawn(move || {
        const STEPS: i32 = 8;
        for i in 1..=STEPS {
            std::thread::sleep(std::time::Duration::from_millis(14));
            let a = app.clone();
            let l = label.clone();
            let y = top - (height as i32 * (STEPS - i)) / STEPS;
            let _ = app.run_on_main_thread(move || {
                if let Some(w) = a.get_webview_window(&l) {
                    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                }
                if i == STEPS {
                    QUICK_PLACING.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            });
        }
    });
}

/// The quick terminal is being placed by `drop_quick`: its resize is
/// not the user's to remember.
static QUICK_PLACING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The shares last saved (read from the prefs at first use): a resize
/// that changes nothing writes nothing.
static QUICK_SAVED: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// The user dragged the quick terminal's edge: it stays centred along
/// the top of its screen (both sides move at once), and its size is
/// remembered as shares of the screen, once the drag has rested.
fn quick_resized(window: &tauri::Window, size: tauri::PhysicalSize<u32>) {
    if QUICK_PLACING.load(std::sync::atomic::Ordering::SeqCst)
        || !window.is_visible().unwrap_or(false)
    {
        return;
    }
    let Ok(Some(m)) = window.current_monitor() else {
        return;
    };
    let area = *m.work_area();
    let x = area.position.x + (area.size.width.saturating_sub(size.width) / 2) as i32;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, area.position.y));
    let wf =
        ((size.width as f64 / area.size.width as f64).clamp(0.2, 1.0) * 1000.0).round() / 1000.0;
    let hf =
        ((size.height as f64 / area.size.height as f64).clamp(0.1, 1.0) * 1000.0).round() / 1000.0;
    {
        let mut saved = QUICK_SAVED.lock().unwrap_or_else(|e| e.into_inner());
        if saved.is_none() {
            let p = prefs::load();
            *saved = Some((
                p.quick_width.unwrap_or(QUICK_WIDTH),
                p.quick_height.unwrap_or(QUICK_HEIGHT),
            ));
        }
        if *saved == Some((wf, hf)) {
            return;
        }
        *saved = Some((wf, hf));
    }
    // Every step of the drag puts the write off again: the size written
    // is the one the drag rested at.
    after("quick-size", Duration::from_millis(500), true, move || {
        let _ = prefs::update(|p| {
            p.quick_width = Some(wf);
            p.quick_height = Some(hf);
        });
    });
}

/// The quick terminal lost the keyboard: away, unless the keyboard went
/// to another window of ours (the prompt window asking for its shell's
/// login, a Console), which gives it back. Judged a moment later, since
/// the window taking the keyboard is told after the one losing it.
fn quick_unfocused(app: &AppHandle) {
    let app = app.clone();
    after(
        "quick-unfocused",
        Duration::from_millis(150),
        true,
        move || {
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                let ours = own_windows(&a)
                    .iter()
                    .any(|w| w.is_focused().unwrap_or(false));
                if !ours {
                    hide_quick(&a);
                }
            });
        },
    );
}

/// The quick terminal away (its shell kept).
pub fn hide_quick(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(QUICK) {
        let _ = w.hide();
        if !any_open(app, None) {
            apply_activation_policy(app, false);
        }
    }
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
    let _ = w.set_title(console_title(host));
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
/// The prompt window, for the question at the head of the queue, or
/// hidden when there is none. Its title says who asks: ssh, logging in,
/// or the host, for an action of the Console (`PromptView::about`).
pub fn present_prompt(app: &AppHandle, model: &Arc<Model>, pending: bool) {
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
                Ok(w) => {
                    over_full_screen(&w);
                    w
                }
                Err(e) => {
                    log::error(format!("cannot open the prompt window: {e}"));
                    return;
                }
            }
        }
    };
    let title = match model.action_prompt_host() {
        Some(host) => format!("Shellder — {host} is asking"),
        None => "Shellder — ssh is asking".to_string(),
    };
    let _ = w.set_title(&title);
    present(app, &w);
}

/// A Console window back from full screen (seen as a resize while not
/// full screen after one while it was) gets its corner set again
/// (`match_corners`): full screen took it away, and AppKit's own
/// computation of it on the way back is the one the blur does not
/// follow.
#[cfg(target_os = "macos")]
fn left_fullscreen(window: &tauri::Window) {
    static FULL: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let now = window.is_fullscreen().unwrap_or(false);
    let mut full = FULL.lock().unwrap_or_else(|e| e.into_inner());
    let full = full.get_or_insert_with(HashSet::new);
    let was = if now {
        full.insert(window.label().to_string())
    } else {
        full.remove(window.label())
    };
    if was && !now {
        if let Some(w) = window.get_webview_window(window.label()) {
            match_corners(&w);
        }
    }
}

/// What closing a window means. Closing the last one sends the app to the
/// background (it leaves the Dock and lives in the menu bar) or quits,
/// per Settings. The prompt window never closes on its own: it is hidden
/// when nothing is asked.
pub fn on_window_event(model: &Arc<Model>, window: &tauri::Window, event: &WindowEvent) {
    match event {
        // A window that is gone comes back as a new one, with a new page.
        // A Console window's terminals are hung up here, not when the
        // close was asked for: the page may still ask the user (a
        // command running in a tab) and keep the window.
        WindowEvent::Destroyed => {
            let label = window.label().to_string();
            forget(&label);
            if label.starts_with("console-") || label == QUICK {
                terminals::close_window(&label);
            }
            if label != MAIN {
                last_gone(model, window.app_handle(), None);
            }
        }
        WindowEvent::Resized(size) if window.label() == QUICK => quick_resized(window, *size),
        WindowEvent::Resized(size) => {
            keep_above_minimum(window, *size);
            #[cfg(target_os = "macos")]
            if has_terminals(window.label()) {
                left_fullscreen(window);
            }
        }
        // The quick terminal goes away with the keyboard.
        WindowEvent::Focused(false) if window.label() == QUICK => {
            quick_unfocused(window.app_handle())
        }
        // The page follows the appearance by itself; the window behind it
        // has to be told. Not a transparent Console window: it has no
        // colour of its own to keep.
        WindowEvent::ThemeChanged(theme) if !has_terminals(window.label()) => {
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
        if label == MAIN {
            // The main window is kept (hidden) so its state survives.
            api.prevent_close();
            let _ = window.hide();
            last_gone(model, app, Some(&label));
        }
    }
}

/// A window went: with none left, the app goes to the background
/// (Settings) or quits.
fn last_gone(model: &Arc<Model>, app: &AppHandle, except: Option<&str>) {
    if QUITTING.load(std::sync::atomic::Ordering::SeqCst) || any_open(app, except) {
        return;
    }
    if prefs::load().keep_in_background {
        apply_activation_policy(app, false);
    } else {
        quit(model, app);
    }
}

static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Stop the masters, then leave. The windows going down meanwhile do not
/// start it again.
pub fn quit(model: &Arc<Model>, app: &AppHandle) {
    if QUITTING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let model = model.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        model.shutdown();
        app.exit(0);
    });
}

#[cfg(test)]
mod timer_tests {
    use super::after;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn a_job_given_again_runs_once_and_others_run_beside_it() {
        let drag = Arc::new(AtomicUsize::new(0));
        let other = Arc::new(AtomicUsize::new(0));
        for _ in 0..50 {
            let d = drag.clone();
            after("test-drag", Duration::from_millis(400), true, move || {
                d.fetch_add(1, Ordering::SeqCst);
            });
        }
        let o = other.clone();
        after("test-other", Duration::from_millis(10), false, move || {
            o.fetch_add(1, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(other.load(Ordering::SeqCst), 1);
        assert_eq!(drag.load(Ordering::SeqCst), 0);
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(drag.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_job_given_again_without_restart_keeps_its_time() {
        let ran = Arc::new(AtomicUsize::new(0));
        let r = ran.clone();
        after("test-keep", Duration::from_millis(200), false, move || {
            r.fetch_add(1, Ordering::SeqCst);
        });
        let r = ran.clone();
        after("test-keep", Duration::from_secs(60), false, move || {
            r.fetch_add(10, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(ran.load(Ordering::SeqCst), 10);
    }
}

#[cfg(test)]
mod snap_tests {
    use super::{next_screen, Rect, Span};

    #[test]
    fn spans_are_read_off_the_frame() {
        // A 1440-wide area: halves of 720; a 1441-wide one: 720 and 721.
        assert_eq!(Rect::span(0.0, 1440.0, 0.0, 720.0), Some(Span::Start));
        assert_eq!(Rect::span(0.0, 1440.0, 720.0, 720.0), Some(Span::End));
        assert_eq!(Rect::span(0.0, 1441.0, 720.0, 721.0), Some(Span::End));
        assert_eq!(Rect::span(0.0, 1440.0, 0.0, 1440.0), Some(Span::Whole));
        assert_eq!(Rect::span(0.0, 1440.0, 3.0, 1437.0), Some(Span::Whole));
        // A frame that held its minimum against a half is free.
        assert_eq!(Rect::span(0.0, 1440.0, 0.0, 760.0), None);
        assert_eq!(Rect::span(100.0, 1440.0, 0.0, 720.0), None);
        assert_eq!(Rect::place(25.0, 1441.0, Span::End), (745.0, 721.0));
    }

    #[test]
    fn the_screens_are_walked_in_order() {
        let main = Rect {
            x: 0.0,
            y: 0.0,
            w: 1440.0,
            h: 875.0,
        };
        let left = Rect {
            x: -1920.0,
            y: -100.0,
            w: 1920.0,
            h: 1080.0,
        };
        let above = Rect {
            x: 200.0,
            y: -1080.0,
            w: 1920.0,
            h: 1080.0,
        };
        let all = [main, left, above];
        // By the left edge, then the top: left, main, above.
        assert_eq!(next_screen(&left, &all, 1), Some(main));
        assert_eq!(next_screen(&main, &all, 1), Some(above));
        assert_eq!(next_screen(&above, &all, 1), Some(left));
        assert_eq!(next_screen(&left, &all, -1), Some(above));
        assert_eq!(next_screen(&main, &[main], 1), None);
        assert_eq!(next_screen(&main, &[main, left], -1), Some(left));
        assert_eq!(next_screen(&main, &[main, left], 1), Some(left));
    }
}
