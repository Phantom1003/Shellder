pub mod commands;
pub mod files;
pub mod model;
pub mod terminals;
pub mod tray;
pub mod updater;
pub mod windows;

use std::sync::Arc;

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin({
            // The window positions and sizes. Not whether a window was
            // visible: restored, that shows the window as soon as it is
            // built, before its page is drawn, and `windows::present`
            // decides when a window comes on screen. Nor whether it had
            // decorations: a Console loses them in its immersive mode,
            // which the page keeps per host while the plugin keeps the
            // decorations per window label, and a label last closed
            // immersive (switched to another host, or a second window
            // of the host) came back without its title bar under a page
            // drawing its bars; the page asks for the title bar to go
            // when it restores the mode (`console_immersive`). An isolated copy
            // (SHELLDER_STATE_DIR) keeps its own file, in its state dir,
            // instead of the user's.
            use tauri_plugin_window_state::StateFlags;
            let mut b = tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE - StateFlags::DECORATIONS)
                .with_denylist(&[windows::PROMPT, windows::QUICK]);
            if crate::core::config::get().state_dir_overridden {
                b = b.with_filename(crate::core::config::path_str(
                    &crate::core::config::get()
                        .state_dir
                        .join("window-state.json"),
                ));
            }
            b.build()
        })
        .manage(model.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_enabled,
            commands::set_locked,
            commands::host_action,
            commands::set_idle_mode,
            commands::set_host_reconnect,
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
            commands::window_ready,
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
            commands::files_read,
            commands::files_copy_selection,
            commands::files_transfer,
            commands::console_snapshot,
            commands::console_forward,
            commands::console_cancel_forward,
            commands::console_forwards,
            commands::console_rename,
            commands::console_kill,
            commands::console_disk,
            commands::console_swap,
            commands::console_signal,
            commands::console_tree,
            commands::console_switch,
            commands::term_open,
            commands::term_write,
            commands::term_type_password,
            commands::term_resize,
            commands::term_close,
            commands::term_reset,
            commands::notify,
            commands::quick_hide,
            commands::console_immersive,
            commands::window_snap,
            commands::window_screen,
            commands::terminal_themes,
            commands::system_fonts,
            commands::files_exists,
            commands::themes_dir,
            commands::theme_write,
            commands::theme_delete,
            commands::theme_rename,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            // Start as an accessory (no Dock tile); a window on screen
            // switches to a regular app.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            #[cfg(target_os = "macos")]
            disable_press_and_hold();

            let h = handle.clone();
            let mm = m.clone();
            let _ = m.on_prompt.set(Box::new(move |pending| {
                let h2 = h.clone();
                let mm = mm.clone();
                let _ = h.run_on_main_thread(move || windows::present_prompt(&h2, &mm, pending));
            }));
            let h = handle.clone();
            let mm = m.clone();
            let _ = m.on_settings_changed.set(Box::new(move |what| {
                let h2 = h.clone();
                let mm = mm.clone();
                let _ = h.run_on_main_thread(move || match what {
                    model::Applies::MenuBar => apply_menu_bar_setting(&h2, &mm),
                    model::Applies::QuickTerminal => apply_quick_terminal(&h2, &mm),
                    model::Applies::TerminalEffects => windows::apply_terminal_effects(&h2),
                    model::Applies::Pages => {}
                });
            }));

            // The tray and the Files windows follow every state the pages
            // get, the same value. A pane can show any host: one with a
            // master up through it, any other through a session of its
            // own; the list goes to them when it changed (a Files window
            // opened later is given it then, `files_open`).
            let h = handle.clone();
            let shown_hosts = std::sync::Mutex::new(Vec::<String>::new());
            let _ = m.on_published.set(Box::new(move |snap| {
                let changed = {
                    let mut shown = shown_hosts.lock().unwrap_or_else(|e| e.into_inner());
                    let same = shown.len() == snap.hosts.len()
                        && shown.iter().zip(&snap.hosts).all(|(a, h)| *a == h.alias);
                    if !same {
                        *shown = snap.hosts.iter().map(|h| h.alias.clone()).collect();
                    }
                    (!same).then(|| shown.clone())
                };
                if let Some(hosts) = changed {
                    for f in files::all() {
                        f.set_hosts(hosts.clone());
                    }
                }
                if h.tray_by_id(tray::ID).is_some() {
                    let h2 = h.clone();
                    let _ = h.run_on_main_thread(move || tray::refresh(&h2, &snap));
                }
            }));
            m.start(handle.clone());
            apply_menu_bar_setting(&handle, &m);
            apply_quick_terminal(&handle, &m);

            updater::schedule(&handle, &m);
            watch_sigterm(&handle, &m);

            // "Start silently" applies to the login item only: a double
            // click always opens the window. It also needs "Run in
            // background", as an app that quits with its window must show one.
            let p = prefs::load();
            if !background && !(login_item && p.silent_launch && p.keep_in_background) {
                windows::show_main(&handle);
            }
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
                // Activation (a double click on the bundle, the Dock) brings a window back.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => windows::reopen(app),
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

/// A held key repeats in a terminal; macOS would show the accent popup
/// instead (press and hold). Off for this app, as every terminal
/// application has it: the application's own default, set before the
/// first key is pressed.
#[cfg(target_os = "macos")]
fn disable_press_and_hold() {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    use core_foundation_sys::preferences::{
        kCFPreferencesCurrentApplication, CFPreferencesAppSynchronize, CFPreferencesSetAppValue,
    };
    let key = CFString::new("ApplePressAndHoldEnabled");
    unsafe {
        CFPreferencesSetAppValue(
            key.as_concrete_TypeRef(),
            CFBoolean::false_value().as_CFTypeRef(),
            kCFPreferencesCurrentApplication,
        );
        CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication);
    }
}

/// The quick terminal's key, as Settings have it now: registered with the
/// system, the one before let go. Pressed, it drops the quick terminal
/// (a shell on the host chosen) from the top of the screen, over any
/// application, and takes it away again (`windows::toggle_quick_terminal`).
/// A key the system will not register (another application has it, or
/// it is not a key) is said in Settings (`quick_key_error`).
fn apply_quick_terminal(app: &tauri::AppHandle, model: &Arc<model::Model>) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
    static APPLIED: std::sync::Mutex<Option<(Option<String>, Option<String>)>> =
        std::sync::Mutex::new(None);
    let p = prefs::load();
    let wanted = (p.quick_console_key.clone(), p.quick_console_host.clone());
    {
        // The key is registered again only when it, or its host, changed
        // (a key chosen again is the same key).
        let mut applied = APPLIED.lock().unwrap_or_else(|e| e.into_inner());
        if applied.as_ref() == Some(&wanted) {
            return;
        }
        *applied = Some(wanted);
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let Some(key) = p.quick_console_key.as_deref().filter(|k| !k.is_empty()) else {
        model.set_quick_key_error(None);
        return;
    };
    let shortcut: Shortcut = match key.parse() {
        Ok(s) => s,
        Err(e) => {
            log::warn(format!("quick terminal key {key:?} not understood: {e}"));
            model.set_quick_key_error(Some(e.to_string()));
            return;
        }
    };
    let host = p
        .quick_console_host
        .clone()
        .unwrap_or_else(|| crate::core::ssh::LOCAL.to_string());
    let r = gs.on_shortcut(shortcut, move |app, _, event| {
        if event.state() == ShortcutState::Pressed {
            let a = app.clone();
            let h = host.clone();
            let _ = app.run_on_main_thread(move || windows::toggle_quick_terminal(&a, &h));
        }
    });
    match r {
        Ok(()) => {
            log::info(format!("quick terminal on {key}"));
            model.set_quick_key_error(None);
        }
        Err(e) => {
            log::warn(format!("quick terminal key {key} not registered: {e}"));
            model.set_quick_key_error(Some(e.to_string()));
        }
    }
}

/// The write end of the pipe SIGTERM is told through, -1 before
/// `watch_sigterm` made it.
static SIGTERM_PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// A signal handler may do next to nothing: a byte into the pipe, which
/// the thread waiting on the other end reads, and errno left as the
/// interrupted thread had it.
extern "C" fn on_sigterm(_: libc::c_int) {
    #[cfg(target_os = "macos")]
    let errno = unsafe { libc::__error() };
    #[cfg(not(target_os = "macos"))]
    let errno = unsafe { libc::__errno_location() };
    let saved = unsafe { *errno };
    let fd = SIGTERM_PIPE.load(std::sync::atomic::Ordering::SeqCst);
    if fd >= 0 {
        let byte = 1u8;
        unsafe { libc::write(fd, &byte as *const u8 as *const libc::c_void, 1) };
    }
    unsafe { *errno = saved };
}

/// `launchctl bootout` and logout send SIGTERM: shut the masters down
/// cleanly instead of leaving them to the kernel. The handler writes to a
/// pipe and a thread blocked on reading it does the quitting, so nothing
/// wakes up to look while no signal comes.
fn watch_sigterm(app: &tauri::AppHandle, model: &Arc<model::Model>) {
    let mut fds = [-1 as libc::c_int; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        log::warn(format!(
            "no pipe for SIGTERM ({}): it ends the app without stopping the masters",
            std::io::Error::last_os_error()
        ));
        return;
    }
    // Not for the processes the app starts.
    for fd in fds {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    SIGTERM_PIPE.store(fds[1], std::sync::atomic::Ordering::SeqCst);
    unsafe { libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t) };
    let app = app.clone();
    let model = model.clone();
    std::thread::Builder::new()
        .name("shellder-sigterm".into())
        .spawn(move || loop {
            let mut byte = 0u8;
            let n = unsafe { libc::read(fds[0], &mut byte as *mut u8 as *mut libc::c_void, 1) };
            if n == 1 {
                log::info("SIGTERM received, quitting");
                windows::quit(&model, &app);
            } else if n < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            } else {
                break;
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
