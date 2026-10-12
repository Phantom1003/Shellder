//! What the windows may ask of the app. Anything that may wait (ssh, the
//! keychain, a listing) is async so it runs off the main thread.

use std::sync::Arc;

use tauri::{AppHandle, State};

use super::files::{self, FileSort, Pane, ReplaceAnswer};
use super::model::{self, Model, PromptResponse, Snapshot};
use super::{terminals, updater, windows};
use crate::core::config;
use crate::core::keychain::SecretKind;
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
pub async fn reload_config(model: M<'_>) -> Result<(), String> {
    model.reload_catalog(true);
    Ok(())
}

#[tauri::command]
pub async fn set_secret(
    model: M<'_>,
    host: String,
    kind: String,
    value: String,
) -> Result<(), String> {
    model.set_secret(&host, self::kind(&kind)?, &value)
}

#[tauri::command]
pub async fn remove_secret(model: M<'_>, host: String, kind: String) -> Result<(), String> {
    model.remove_secret(&host, self::kind(&kind)?);
    Ok(())
}

#[tauri::command]
pub async fn remove_all_secrets(model: M<'_>, host: String) -> Result<(), String> {
    for k in SecretKind::ALL {
        if model.has_secret(&host, k) {
            model.remove_secret(&host, k);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn reveal_secret(
    model: M<'_>,
    host: String,
    kind: String,
) -> Result<Option<String>, String> {
    Ok(model.reveal_secret(&host, self::kind(&kind)?))
}

#[derive(serde::Serialize)]
pub struct TotpCode {
    code: String,
    remaining: u64,
}

#[tauri::command]
pub async fn totp_code(model: M<'_>, host: String) -> Result<Option<TotpCode>, String> {
    Ok(model
        .totp_code(&host)
        .map(|(code, remaining)| TotpCode { code, remaining }))
}

#[tauri::command]
pub async fn request_keychain_access(model: M<'_>) -> Result<(), String> {
    model.request_keychain_access();
    Ok(())
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

#[tauri::command]
pub async fn log_tail() -> Result<Vec<String>, String> {
    Ok(model::log_tail())
}

#[tauri::command]
pub async fn open_window(
    app: AppHandle,
    which: String,
    host: Option<String>,
) -> Result<(), String> {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || match which.as_str() {
        "main" => windows::show_main(&a),
        "settings" => windows::show_settings(&a),
        "files" => {
            if let Some(h) = host {
                windows::show_files(&a, &h)
            }
        }
        "console" => {
            if let Some(h) = host {
                windows::show_console(&a, &h)
            }
        }
        _ => {}
    });
    Ok(())
}

// MARK: console windows

/// What the host is doing right now: the page polls this while it is open.
#[tauri::command]
pub async fn console_snapshot(host: String) -> Result<monitor::Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || monitor::fetch(&host))
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
            let why = out.text();
            return Err(if why.is_empty() {
                format!("exited with status {}", out.status)
            } else {
                why
            });
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
            let why = out.text();
            return Err(if why.is_empty() {
                format!("exited with status {}", out.status)
            } else {
                why
            });
        }
        crate::core::log::info(format!("{host}: {kind} session {id} ended"));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What a mount, unmount or eject came to.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskOutcome {
    /// What the host said (udisks and diskutil say where a mount went),
    /// or why it refused when `ask` is set.
    pub said: String,
    /// When the host would do it for a password (polkit over ssh, sudo):
    /// the command to run in a terminal tab, where it asks.
    pub ask: Option<String>,
}

/// Mount or unmount a removable volume, or eject its drive, from the
/// Console's Disks card; `device` is a `/dev` path the readings named.
/// Run without a terminal first; a refusal a password would lift is not
/// an error but the command to run in a tab.
#[tauri::command]
pub async fn console_disk(
    host: String,
    action: String,
    device: String,
) -> Result<DiskOutcome, String> {
    use crate::core::remotefs::remote;
    let command = monitor::disk_command(&action, &device, false)?;
    let asking = monitor::disk_command(&action, &device, true)?;
    tauri::async_runtime::spawn_blocking(move || {
        let out = remote::shell(&host, &command);
        if out.status != 0 {
            let mut why = out.text();
            if why.is_empty() {
                why = format!("exited with status {}", out.status);
            }
            if monitor::wants_password(&why) {
                crate::core::log::info(format!(
                    "{host}: {action} {device} wants a password, asking in a terminal: {why}"
                ));
                return Ok(DiskOutcome {
                    said: why,
                    ask: Some(asking),
                });
            }
            return Err(why);
        }
        crate::core::log::info(format!("{host}: {action} {device}"));
        Ok(DiskOutcome {
            said: out.stdout.trim().to_string(),
            ask: None,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The page has measured its terminal: run `command` on the host in it
/// (a `tmux attach`), or the login shell without one.
#[tauri::command]
pub async fn term_open(
    app: AppHandle,
    window: tauri::Window,
    model: M<'_>,
    host: String,
    command: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<u64, String> {
    let label = window.label().to_string();
    let m = model.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        terminals::open(&app, &m, &label, &host, command.as_deref(), cols, rows)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Hang up on the tab's terminal: it was closed (a tmux client detaches,
/// the session lives on).
#[tauri::command]
pub async fn term_close(id: u64) -> Result<(), String> {
    terminals::close(id);
    Ok(())
}

/// Keys typed in the tab, as the terminal encodes them.
#[tauri::command]
pub async fn term_write(id: u64, data: String) -> Result<(), String> {
    terminals::write(id, data.as_bytes())
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

#[tauri::command]
pub async fn files_open(app: AppHandle, host: String) -> Result<files::FilesSnapshot, String> {
    let m = files::for_host(&host);
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
