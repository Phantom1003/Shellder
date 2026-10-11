//! Menu bar item: connection overview and quick actions. A left click
//! toggles the window, the menu is on the right button.

use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use super::model::{Model, Snapshot};
use super::{updater, windows};
use crate::core::daemon::HostState;
use crate::core::prefs;

pub const ID: &str = "shellder-tray";

static LAST_SIGNATURE: Mutex<String> = Mutex::new(String::new());

/// The menu bar picture: the emblem while a master is up, a faded copy of
/// it while none is. On macOS it is a template image, so the system tints
/// it like its own status icons and the fade reads as grey. A Linux panel
/// shows the picture as it is, dark; `inverted` turns it light for a dark
/// panel. Errors and prompts do not show here: the window and the prompt
/// window carry those.
fn icon(connected: bool, inverted: bool) -> Image<'static> {
    let full = Image::from_bytes(include_bytes!("../../icons/tray.png")).expect("tray icon");
    if connected && !inverted {
        return full;
    }
    let (w, h) = (full.width(), full.height());
    let mut rgba = full.rgba().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        if inverted {
            for c in &mut px[..3] {
                *c = 255 - *c;
            }
        }
        if !connected {
            px[3] = (u32::from(px[3]) * 2 / 5) as u8;
        }
    }
    Image::new_owned(rgba, w, h)
}

pub fn install(app: &AppHandle, model: &Arc<Model>) -> tauri::Result<()> {
    if app.tray_by_id(ID).is_some() {
        return Ok(());
    }
    let m = model.clone();
    let m2 = model.clone();
    TrayIconBuilder::with_id(ID)
        .icon(icon(false, prefs::load().invert_menu_bar_icon))
        .icon_as_template(true)
        .tooltip("Shellder")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::toggle_main(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| handle(app, &m, event.id().as_ref()))
        .build(app)?;
    *LAST_SIGNATURE.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
    refresh(app, &m2);
    Ok(())
}

pub fn remove(app: &AppHandle) {
    if let Some(t) = app.tray_by_id(ID) {
        let _ = t.set_visible(false);
        drop(t);
    }
    let _ = app.remove_tray_by_id(ID);
}

fn handle(app: &AppHandle, model: &Arc<Model>, id: &str) {
    let model = model.clone();
    let app = app.clone();
    let id = id.to_string();
    // Menu events arrive on the main thread; the actions may wait on the
    // daemon or the keychain, so run them elsewhere.
    std::thread::spawn(move || match id.as_str() {
        "open" => app
            .run_on_main_thread({
                let app = app.clone();
                move || windows::show_main(&app)
            })
            .unwrap_or(()),
        "settings" | "update" => app
            .run_on_main_thread({
                let app = app.clone();
                move || windows::show_settings(&app)
            })
            .unwrap_or(()),
        "connect-all" => model.connect_all(),
        "disconnect-all" => model.disconnect_all(),
        "quit" => windows::quit(&model, &app),
        other => {
            let Some(rest) = other.strip_prefix("host:") else {
                return;
            };
            let Some((alias, action)) = rest.rsplit_once(':') else {
                return;
            };
            match action {
                "lock" => model.set_locked(alias, true),
                "unlock" => model.set_locked(alias, false),
                "disconnect" => model.disconnect(alias),
                "reconnect" | "takeover" | "retry" => model.reconnect(alias),
                "connect" => model.connect(alias),
                "close" => model.close_socket(alias),
                "show" => {
                    let alias = alias.to_string();
                    let a = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        windows::show_main(&a);
                        use tauri::Emitter;
                        let _ = a.emit("select-host", alias);
                    });
                }
                _ => {}
            }
        }
    });
}

/// The same controls as the host page's header, for the same state.
fn host_controls(snap: &Snapshot, alias: &str) -> Vec<(String, String)> {
    let status = snap.statuses.get(alias);
    let state = status.map(|s| s.state.clone()).unwrap_or(HostState::Off);
    let locked = snap.locked.iter().any(|h| h == alias);
    let enabled = snap.enabled.iter().any(|h| h == alias);
    let manageable = snap
        .resolved
        .get(alias)
        .map(|r| r.control_path.is_some())
        .unwrap_or(false);
    let mut out = vec![(
        if locked { "unlock" } else { "lock" }.to_string(),
        if locked { "Unlock" } else { "Lock" }.to_string(),
    )];
    if state.is_running() {
        out.push(("disconnect".into(), "Disconnect".into()));
        out.push(("reconnect".into(), "Reconnect".into()));
    } else if state == HostState::Foreign {
        out.push(("takeover".into(), "Take over".into()));
    } else if enabled {
        out.push(("retry".into(), "Retry now".into()));
    } else if manageable {
        out.push(("connect".into(), "Connect".into()));
    }
    if state.is_up() {
        out.push(("close".into(), "Close socket".into()));
    }
    out
}

fn signature(snap: &Snapshot) -> String {
    let mut s = String::new();
    for h in &snap.hosts {
        let st = snap
            .statuses
            .get(&h.alias)
            .map(|s| format!("{:?}", s.state))
            .unwrap_or_default();
        s.push_str(&format!(
            "{}|{}|{}|{};",
            h.alias,
            st,
            snap.locked.contains(&h.alias),
            snap.enabled.contains(&h.alias)
        ));
    }
    s.push_str(&format!(
        "update={:?};inverted={}",
        snap.update.version, snap.settings.invert_menu_bar_icon
    ));
    s
}

/// Rebuild the menu and the icon from the model. Called on the main thread
/// after every change; the menu is only rebuilt when something in it
/// changed.
pub fn refresh(app: &AppHandle, model: &Arc<Model>) {
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    let snap = model.snapshot();
    let kept: Vec<&String> = snap
        .hosts
        .iter()
        .map(|h| &h.alias)
        .filter(|a| {
            snap.enabled.contains(a)
                || snap
                    .statuses
                    .get(*a)
                    .map(|s| !s.needed_by.is_empty())
                    .unwrap_or(false)
        })
        .collect();
    let up = kept
        .iter()
        .filter(|a| {
            snap.statuses
                .get(**a)
                .map(|s| s.state.is_up())
                .unwrap_or(false)
        })
        .count();
    let tooltip = if kept.is_empty() {
        "Shellder — nothing switched on".to_string()
    } else {
        format!("Shellder — {up}/{} masters up", kept.len())
    };
    let _ = tray.set_tooltip(Some(tooltip));
    let sig = signature(&snap);
    {
        let mut last = LAST_SIGNATURE.lock().unwrap_or_else(|e| e.into_inner());
        if *last == sig {
            return;
        }
        *last = sig;
    }
    let connected = snap.statuses.values().any(|s| s.state.is_up());
    let _ = tray.set_icon(Some(icon(connected, snap.settings.invert_menu_bar_icon)));
    let _ = tray.set_icon_as_template(true);
    match build_menu(app, &snap) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => crate::core::log::warn(format!("tray menu: {e}")),
    }
}

fn build_menu(app: &AppHandle, snap: &Snapshot) -> tauri::Result<Menu<tauri::Wry>> {
    let mut b = MenuBuilder::new(app);
    b = b.item(&MenuItem::with_id(
        app,
        "open",
        "Open Shellder",
        true,
        None::<&str>,
    )?);
    b = b.item(&PredefinedMenuItem::separator(app)?);
    for h in &snap.hosts {
        let state = snap
            .statuses
            .get(&h.alias)
            .map(|s| s.state.clone())
            .unwrap_or(HostState::Off);
        let dot = match state {
            HostState::Up { .. } => "●",
            HostState::Foreign => "◐",
            _ => "○",
        };
        let sub = Submenu::with_id(
            app,
            format!("host:{}", h.alias),
            format!("{dot}  {}", h.alias),
            true,
        )?;
        for (action, title) in host_controls(snap, &h.alias) {
            sub.append(&MenuItem::with_id(
                app,
                format!("host:{}:{action}", h.alias),
                title,
                true,
                None::<&str>,
            )?)?;
        }
        sub.append(&PredefinedMenuItem::separator(app)?)?;
        sub.append(&MenuItem::with_id(
            app,
            format!("host:{}:show", h.alias),
            "Show in Shellder",
            true,
            None::<&str>,
        )?)?;
        b = b.item(&sub);
    }
    if snap.hosts.is_empty() {
        b = b.item(&MenuItem::with_id(
            app,
            "none",
            "No Host entries found in ~/.ssh/config",
            false,
            None::<&str>,
        )?);
    }
    b = b.item(&PredefinedMenuItem::separator(app)?);
    b = b.item(&MenuItem::with_id(
        app,
        "connect-all",
        "Connect All",
        true,
        None::<&str>,
    )?);
    b = b.item(&MenuItem::with_id(
        app,
        "disconnect-all",
        "Disconnect All",
        true,
        None::<&str>,
    )?);
    b = b.item(&PredefinedMenuItem::separator(app)?);
    if snap.update.state == "available" {
        if let Some(v) = &snap.update.version {
            b = b.item(&MenuItem::with_id(
                app,
                "update",
                format!("Update to {v}…"),
                true,
                None::<&str>,
            )?);
        }
    }
    b = b.item(&MenuItem::with_id(
        app,
        "settings",
        "Settings",
        true,
        None::<&str>,
    )?);
    b = b.item(&MenuItem::with_id(
        app,
        "quit",
        "Quit Shellder",
        true,
        Some("CmdOrCtrl+Q"),
    )?);
    let _ = updater::noop;
    b.build()
}
