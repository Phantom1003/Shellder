//! The terminal in a Console window (the tmux session it opened): one
//! `Terminal` per window, found by the window's label. Output goes to the
//! page as events, keys come back as commands.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use tauri::{AppHandle, Emitter};

use super::model::Model;
use crate::core::log;
use crate::core::term::Terminal;

/// The window's terminal and the id it was opened with.
type Terms = HashMap<String, (u64, Arc<Terminal>)>;

static TERMS: Mutex<Option<Terms>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with<R>(f: impl FnOnce(&mut Terms) -> R) -> R {
    let mut guard = TERMS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// What the page gets with every piece of output and with the end: the
/// id `open` returned, so that what an earlier terminal of the window
/// still says after it was replaced is told apart.
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

/// Start `ssh host command` (the login shell without one) for the window
/// `label`, which is already open and has measured itself. Its output
/// arrives as `term-data` events (base64, so the bytes come through as
/// they are) and its end as one `term-exit`, both carrying the id this
/// returns.
pub fn open(
    app: &AppHandle,
    model: &Arc<Model>,
    label: &str,
    host: &str,
    command: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<u64, String> {
    close(label);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let out = app.clone();
    let out_label = label.to_string();
    let exit = app.clone();
    let exit_label = label.to_string();
    let handler: Arc<dyn crate::core::prompt::PromptHandler> = model.clone();
    let t = Terminal::open(
        host,
        command,
        cols,
        rows,
        handler,
        move |bytes| {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            let _ = out.emit_to(out_label.as_str(), "term-data", Data { id, data });
        },
        move |code| {
            let _ = exit.emit_to(exit_label.as_str(), "term-exit", Exit { id, code });
            // The window may already hold a newer terminal: leave that one.
            with(|m| {
                if m.get(&exit_label).map(|(i, _)| *i) == Some(id) {
                    m.remove(&exit_label);
                }
            });
        },
    )?;
    with(|m| {
        m.insert(label.to_string(), (id, Arc::new(t)));
    });
    Ok(id)
}

fn get(label: &str) -> Option<Arc<Terminal>> {
    with(|m| m.get(label).map(|(_, t)| t.clone()))
}

pub fn write(label: &str, bytes: &[u8]) -> Result<(), String> {
    let t = get(label).ok_or("nothing is open in this window")?;
    t.write(bytes).map_err(|e| e.to_string())
}

pub fn resize(label: &str, cols: u16, rows: u16) {
    if let Some(t) = get(label) {
        t.resize(cols, rows);
    }
}

/// The window went away, or wants another session: hang up on what it
/// has (a tmux client detaches; the session lives on).
pub fn close(label: &str) {
    if let Some((_, t)) = with(|m| m.remove(label)) {
        log::info(format!("{}: console terminal closed", t.host));
        t.close();
    }
}

/// Every terminal: what quitting hangs up on.
pub fn close_all() {
    let all: Vec<Arc<Terminal>> = with(|m| m.drain().map(|(_, (_, t))| t).collect());
    for t in all {
        t.close();
    }
}
