pub mod commands;
pub mod files;
pub mod model;
pub mod terminals;
pub mod tray;
pub mod updater;
pub mod windows;

use std::sync::Arc;

use tauri::Listener;

use crate::core::autostart;
use crate::core::log;
use crate::core::prefs;

/// Restart through Tauri: the new process is spawned before this one has
/// exited, so it is marked (`config::RELAUNCH_ENV`) as the one launch that
/// may wait for the instance lock instead of giving up at once.
pub fn restart(app: &tauri::AppHandle) -> ! {
    std::env::set_var(crate::core::config::RELAUNCH_ENV, "1");
    app.restart()
}

/// Start the GUI. `background`: no window for this launch. `login_item`:
/// a launch by the login item, the only one that honours "start silently".
pub fn run(background: bool, login_item: bool) {
    let model = model::Model::new();
    let m = model.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_denylist(&[windows::PROMPT])
                .build(),
        )
        .manage(model.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_enabled,
            commands::set_locked,
            commands::host_action,
            commands::set_idle_mode,
            commands::reload_config,
            commands::set_secret,
            commands::remove_secret,
            commands::remove_all_secrets,
            commands::reveal_secret,
            commands::totp_code,
            commands::request_keychain_access,
            commands::answer_prompt,
            commands::set_setting,
            commands::log_tail,
            commands::open_window,
            commands::open_path,
            commands::open_url,
            commands::open_shell,
            commands::check_update,
            commands::install_update,
            commands::relaunch,
            commands::quit,
            commands::abbreviate_home,
            commands::files_open,
            commands::files_state,
            commands::files_set_source,
            commands::files_set_root,
            commands::files_subdirectories,
            commands::files_nav,
            commands::files_set_sort,
            commands::files_select,
            commands::files_target,
            commands::files_create,
            commands::files_delete,
            commands::files_drop,
            commands::files_drop_local,
            commands::files_copy_selection,
            commands::files_transfer,
            commands::console_snapshot,
            commands::console_open_vnc,
            commands::term_open,
            commands::term_write,
            commands::term_resize,
            commands::term_close,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            // Start as an accessory (no Dock tile); a window on screen
            // switches to a regular app.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let h = handle.clone();
            let _ = m.on_prompt.set(Box::new(move |pending| {
                let h2 = h.clone();
                let _ = h.run_on_main_thread(move || windows::present_prompt(&h2, pending));
            }));
            let h = handle.clone();
            let mm = m.clone();
            let _ = m.on_settings_changed.set(Box::new(move || {
                let h2 = h.clone();
                let mm = mm.clone();
                let _ = h.run_on_main_thread(move || apply_menu_bar_setting(&h2, &mm));
            }));
            m.start(handle.clone());
            apply_menu_bar_setting(&handle, &m);

            // The tray and the Files windows follow every change. A pane
            // can show any host: one with a master up through it, any
            // other through a session of its own.
            let h = handle.clone();
            let mm = m.clone();
            handle.listen("state", move |_| {
                let h2 = h.clone();
                let m3 = mm.clone();
                let _ = h.run_on_main_thread(move || tray::refresh(&h2, &m3));
                let hosts: Vec<String> = mm
                    .snapshot()
                    .hosts
                    .iter()
                    .map(|h| h.alias.clone())
                    .collect();
                for f in files::all() {
                    f.set_hosts(hosts.clone());
                }
            });

            updater::schedule(&handle, &m);
            watch_sigterm(&handle, &m);

            // "Start silently" applies to the login item only: a double
            // click always opens the window. It also needs "Run in
            // background", as an app that quits with its window must show one.
            let p = prefs::load();
            if !background && !(login_item && p.silent_launch && p.keep_in_background) {
                windows::show_main(&handle);
            }
            let _ = autostart::installed(); // keep the module linked
            Ok(())
        })
        .on_window_event({
            let model = model.clone();
            move |window, event| windows::on_window_event(&model, window, event)
        })
        .build(tauri::generate_context!())
        .expect("error while building the application")
        .run({
            let model = model.clone();
            move |app, event| match event {
                // Activation (a double click on the bundle, the Dock) opens the window.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => windows::show_main(app),
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    // Closing the last window never exits by itself; quit() does.
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => {
                    model.shutdown();
                    log::info("daemon stopped");
                }
                _ => {}
            }
        });
}

static SIGTERM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_sigterm(_: libc::c_int) {
    SIGTERM.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// `launchctl bootout` and logout send SIGTERM: shut the masters down
/// cleanly instead of leaving them to the kernel.
fn watch_sigterm(app: &tauri::AppHandle, model: &Arc<model::Model>) {
    unsafe { libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t) };
    let app = app.clone();
    let model = model.clone();
    std::thread::Builder::new()
        .name("shellder-sigterm".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if SIGTERM.swap(false, std::sync::atomic::Ordering::SeqCst) {
                log::info("SIGTERM received, quitting");
                windows::quit(&model, &app);
            }
        })
        .expect("sigterm thread");
}

fn apply_menu_bar_setting(app: &tauri::AppHandle, model: &Arc<model::Model>) {
    let p = prefs::load();
    if p.keep_in_background && p.show_menu_bar_icon() {
        if let Err(e) = tray::install(app, model) {
            log::warn(format!("tray: {e}"));
        }
    } else {
        tray::remove(app);
    }
}
