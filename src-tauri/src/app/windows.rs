//! The windows: one main window, Settings, the floating prompt window, and
//! one Files window and one Console window per host. Also what closing
//! the last one means (background or quit, per Settings) and, on macOS,
//! whether the app has a Dock tile.

use std::sync::Arc;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use super::model::Model;
use super::terminals;
use crate::core::log;
use crate::core::prefs;

pub const MAIN: &str = "main";
pub const SETTINGS: &str = "settings";
pub const PROMPT: &str = "prompt";

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

fn any_visible(app: &AppHandle, except: Option<&str>) -> bool {
    own_windows(app).iter().any(|w| {
        Some(w.label()) != except
            && w.is_visible().unwrap_or(false)
            && !w.is_minimized().unwrap_or(false)
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

/// Bring a window on screen: first restore the Dock tile (so the app can
/// become active), then order the window front.
fn present(app: &AppHandle, w: &WebviewWindow) {
    apply_activation_policy(app, true);
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
}

fn builder<'a>(
    app: &'a AppHandle,
    label: &str,
    page: &str,
    title: &str,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let b = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .visible(false);
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
        None => match builder(app, MAIN, "index.html", "Shellder")
            .inner_size(900.0, 560.0)
            .min_inner_size(720.0, 420.0)
            .center()
            .build()
        {
            Ok(w) => w,
            Err(e) => {
                log::error(format!("cannot open the main window: {e}"));
                return;
            }
        },
    };
    present(app, &w);
}

/// The menu bar icon: bring the window up, or put it away when it is
/// already in front. Away means what closing it means (background or
/// quit, per Settings), except that a click never quits: it hides.
pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
            if !any_visible(app, None) {
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
        None => match builder(app, SETTINGS, "settings.html", "Shellder Settings")
            .inner_size(560.0, 640.0)
            .min_inner_size(480.0, 320.0)
            .center()
            .build()
        {
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
            match builder(app, &label, &page, &format!("Files — {host}"))
                .inner_size(880.0, 540.0)
                .min_inner_size(800.0, 380.0)
                .center()
                .build()
            {
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
    let label = console_label(host);
    let w = match app.get_webview_window(&label) {
        Some(w) => w,
        None => {
            let page = format!("console.html?host={}", urlencode(host));
            match builder(app, &label, &page, &format!("Console — {host}"))
                .inner_size(1000.0, 620.0)
                .min_inner_size(760.0, 420.0)
                .center()
                .build()
            {
                Ok(w) => w,
                Err(e) => {
                    log::error(format!("{host}: cannot open the Console window: {e}"));
                    return;
                }
            }
        }
    };
    present(app, &w);
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
        if let Some(w) = app.get_webview_window(PROMPT) {
            let _ = w.hide();
            if !any_visible(app, None) {
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
                .visible(false)
                .center();
            match b.build() {
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
    if let WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        let label = window.label().to_string();
        if label == PROMPT {
            api.prevent_close();
            return;
        }
        if label.starts_with("console-") {
            terminals::close(&label);
        }
        let last = !any_visible(app, Some(&label));
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
