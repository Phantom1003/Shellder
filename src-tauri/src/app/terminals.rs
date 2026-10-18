//! The terminals of the Console windows (a shell, a tmux session, a
//! container…): any number per window, each known by an id. Output goes to
//! the page as events, keys come back as commands.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use tauri::{AppHandle, Emitter};

use super::model::Model;
use crate::core::keychain::{self, SecretKind};
use crate::core::log;
use crate::core::term::{Launch, Terminal};

/// Every terminal by its id: the window's label and the terminal.
type Terms = HashMap<u64, (String, Arc<Terminal>)>;

static TERMS: Mutex<Option<Terms>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with<R>(f: impl FnOnce(&mut Terms) -> R) -> R {
    let mut guard = TERMS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// What the page gets with every piece of output and with the end: the
/// id `open` returned, which says which of the window's terminals it is.
#[derive(Clone, serde::Serialize)]
struct Data {
    id: u64,
    data: String,
}

#[derive(Clone, serde::Serialize)]
struct Exit {
    id: u64,
    code: i32,
}

/// Start `ssh host command` (the account's shell without one, in `dir`
/// when given, with the shell integration when Settings say so) for the
/// window `label`, whose page has measured the terminal. Its output arrives as
/// `term-data` events (base64, so the bytes come through as they are) and
/// its end as one `term-exit`, both carrying the id this returns.
#[allow(clippy::too_many_arguments)]
pub fn open(
    app: &AppHandle,
    model: &Arc<Model>,
    label: &str,
    host: &str,
    command: Option<&str>,
    dir: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<u64, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let launch = match command {
        Some(c) => Launch::Command(c),
        None => Launch::Shell {
            dir,
            integration: crate::core::prefs::load().shell_integration(),
        },
    };
    let out = app.clone();
    let out_label = label.to_string();
    let exit = app.clone();
    let exit_label = label.to_string();
    let handler: Arc<dyn crate::core::prompt::PromptHandler> = model.clone();
    let t = Terminal::open(
        host,
        launch,
        cols,
        rows,
        handler,
        move |bytes| {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            let _ = out.emit_to(out_label.as_str(), "term-data", Data { id, data });
        },
        move |code| {
            let _ = exit.emit_to(exit_label.as_str(), "term-exit", Exit { id, code });
            with(|m| {
                m.remove(&id);
            });
        },
    )?;
    with(|m| {
        m.insert(id, (label.to_string(), Arc::new(t)));
    });
    Ok(id)
}

fn get(id: u64) -> Option<Arc<Terminal>> {
    with(|m| m.get(&id).map(|(_, t)| t.clone()))
}

pub fn write(id: u64, bytes: &[u8]) -> Result<(), String> {
    let t = get(id).ok_or("this terminal is gone")?;
    t.write(bytes).map_err(|e| e.to_string())
}

/// The password stored for the terminal's host typed into it, Return
/// included, at the user's word: the page saw the shell ask for one
/// (sudo's, su's) and offered. The secret stays on this side; the page
/// only asks for it to be typed.
pub fn type_password(id: u64) -> Result<(), String> {
    let t = get(id).ok_or("this terminal is gone")?;
    let Some(password) = keychain::get(&t.host, SecretKind::Password) else {
        return Err(format!("no password stored for {}", t.host));
    };
    let mut bytes = password.into_bytes();
    bytes.push(b'\r');
    t.write(&bytes).map_err(|e| e.to_string())?;
    log::info(format!(
        "{}: the stored password typed into a console terminal at the user's request",
        t.host
    ));
    Ok(())
}

pub fn resize(id: u64, cols: u16, rows: u16) {
    if let Some(t) = get(id) {
        t.resize(cols, rows);
    }
}

/// The page closed the tab: hang up (a tmux client detaches, the session
/// lives on).
pub fn close(id: u64) {
    if let Some((_, t)) = with(|m| m.remove(&id)) {
        log::info(format!("{}: console terminal closed", t.host));
        t.close();
    }
}

/// The window went away: hang up on all of its terminals.
pub fn close_window(label: &str) {
    hang_up_window(label, "console window closed");
}

/// The window's page starts afresh (a reload: `cargo tauri dev` does one
/// on every change under `ui/`): what the page before it opened is
/// nobody's now, and would hold its sessions on the master beside the new
/// page's until MaxSessions refused the rest, so it is hung up first.
pub fn reset_window(label: &str) {
    hang_up_window(label, "console page reloaded, its terminal hung up");
}

fn hang_up_window(label: &str, why: &str) {
    let gone: Vec<Arc<Terminal>> = with(|m| {
        let ids: Vec<u64> = m
            .iter()
            .filter(|(_, (l, _))| l == label)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| m.remove(&id).map(|(_, t)| t))
            .collect()
    });
    for t in gone {
        log::info(format!("{}: {why}", t.host));
        t.close();
    }
}

/// Every terminal: what quitting hangs up on.
pub fn close_all() {
    let all: Vec<Arc<Terminal>> = with(|m| m.drain().map(|(_, (_, t))| t).collect());
    for t in &all {
        t.close();
    }
    // Hung up is not gone: the process exits in its own time, and one
    // still there when the app has exited keeps the app "running" for
    // macOS 26. A moment, not for ever.
    let deadline = Instant::now() + Duration::from_secs(3);
    while all.iter().any(|t| t.is_running()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}
