//! The terminals of the Console windows (a shell, a tmux session, a
//! container…): any number per window, each known by an id. Output goes to
//! the page as events, keys come back as commands.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

use base64::Engine;
use tauri::{AppHandle, Emitter};

use super::model::Model;
use crate::core::keychain::{self, SecretKind};
use crate::core::log;
use crate::core::term::{Launch, Terminal};

/// A terminal of a window, the queue to its writer, and how much of
/// its output the page has still to parse.
struct Tab {
    label: String,
    term: Arc<Terminal>,
    keys: Sender<Vec<u8>>,
    flow: Arc<Flow>,
}

/// Gone from the terminals, however: its reader waits for the page no
/// more.
impl Drop for Tab {
    fn drop(&mut self) {
        self.flow.close();
    }
}

/// What holds a terminal's output back to the pace its page parses it
/// at. A pty gives a flood faster than xterm.js takes it (some 65 MB a
/// second against 20 to 30 in the window), the pieces queue up in the
/// page, and xterm throws away what it is given once 50 MB wait: the
/// output of a `cat` of a large file lost its end, and Ctrl-C did
/// nothing a user could see until the queue was through. So the page
/// says how much it has parsed (`term_ack`), and the terminal's reader
/// waits, before it hands a piece on, while [`WAITING`] bytes are out:
/// it reads the pty no further, the pty fills, and the program waits on
/// its write as it does on any terminal that is slow.
struct Flow {
    state: Mutex<FlowState>,
    changed: Condvar,
}

struct FlowState {
    /// Bytes sent to the page that it has not said it parsed.
    waiting: usize,
    /// The page's window is on screen. One that is not runs its timers
    /// late, xterm's among them: its terminals are not held to that
    /// pace, a build left running in a window put away goes on as fast
    /// as it did.
    held: bool,
    open: bool,
}

/// As much may wait in the page: a twentieth of a second of a flood at
/// the pace the window parses it, so what is typed (Ctrl-C) shows at
/// once. The page acknowledges in steps a quarter of this
/// (`ACK_STEP` in console.js), so what it has parsed and not yet said
/// never holds the reader.
const WAITING: usize = 1 << 20;

/// A page that says nothing for this long while the reader waits is not
/// waited for: the piece goes on, as all did before, rather than a
/// terminal standing still for a reason nobody sees.
const PATIENCE: Duration = Duration::from_secs(10);

impl Flow {
    fn new(held: bool) -> Flow {
        Flow {
            state: Mutex::new(FlowState {
                waiting: 0,
                held,
                open: true,
            }),
            changed: Condvar::new(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, FlowState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The reader has `bytes` for the page: waits while the page has as
    /// much as it may to parse, then counts them. False when the page
    /// said nothing for `patience` and the piece goes on unwaited.
    fn send(&self, bytes: usize, patience: Duration) -> bool {
        let mut s = self.state();
        let mut heard = true;
        while s.open && s.held && s.waiting >= WAITING {
            let before = s.waiting;
            let (next, timeout) = self
                .changed
                .wait_timeout(s, patience)
                .unwrap_or_else(|e| e.into_inner());
            s = next;
            if timeout.timed_out() && s.waiting >= before {
                s.waiting = 0;
                heard = false;
            }
        }
        s.waiting += bytes;
        heard
    }

    /// The page parsed `bytes`.
    fn parsed(&self, bytes: usize) {
        let mut s = self.state();
        s.waiting = s.waiting.saturating_sub(bytes);
        self.changed.notify_all();
    }

    fn hold(&self, held: bool) {
        self.state().held = held;
        self.changed.notify_all();
    }

    fn close(&self) {
        self.state().open = false;
        self.changed.notify_all();
    }
}

/// The windows whose page said it is not on screen (`hold`).
static AWAY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn away<R>(f: impl FnOnce(&mut HashSet<String>) -> R) -> R {
    let mut guard = AWAY.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashSet::new))
}

/// Every terminal by its id.
type Terms = HashMap<u64, Tab>;

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
    let flow = Arc::new(Flow::new(!away(|a| a.contains(label))));
    let pace = flow.clone();
    let slow_host = host.to_string();
    let t = Terminal::open(
        host,
        launch,
        cols,
        rows,
        handler,
        move |bytes| {
            // On the reader's thread: waiting here is not reading the pty.
            if !pace.send(bytes.len(), PATIENCE) {
                log::warn(format!(
                    "{slow_host}: the page has not taken the terminal's output for {} s, going on without it",
                    PATIENCE.as_secs()
                ));
            }
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
    let term = Arc::new(t);
    let keys = writer(id, Arc::downgrade(&term));
    with(|m| {
        m.insert(
            id,
            Tab {
                label: label.to_string(),
                term,
                keys,
                flow,
            },
        );
    });
    Ok(id)
}

/// The page parsed `bytes` of the terminal's output.
pub fn ack(id: u64, bytes: usize) {
    if let Some(flow) = with(|m| m.get(&id).map(|tab| tab.flow.clone())) {
        flow.parsed(bytes);
    }
}

/// The window's page is on screen (`held`) or not: whether its
/// terminals wait for it (see [`FlowState::held`]).
pub fn hold(label: &str, held: bool) {
    away(|a| {
        if held {
            a.remove(label);
        } else {
            a.insert(label.to_string());
        }
    });
    let flows: Vec<Arc<Flow>> = with(|m| {
        m.values()
            .filter(|tab| tab.label == label)
            .map(|tab| tab.flow.clone())
            .collect()
    });
    for flow in flows {
        flow.hold(held);
    }
}

/// The thread that writes what is typed into a terminal, in the order it
/// was typed. The pty takes only so much that its program has not read:
/// a paste into a program busy with something else waits here, on this
/// terminal's thread, not on a thread every window's calls need. It ends
/// with the terminal (the queue's sender goes with its entry) or at the
/// first write that fails.
fn writer(id: u64, term: Weak<Terminal>) -> Sender<Vec<u8>> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let spawned = std::thread::Builder::new()
        .name(format!("term-keys-{id}"))
        .spawn(move || {
            for bytes in rx {
                let Some(t) = term.upgrade() else {
                    break;
                };
                if let Err(e) = t.write(&bytes) {
                    log::warn(format!("{}: cannot write to the terminal: {e}", t.host));
                    break;
                }
            }
        });
    if let Err(e) = spawned {
        log::error(format!("terminal {id}: no thread for its keys: {e}"));
    }
    tx
}

fn get(id: u64) -> Option<Arc<Terminal>> {
    with(|m| m.get(&id).map(|tab| tab.term.clone()))
}

/// Bytes for the terminal, queued for its writer: this never waits on
/// the terminal.
fn send(id: u64, bytes: Vec<u8>) -> Result<(), String> {
    let keys = with(|m| m.get(&id).map(|tab| tab.keys.clone())).ok_or("this terminal is gone")?;
    keys.send(bytes)
        .map_err(|_| "this terminal is gone".to_string())
}

pub fn write(id: u64, bytes: &[u8]) -> Result<(), String> {
    send(id, bytes.to_vec())
}

/// The password stored for the terminal's host typed into it, Return
/// included, at the user's word: the page saw the shell ask for one
/// (sudo's, su's) and offered. The secret stays on this side; the page
/// only asks for it to be typed. It goes in the queue behind the keys
/// typed before it.
pub fn type_password(id: u64) -> Result<(), String> {
    let t = get(id).ok_or("this terminal is gone")?;
    let Some(password) = keychain::get(&t.host, SecretKind::Password) else {
        return Err(format!("no password stored for {}", t.host));
    };
    let mut bytes = password.into_bytes();
    bytes.push(b'\r');
    send(id, bytes)?;
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
    if let Some(tab) = with(|m| m.remove(&id)) {
        log::info(format!("{}: console terminal closed", tab.term.host));
        tab.term.close();
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
            .filter(|(_, tab)| tab.label == label)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| m.remove(&id).map(|tab| tab.term.clone()))
            .collect()
    });
    for t in gone {
        log::info(format!("{}: {why}", t.host));
        t.close();
    }
}

/// Every terminal: what quitting hangs up on.
pub fn close_all() {
    let all: Vec<Arc<Terminal>> = with(|m| m.drain().map(|(_, tab)| tab.term.clone()).collect());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that has `WAITING` bytes out, on a thread: whether its
    /// next piece went on within `within`, and what `send` said.
    fn next_piece(flow: &Arc<Flow>, patience: Duration, within: Duration) -> Option<bool> {
        let (tx, rx) = mpsc::channel();
        let f = flow.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f.send(16384, patience));
        });
        rx.recv_timeout(within).ok()
    }

    const LONG: Duration = Duration::from_secs(60);
    const SHORT: Duration = Duration::from_millis(100);

    #[test]
    fn the_reader_waits_for_the_page_and_no_longer() {
        let flow = Arc::new(Flow::new(true));
        // Up to the limit nothing waits.
        for _ in 0..WAITING / 16384 {
            assert!(flow.send(16384, LONG));
        }
        assert_eq!(flow.state().waiting, WAITING);
        // At it, the next piece waits, and goes on when the page has
        // parsed some.
        let (tx, rx) = mpsc::channel();
        let f = flow.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f.send(16384, LONG));
        });
        assert!(rx.recv_timeout(SHORT).is_err(), "it did not wait");
        flow.parsed(WAITING / 4);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert_eq!(flow.state().waiting, WAITING - WAITING / 4 + 16384);
        // More parsed than was sent (a page counting another way) is
        // nothing waiting, not less than nothing.
        flow.parsed(usize::MAX);
        assert_eq!(flow.state().waiting, 0);
    }

    #[test]
    fn a_window_put_away_or_a_tab_closed_holds_nothing() {
        for free in [Flow::hold as fn(&Flow, bool), |f, _| f.close()] {
            let flow = Arc::new(Flow::new(true));
            assert!(flow.send(WAITING, LONG));
            let (tx, rx) = mpsc::channel();
            let f = flow.clone();
            std::thread::spawn(move || {
                let _ = tx.send(f.send(16384, LONG));
            });
            assert!(rx.recv_timeout(SHORT).is_err(), "it did not wait");
            free(&flow, false);
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
        }
        // A window that was away when the terminal opened.
        let flow = Arc::new(Flow::new(false));
        assert!(flow.send(WAITING, LONG));
        assert_eq!(next_piece(&flow, LONG, Duration::from_secs(5)), Some(true));
        // Back on screen with all of that still to parse: it waits.
        flow.hold(true);
        assert_eq!(next_piece(&flow, LONG, SHORT), None);
        flow.close();
    }

    #[test]
    fn a_page_that_says_nothing_is_not_waited_for_for_ever() {
        let flow = Arc::new(Flow::new(true));
        assert!(flow.send(WAITING, LONG));
        assert_eq!(
            next_piece(&flow, Duration::from_millis(50), Duration::from_secs(5)),
            Some(false)
        );
        // What it was waited for is forgotten: the next pieces go on.
        assert_eq!(flow.state().waiting, 16384);
        assert!(flow.send(16384, LONG));
    }
}
