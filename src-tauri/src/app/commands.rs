//! What the windows may ask of the app. Anything that may wait (ssh, the
//! keychain, a listing) is async so it runs off the main thread.

use std::sync::Arc;

use tauri::{AppHandle, State};

use super::files::{self, FileSort, Pane, ReplaceAnswer};
use super::model::{self, Model, PromptResponse, Snapshot};
use super::{terminals, updater, windows};
use crate::core::config;
use crate::core::keychain::SecretKind;
use crate::core::reconnect;
use crate::core::ssh::{FileSource, IdleMode};
use crate::core::{forward, monitor};

type M<'a> = State<'a, Arc<Model>>;

fn kind(k: &str) -> Result<SecretKind, String> {
    SecretKind::parse(k).ok_or_else(|| format!("unknown secret kind {k}"))
}

fn pane(p: &str) -> Result<Pane, String> {
    match p {
        "left" => Ok(Pane::Left),
        "right" => Ok(Pane::Right),
        _ => Err(format!("unknown pane {p}")),
    }
}

#[tauri::command]
pub async fn get_state(model: M<'_>) -> Result<Snapshot, String> {
    Ok(model.snapshot())
}

#[tauri::command]
pub async fn set_enabled(model: M<'_>, host: String, on: bool) -> Result<(), String> {
    model.set_enabled(&host, on);
    Ok(())
}

#[tauri::command]
pub async fn set_locked(model: M<'_>, host: String, on: bool) -> Result<(), String> {
    model.set_locked(&host, on);
    Ok(())
}

#[tauri::command]
pub async fn host_action(model: M<'_>, host: String, action: String) -> Result<(), String> {
    match action.as_str() {
        "connect" => model.connect(&host),
        "disconnect" => model.disconnect(&host),
        "reconnect" | "retry" | "takeover" => model.reconnect(&host),
        "close" => model.close_socket(&host),
        "connect-all" => model.connect_all(),
        "disconnect-all" => model.disconnect_all(),
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}

#[tauri::command]
pub async fn set_idle_mode(model: M<'_>, host: String, mode: String) -> Result<(), String> {
    let m = IdleMode::parse(&mode).ok_or_else(|| format!("unknown mode {mode}"))?;
    model.set_idle_mode(&host, m);
    Ok(())
}

#[tauri::command]
pub async fn set_host_reconnect(
    model: M<'_>,
    host: String,
    rule: Option<reconnect::Partial>,
) -> Result<(), String> {
    model.set_host_reconnect(&host, rule);
    Ok(())
}

#[tauri::command]
pub async fn reload_config(model: M<'_>) -> Result<(), String> {
    model.reload_catalog(true);
    Ok(())
}

/// Run `f` with the model on a thread that may wait: the keychain may
/// keep a call for as long as its dialog is up, and the async workers are
/// what every window's calls run on.
async fn blocking<R: Send + 'static>(
    model: &M<'_>,
    f: impl FnOnce(&Model) -> R + Send + 'static,
) -> Result<R, String> {
    let m = model.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(&m))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_secret(
    model: M<'_>,
    host: String,
    kind: String,
    value: String,
) -> Result<(), String> {
    let kind = self::kind(&kind)?;
    blocking(&model, move |m| m.set_secret(&host, kind, &value)).await?
}

#[tauri::command]
pub async fn remove_secret(model: M<'_>, host: String, kind: String) -> Result<(), String> {
    let kind = self::kind(&kind)?;
    blocking(&model, move |m| m.remove_secret(&host, kind)).await
}

#[tauri::command]
pub async fn remove_all_secrets(model: M<'_>, host: String) -> Result<(), String> {
    blocking(&model, move |m| {
        for k in SecretKind::ALL {
            if m.has_secret(&host, k) {
                m.remove_secret(&host, k);
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn reveal_secret(
    model: M<'_>,
    host: String,
    kind: String,
) -> Result<Option<String>, String> {
    let kind = self::kind(&kind)?;
    blocking(&model, move |m| m.reveal_secret(&host, kind)).await
}

#[derive(serde::Serialize)]
pub struct TotpCode {
    code: String,
    remaining: u64,
}

#[tauri::command]
pub async fn totp_code(model: M<'_>, host: String) -> Result<Option<TotpCode>, String> {
    blocking(&model, move |m| {
        m.totp_code(&host)
            .map(|(code, remaining)| TotpCode { code, remaining })
    })
    .await
}

#[tauri::command]
pub async fn request_keychain_access(model: M<'_>) -> Result<(), String> {
    blocking(&model, |m| m.request_keychain_access()).await
}

#[tauri::command]
pub async fn answer_prompt(
    model: M<'_>,
    id: u64,
    answer: Option<String>,
    save: bool,
) -> Result<(), String> {
    model.answer_prompt(id, PromptResponse { answer, save });
    Ok(())
}

#[tauri::command]
pub async fn set_setting(
    model: M<'_>,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    model.set_setting(&key, value)
}

/// The log panel's lines: the last ones (`after` null), or those written
/// since the `offset` the last call gave.
#[tauri::command]
pub async fn log_tail(after: Option<u64>) -> Result<model::LogTail, String> {
    Ok(model::log_tail(after))
}

/// The page has drawn itself: the window, built hidden, may come on screen.
#[tauri::command]
pub async fn window_ready(app: AppHandle, window: tauri::Window) -> Result<(), String> {
    let label = window.label().to_string();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || windows::page_ready(&a, &label));
    Ok(())
}

#[tauri::command]
pub async fn open_window(
    app: AppHandle,
    which: String,
    host: Option<String>,
    fresh: Option<bool>,
) -> Result<(), String> {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || match which.as_str() {
        "main" => windows::show_main(&a),
        "settings" => windows::show_settings(&a),
        "termprefs" => windows::show_console_settings(&a),
        "files" => {
            if let Some(h) = host {
                windows::show_files(&a, &h)
            }
        }
        // `fresh`: another Console window on the host (⌘N), beside the
        // one it has.
        "console" => {
            if let Some(h) = host {
                if fresh.unwrap_or(false) {
                    windows::new_console(&a, &h)
                } else {
                    windows::show_console(&a, &h)
                }
            }
        }
        _ => {}
    });
    Ok(())
}

// MARK: console windows

/// The Console window shows another host from now on: the page is told
/// (`console-host`) and its column reads that host, while the tabs open
/// stay, each on the host it runs on. A window already showing the host
/// is brought up instead.
#[tauri::command]
pub async fn console_switch(
    app: AppHandle,
    window: tauri::Window,
    host: String,
) -> Result<(), String> {
    let label = window.label().to_string();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || windows::switch_console(&a, &label, &host));
    Ok(())
}

/// What the host is doing right now: the page polls this while it is
/// open, naming the parts of the Status tab it shows (`monitor::PARTS`)
/// and the lists of the tabs it needs (`monitor::TAB_PARTS`); what is not
/// named is not read, which spares the host its commands.
#[tauri::command]
pub async fn console_snapshot(
    host: String,
    parts: Vec<String>,
) -> Result<monitor::Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || monitor::fetch(&host, &parts))
        .await
        .map_err(|e| e.to_string())?
}

/// A port forward on the host's master, from the Console's Ports tab:
/// `direction` is "local" (a port here reaches one on the host) or
/// "remote" (the other way); a port left out is picked, here a free one,
/// on the host one sshd allocates.
#[tauri::command]
pub async fn console_forward(
    host: String,
    direction: forward::Direction,
    local: Option<u16>,
    remote: Option<u16>,
) -> Result<forward::Forward, String> {
    tauri::async_runtime::spawn_blocking(move || {
        forward::establish(&host, direction, local, remote)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn console_cancel_forward(host: String, forward: forward::Forward) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || forward::cancel(&host, forward))
        .await
        .map_err(|e| e.to_string())?
}

/// The forwards still up on the host's master.
#[tauri::command]
pub async fn console_forwards(host: String) -> Result<Vec<forward::Forward>, String> {
    tauri::async_runtime::spawn_blocking(move || forward::active(&host))
        .await
        .map_err(|e| e.to_string())
}

/// What a command that failed on the host said, or its exit status when
/// it said nothing.
fn failure(said: String, status: i32) -> String {
    if said.is_empty() {
        format!("exited with status {status}")
    } else {
        said
    }
}

/// Give a tmux or screen session another name, through the master or the
/// session like the readings.
#[tauri::command]
pub async fn console_rename(
    host: String,
    kind: String,
    id: String,
    name: String,
) -> Result<(), String> {
    use crate::core::remotefs::{quote, remote};
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("A session needs a name.".into());
    }
    let command = match kind.as_str() {
        "tmux" => format!(
            "tmux rename-session -t {} {}",
            quote(&format!("={id}")),
            quote(&name)
        ),
        "screen" => format!("screen -S {} -X sessionname {}", quote(&id), quote(&name)),
        other => return Err(format!("cannot rename a {other} session")),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let out = remote::shell(&host, &command);
        if out.status != 0 {
            return Err(failure(out.text(), out.status));
        }
        crate::core::log::info(format!("{host}: {kind} session {id} renamed"));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// End a tmux or screen session, and everything in it.
#[tauri::command]
pub async fn console_kill(host: String, kind: String, id: String) -> Result<(), String> {
    use crate::core::remotefs::{quote, remote};
    let command = match kind.as_str() {
        "tmux" => format!("tmux kill-session -t {}", quote(&format!("={id}"))),
        "screen" => format!("screen -S {} -X quit", quote(&id)),
        other => return Err(format!("cannot end a {other} session")),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let out = remote::shell(&host, &command);
        if out.status != 0 {
            return Err(failure(out.text(), out.status));
        }
        crate::core::log::info(format!("{host}: {kind} session {id} ended"));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What something done on the host from the Status tab (a mount, a swap
/// file, a signal) came to.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// What the host said (udisks and diskutil say where a mount went).
    pub said: String,
    /// The host wanted a password for it and the user cancelled the
    /// question: nothing was done, and there is nothing to say.
    pub cancelled: bool,
}

/// Run `command` on the host, through the master or the session, where
/// nothing can stop for a password. A refusal a password would lift
/// (`monitor::wants_password`) is not an error: `asking`, the same
/// command for a terminal, runs on an ssh of its own with the app's
/// asking pty as its terminal (`ssh::run_on_terminal`), so that what the
/// host asks on it (sudo, polkit's text agent) is answered from the
/// vault or by the user in the prompt window, never in a tab. `what`
/// names it in the log, `about` in the window.
async fn run_or_ask(
    model: Arc<Model>,
    host: String,
    command: String,
    asking: String,
    what: String,
    about: String,
) -> Result<Outcome, String> {
    use crate::core::prompt::Origin;
    use crate::core::remotefs::remote;
    use crate::core::ssh;
    tauri::async_runtime::spawn_blocking(move || {
        let out = remote::shell(&host, &command);
        if out.status == 0 {
            crate::core::log::info(format!("{host}: {what}"));
            return Ok(Outcome {
                said: out.stdout.trim().to_string(),
                cancelled: false,
            });
        }
        let why = failure(out.text(), out.status);
        if !monitor::wants_password(&why) {
            return Err(why);
        }
        crate::core::log::info(format!(
            "{host}: {what} wants a password, running it on a terminal of its own: {why}"
        ));
        let prompts = model::ActionPrompts::new(model, about);
        let out = ssh::run_on_terminal(&host, &asking, Origin::Action, prompts.clone());
        if prompts.cancelled() {
            crate::core::log::info(format!("{host}: {what} cancelled by the user"));
            return Ok(Outcome {
                said: String::new(),
                cancelled: true,
            });
        }
        // polkit's text agent frames its question in `==== … ====` lines.
        let said: String = out
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("===="))
            .collect::<Vec<_>>()
            .join("\n");
        if out.status != 0 {
            let why = format!("{}\n{said}", out.stderr).trim().to_string();
            return Err(failure(why, out.status));
        }
        crate::core::log::info(format!("{host}: {what}"));
        Ok(Outcome {
            said,
            cancelled: false,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Mount or unmount a removable volume, or eject its drive, from the
/// Console's Disks card; `device` is a `/dev` path the readings named.
#[tauri::command]
pub async fn console_disk(
    model: M<'_>,
    host: String,
    action: String,
    device: String,
) -> Result<Outcome, String> {
    let command = monitor::disk_command(&action, &device, false)?;
    let asking = monitor::disk_command(&action, &device, true)?;
    let about = match action.as_str() {
        "unmount" => format!("umount {device}"),
        _ => format!("{action} {device}"),
    };
    run_or_ask(
        model.inner().clone(),
        host,
        command,
        asking,
        format!("{action} {device}"),
        about,
    )
    .await
}

/// Add a swap file of `size` MB on the host, until its next boot
/// (`action` `add`), or remove one the Console made (`remove`), from the
/// Swap gauge of the Status tab.
#[tauri::command]
pub async fn console_swap(
    model: M<'_>,
    host: String,
    action: String,
    path: String,
    size: u32,
) -> Result<Outcome, String> {
    let command = monitor::swap_command(&action, &path, size, false)?;
    let asking = monitor::swap_command(&action, &path, size, true)?;
    let (what, about) = match action.as_str() {
        "add" => (
            format!("swap file {path} of {size} MB added"),
            format!("swapon {path}"),
        ),
        _ => (
            format!("swap file {path} removed"),
            format!("swapoff {path}"),
        ),
    };
    run_or_ask(model.inner().clone(), host, command, asking, what, about).await
}

/// Send `signal` (TERM, KILL, INT, HUP, STOP, CONT) to the process `pid`
/// on the host, from the process table of the Status tab: as this
/// account, or through sudo for another user's process.
#[tauri::command]
pub async fn console_signal(
    model: M<'_>,
    host: String,
    pid: u32,
    signal: String,
) -> Result<Outcome, String> {
    let command = monitor::signal_command(pid, &signal, false)?;
    let asking = monitor::signal_command(pid, &signal, true)?;
    run_or_ask(
        model.inner().clone(),
        host,
        command,
        asking,
        format!("SIG{signal} sent to {pid}"),
        format!("kill -{signal} {pid}"),
    )
    .await
}

/// The process tree of the window's active tab, for the Status tab's
/// Processes card: `kind` and `name` are the tab's (a tmux or screen
/// session, a container), anything else roots at the ssh logins of this
/// account (`monitor::TreeRoot`).
#[tauri::command]
pub async fn console_tree(
    host: String,
    kind: String,
    name: String,
) -> Result<monitor::Tree, String> {
    let root = monitor::TreeRoot::new(&kind, &name);
    tauri::async_runtime::spawn_blocking(move || monitor::tree(&host, &root))
        .await
        .map_err(|e| e.to_string())?
}

/// The page has measured its terminal: run `command` on the host in it
/// (a `tmux attach`), or the login shell without one.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn term_open(
    app: AppHandle,
    window: tauri::Window,
    model: M<'_>,
    host: String,
    command: Option<String>,
    dir: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<u64, String> {
    let label = window.label().to_string();
    let m = model.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        terminals::open(
            &app,
            &m,
            &label,
            &host,
            command.as_deref(),
            dir.as_deref(),
            cols,
            rows,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A notification from a Console page: a long command done, a bell, a
/// program's own (OSC 9, OSC 777) in a window that is not in front.
#[tauri::command]
pub async fn notify(app: AppHandle, title: String, body: String) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    // macOS delivers a notification only for a process inside an .app
    // bundle; a `cargo tauri dev` binary posts one that is never shown,
    // and the plugin reports no error. Said once in the log instead.
    #[cfg(target_os = "macos")]
    {
        use std::sync::Once;
        static SAID: Once = Once::new();
        let bundled = std::env::current_exe()
            .map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
            .unwrap_or(true);
        if !bundled {
            SAID.call_once(|| {
                crate::core::log::warn(format!(
                    "notification not shown ({title}): this process is not in an .app bundle, macOS shows none for it; run the built app"
                ))
            });
            return Err("not in an app bundle".into());
        }
    }
    let r = app
        .notification()
        .builder()
        .title(&title)
        .body(&body)
        .show();
    if let Err(e) = &r {
        // macOS shows nothing for a binary outside an .app bundle (a
        // `cargo tauri dev` run): the reason goes to the log.
        crate::core::log::warn(format!("notification not shown ({title}): {e}"));
    }
    r.map_err(|e| e.to_string())
}

/// A theme file of the user's: its name (the file's) and its text, in
/// Ghostty's theme format (`background = …`, `palette = N=#…`), parsed by
/// the page (ui/themes.js).
#[derive(Clone, serde::Serialize)]
pub struct ThemeFile {
    pub name: String,
    pub text: String,
}

fn themes_path() -> std::path::PathBuf {
    crate::core::config::get().state_dir.join("themes")
}

/// The theme files in the state directory's `themes`, by name.
#[tauri::command]
pub async fn terminal_themes() -> Result<Vec<ThemeFile>, String> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(themes_path()) else {
        return Ok(out);
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&p) {
            if text.len() <= 64 * 1024 {
                out.push(ThemeFile {
                    name: name.to_string(),
                    text,
                });
            }
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// The fonts on this machine and the terminals' default among them
/// (`core::fonts`), for the Terminal page's font picker: read off the
/// main thread, since every font file is opened for its name.
#[tauri::command]
pub async fn system_fonts() -> Result<crate::core::fonts::Fonts, String> {
    tauri::async_runtime::spawn_blocking(crate::core::fonts::fonts)
        .await
        .map_err(|e| e.to_string())
}

/// A theme file's name as the page gives it: a file name and nothing
/// else (no path, not hidden, not too long).
fn theme_file(name: &str) -> Result<std::path::PathBuf, String> {
    if name.is_empty()
        || name.len() > 120
        || name.starts_with('.')
        || name.contains(['/', '\\', '\0'])
    {
        return Err(format!("not a theme file name: {name:?}"));
    }
    Ok(themes_path().join(name))
}

/// Tell every window the theme files changed, so the Consoles read
/// them again and the settings window redraws its list.
fn themes_changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("themes-changed", ());
}

/// Write a theme file (a colour changed in the settings window, a copy
/// of a palette made there), making the folder when needed.
#[tauri::command]
pub async fn theme_write(app: AppHandle, name: String, text: String) -> Result<(), String> {
    if text.len() > 64 * 1024 {
        return Err("theme file too long".into());
    }
    let p = theme_file(&name)?;
    crate::core::config::ensure_dirs();
    std::fs::create_dir_all(themes_path())
        .map_err(|e| format!("{}: {e}", themes_path().display()))?;
    std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    themes_changed(&app);
    Ok(())
}

/// Rename a theme file; a name taken is refused.
#[tauri::command]
pub async fn theme_rename(app: AppHandle, name: String, to: String) -> Result<(), String> {
    let from = theme_file(&name)?;
    let dest = theme_file(&to)?;
    if dest.exists() {
        return Err(format!("there is a theme file named {to} already"));
    }
    std::fs::rename(&from, &dest).map_err(|e| format!("{}: {e}", from.display()))?;
    themes_changed(&app);
    Ok(())
}

/// Delete a theme file.
#[tauri::command]
pub async fn theme_delete(app: AppHandle, name: String) -> Result<(), String> {
    let p = theme_file(&name)?;
    std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    themes_changed(&app);
    Ok(())
}

/// The themes directory, made if it is not there, for the button that
/// opens it.
#[tauri::command]
pub async fn themes_dir() -> Result<String, String> {
    crate::core::config::ensure_dirs();
    let p = themes_path();
    std::fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(crate::core::config::path_str(&p))
}

/// Whether `path` is there on `source` (a host, or this machine): a path
/// clicked in a terminal is looked for on the terminal's host first,
/// then here (ssh's own messages name this machine's files).
#[tauri::command]
pub async fn files_exists(source: FileSource, path: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || crate::core::remotefs::fs::exists(&source, &path))
        .await
        .map_err(|e| e.to_string())
}

/// A Console window's immersive mode (⌘⇧I): the page hides its bars
/// and column; the window loses its title bar and its buttons too (on
/// macOS, where it had them over the page), and gets them back.
#[tauri::command]
pub async fn console_immersive(window: tauri::WebviewWindow, on: bool) -> Result<(), String> {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || windows::set_immersive(&w, on));
    Ok(())
}

/// The window to a half or a quarter of its screen, or the whole of it
/// (the snap-* keys): `h` 'left' or 'right', `v` 'top' or 'bottom',
/// 'full' for the whole axis; an axis not given keeps the half the
/// window is in now, so left then up is the top left quarter.
#[tauri::command]
pub async fn window_snap(
    window: tauri::WebviewWindow,
    h: Option<String>,
    v: Option<String>,
) -> Result<(), String> {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || windows::snap(&w, h.as_deref(), v.as_deref()));
    Ok(())
}

/// The window to the screen before (`step` -1) or after (1) its own,
/// where it was there (the screen-* keys).
#[tauri::command]
pub async fn window_screen(window: tauri::WebviewWindow, step: i32) -> Result<(), String> {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || windows::to_screen(&w, step));
    Ok(())
}

/// The quick terminal's page puts the window away (Esc at the prompt).
#[tauri::command]
pub async fn quick_hide(app: AppHandle) -> Result<(), String> {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || windows::hide_quick(&a));
    Ok(())
}

/// Hang up on the tab's terminal: it was closed (a tmux client detaches,
/// the session lives on).
#[tauri::command]
pub async fn term_close(id: u64) -> Result<(), String> {
    terminals::close(id);
    Ok(())
}

/// The Console page starts: whatever an earlier page of this window
/// opened (one reloaded under it) is hung up before this one opens its
/// tabs, so the window never holds two pages' terminals.
#[tauri::command]
pub async fn term_reset(window: tauri::Window) -> Result<(), String> {
    terminals::reset_window(window.label());
    Ok(())
}

/// Keys typed in the tab, as the terminal encodes them: queued for the
/// terminal's own writer, so a program that does not read them holds up
/// nothing else.
#[tauri::command]
pub async fn term_write(id: u64, data: String) -> Result<(), String> {
    terminals::write(id, data.as_bytes())
}

/// The shell in the tab asked for a password and the user took the
/// offer to type the stored one (read from the keychain, which may wait).
#[tauri::command]
pub async fn term_type_password(id: u64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || terminals::type_password(id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn term_resize(id: u64, cols: u16, rows: u16) -> Result<(), String> {
    terminals::resize(id, cols, rows);
    Ok(())
}

/// Open the ssh config, the log, or any file in the application the
/// system has for it. A file with no extension (the ssh config) goes to
/// the text editor; one that is missing is created first, empty and private.
#[tauri::command]
pub async fn open_path(app: AppHandle, path: String, editor: bool) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if editor && !std::path::Path::new(&path).exists() {
        let _ = std::fs::write(&path, "");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(target_os = "macos")]
    if editor {
        // `open -t` picks the application the user chose for plain text,
        // whatever the file's (missing) extension says.
        return std::process::Command::new("/usr/bin/open")
            .arg("-t")
            .arg(&path)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// An interactive `ssh <alias>` in the user's terminal, through the
/// `ssh://` URL handler (Terminal unless another application claims it).
#[tauri::command]
pub async fn open_shell(app: AppHandle, host: String) -> Result<(), String> {
    windows::open_in_terminal_app(&app, &host)
}

#[tauri::command]
pub async fn check_update(app: AppHandle, model: M<'_>) -> Result<(), String> {
    updater::check(&app, &model, true);
    Ok(())
}

#[tauri::command]
pub async fn install_update(app: AppHandle, model: M<'_>) -> Result<(), String> {
    updater::install(&app, &model);
    Ok(())
}

#[tauri::command]
/// Start a fresh copy once this one has quit. On macOS the bundle is
/// opened through LaunchServices only after this pid is gone, with the
/// SHELLDER_* hooks carried over so a test copy comes back as a test
/// copy; elsewhere Tauri's own restart does.
pub async fn relaunch(app: AppHandle, model: M<'_>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let exe = &config::get().self_path;
        let bundle = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .filter(|b| b.extension().map(|e| e == "app").unwrap_or(false));
        if let Some(bundle) = bundle {
            let mut script = format!(
                "while kill -0 {} 2>/dev/null; do sleep 0.2; done; exec /usr/bin/open -n",
                std::process::id()
            );
            for (k, v) in std::env::vars().filter(|(k, _)| k.starts_with("SHELLDER_")) {
                script.push_str(&format!(" --env '{k}={}'", v.replace('\'', "'\\''")));
            }
            script.push_str(" \"$0\"");
            std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .arg(bundle)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            crate::core::log::info("relaunch requested");
            windows::quit(&model, &app);
            return Ok(());
        }
    }
    model.shutdown();
    super::restart(&app)
}

#[tauri::command]
pub async fn quit(app: AppHandle, model: M<'_>) -> Result<(), String> {
    windows::quit(&model, &app);
    Ok(())
}

#[tauri::command]
pub async fn abbreviate_home(path: String) -> Result<String, String> {
    Ok(config::abbreviate_home(&path))
}

// MARK: Files windows

/// A Files window or a Console's Files tab opens: given the host list
/// now, as the list is handed on only when it changes.
#[tauri::command]
pub async fn files_open(
    app: AppHandle,
    model: M<'_>,
    host: String,
) -> Result<files::FilesSnapshot, String> {
    let m = files::for_host(&host);
    m.set_hosts(model.host_aliases());
    m.start(&app);
    Ok(m.snapshot())
}

#[tauri::command]
pub async fn files_state(host: String) -> Result<files::FilesSnapshot, String> {
    Ok(files::for_host(&host).snapshot())
}

#[tauri::command]
pub async fn files_set_source(
    host: String,
    pane: String,
    source: FileSource,
) -> Result<(), String> {
    files::for_host(&host).set_source(self::pane(&pane)?, source);
    Ok(())
}

#[tauri::command]
pub async fn files_set_root(host: String, pane: String, path: String) -> Result<(), String> {
    files::for_host(&host).set_root(self::pane(&pane)?, &path, true);
    Ok(())
}

#[tauri::command]
pub async fn files_subdirectories(
    host: String,
    pane: String,
    path: String,
) -> Result<Vec<files::Shortcut>, String> {
    let p = self::pane(&pane)?;
    tauri::async_runtime::spawn_blocking(move || files::for_host(&host).subdirectories(p, &path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn files_nav(
    host: String,
    pane: String,
    action: String,
    path: Option<String>,
) -> Result<(), String> {
    let m = files::for_host(&host);
    let p = self::pane(&pane)?;
    match action.as_str() {
        "back" => m.back(p),
        "up" => m.up(p),
        "reload" => m.reload(p),
        "toggle-hidden" => m.toggle_hidden(p),
        "toggle" => m.toggle(p, path.as_deref().unwrap_or("")),
        "expand" => m.expand(p, path.as_deref().unwrap_or("")),
        "collapse" => m.collapse(p, path.as_deref().unwrap_or("")),
        "collapse-all" => m.collapse_all(p, path.as_deref().unwrap_or("")),
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}

#[tauri::command]
pub async fn files_set_sort(host: String, pane: String, sort: FileSort) -> Result<(), String> {
    files::for_host(&host).set_sort(self::pane(&pane)?, sort);
    Ok(())
}

#[tauri::command]
pub async fn files_select(host: String, pane: String, paths: Vec<String>) -> Result<(), String> {
    files::for_host(&host).set_selected(self::pane(&pane)?, paths);
    Ok(())
}

#[tauri::command]
pub async fn files_target(host: String, pane: String) -> Result<String, String> {
    Ok(files::for_host(&host).target(self::pane(&pane)?))
}

#[tauri::command]
pub async fn files_create(
    host: String,
    pane: String,
    name: String,
    directory: bool,
) -> Result<(), String> {
    files::for_host(&host).create(self::pane(&pane)?, &name, directory);
    Ok(())
}

#[tauri::command]
pub async fn files_delete(host: String, pane: String, paths: Vec<String>) -> Result<(), String> {
    files::for_host(&host).delete(self::pane(&pane)?, paths);
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct Dragged {
    source: FileSource,
    path: String,
}

/// Rows dragged from one pane and dropped on a row (or the empty space)
/// of the other.
#[tauri::command]
pub async fn files_drop(
    host: String,
    pane: String,
    dragged: Vec<Dragged>,
    onto: Option<String>,
) -> Result<bool, String> {
    let m = files::for_host(&host);
    let p = self::pane(&pane)?;
    let directory = match onto {
        Some(path) => m.destination(p, &path),
        None => m.root(p),
    };
    Ok(m.accept(
        dragged.into_iter().map(|d| (d.source, d.path)).collect(),
        &directory,
        p,
    ))
}

/// Files dragged in from the desktop.
#[tauri::command]
pub async fn files_drop_local(
    host: String,
    pane: String,
    paths: Vec<String>,
    onto: Option<String>,
) -> Result<bool, String> {
    let m = files::for_host(&host);
    let p = self::pane(&pane)?;
    let directory = match onto {
        Some(path) => m.destination(p, &path),
        None => m.root(p),
    };
    Ok(m.accept_local_files(paths, &directory, p))
}

/// The start of a file, for the Console's preview tab: up to `limit + 1`
/// bytes of a file on a host (through its master or its session, like a
/// listing) or on this machine, as they are (the page gets an
/// ArrayBuffer). One more than asked tells the page the file goes on.
#[tauri::command]
pub async fn files_read(
    source: FileSource,
    path: String,
    limit: usize,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (data, _) = crate::core::remotefs::fs::read(&source, &path, limit + 1)
            .map_err(|e| e.to_string())?;
        Ok(tauri::ipc::Response::new(data))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn files_copy_selection(host: String, pane: String) -> Result<(), String> {
    files::for_host(&host).copy_selection(self::pane(&pane)?);
    Ok(())
}

#[tauri::command]
pub async fn files_transfer(
    host: String,
    action: String,
    id: Option<u64>,
    answer: Option<ReplaceAnswer>,
    on: Option<bool>,
) -> Result<(), String> {
    let m = files::for_host(&host);
    match action.as_str() {
        "cancel" => m.cancel(),
        "clear" => m.clear_history(),
        "retry" => {
            if let Some(id) = id {
                m.retry(id)
            }
        }
        "answer" => {
            if let Some(a) = answer {
                m.answer_replace(a)
            }
        }
        "history" => m.set_show_history(on.unwrap_or(true)),
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}
