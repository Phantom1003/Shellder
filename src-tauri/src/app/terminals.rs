//! The shell windows: one `Terminal` per window, found by the window's
//! label. Output goes to the page as events, keys come back as commands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use tauri::{AppHandle, Emitter};

use super::model::Model;
use crate::core::log;
use crate::core::term::Terminal;

static TERMS: Mutex<Option<HashMap<String, Arc<Terminal>>>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut HashMap<String, Arc<Terminal>>) -> R) -> R {
    let mut guard = TERMS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// Start the shell for the window `label`, which is already open and has
/// measured itself. Its output arrives as `term-data` events (base64, so
/// the bytes come through as they are) and its end as one `term-exit`.
pub fn open(
    app: &AppHandle,
    model: &Arc<Model>,
    label: &str,
    host: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    close(label);
    let out = app.clone();
    let out_label = label.to_string();
    let exit = app.clone();
    let exit_label = label.to_string();
    let handler: Arc<dyn crate::core::prompt::PromptHandler> = model.clone();
    let t = Terminal::open(
        host,
        cols,
        rows,
        handler,
        move |bytes| {
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            let _ = out.emit_to(out_label.as_str(), "term-data", b64);
        },
        move |code| {
            let _ = exit.emit_to(exit_label.as_str(), "term-exit", code);
            with(|m| {
                m.remove(&exit_label);
            });
        },
    )?;
    with(|m| {
        m.insert(label.to_string(), Arc::new(t));
    });
    Ok(())
}

fn get(label: &str) -> Option<Arc<Terminal>> {
    with(|m| m.get(label).cloned())
}

pub fn write(label: &str, bytes: &[u8]) -> Result<(), String> {
    let t = get(label).ok_or("no shell in this window")?;
    t.write(bytes).map_err(|e| e.to_string())
}

pub fn resize(label: &str, cols: u16, rows: u16) {
    if let Some(t) = get(label) {
        t.resize(cols, rows);
    }
}

/// The window went away: hang up on its shell.
pub fn close(label: &str) {
    if let Some(t) = with(|m| m.remove(label)) {
        log::info(format!("{}: terminal window closed", t.host));
        t.close();
    }
}

/// Every shell: what quitting hangs up on.
pub fn close_all() {
    let all: Vec<Arc<Terminal>> = with(|m| m.drain().map(|(_, t)| t).collect());
    for t in all {
        t.close();
    }
}
