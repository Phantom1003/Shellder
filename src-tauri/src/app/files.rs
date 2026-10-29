//! The two trees of a Files window and the copies dragged between them.
//! Each pane looks at its own side — this machine or any host, through its
//! master when one is up and through a session of its own otherwise — so
//! the same window also copies between two hosts. Listings and copies run
//! on their own threads, a few listings of one side at a time. A change
//! is pushed to the window as one snapshot, a burst of changes as a
//! snapshot or two; a copy's progress goes as a small event of its own.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::core::log;
use crate::core::prefs;
use crate::core::remotefs::{self, fs, FileItem, FsError};
use crate::core::ssh::{self, FileSource};
use crate::core::transfer::{TransferJob, TransferRun};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pane {
    Left,
    Right,
}

impl Pane {
    pub const ALL: [Pane; 2] = [Pane::Left, Pane::Right];
    pub fn other(self) -> Pane {
        match self {
            Pane::Left => Pane::Right,
            Pane::Right => Pane::Left,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Pane::Left => "left",
            Pane::Right => "right",
        }
    }
}

/// How a pane orders its rows: by one column, either way round. By name,
/// directories come first. By date or by size they sit among the files,
/// as in the Finder, so what changed last, or the biggest thing, is at the
/// top whatever it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSort {
    pub key: SortKey,
    pub ascending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    Name,
    Modified,
    Size,
}

impl Default for FileSort {
    fn default() -> Self {
        FileSort {
            key: SortKey::Name,
            ascending: true,
        }
    }
}

impl FileSort {
    /// As the prefs keep it: "modified descending".
    pub fn saved(&self) -> String {
        let k = match self.key {
            SortKey::Name => "name",
            SortKey::Modified => "modified",
            SortKey::Size => "size",
        };
        format!(
            "{k} {}",
            if self.ascending {
                "ascending"
            } else {
                "descending"
            }
        )
    }

    pub fn parse(saved: &str) -> Option<FileSort> {
        let mut parts = saved.split(' ');
        let key = match parts.next()? {
            "name" => SortKey::Name,
            "modified" => SortKey::Modified,
            "size" => SortKey::Size,
            _ => return None,
        };
        let ascending = parts.next()? != "descending";
        Some(FileSort { key, ascending })
    }

    /// Put the paths of a directory's entries in this order, `items`
    /// saying what each one is.
    pub fn order(&self, paths: &mut [String], items: &HashMap<String, FileItem>) {
        paths.sort_by(|a, b| match (items.get(a), items.get(b)) {
            (Some(x), Some(y)) => self.compare(x, y),
            _ => a.cmp(b),
        });
    }

    /// Whether `a` goes above `b`. Equal dates and sizes fall back to the
    /// names, A to Z, and names that differ only in case to their bytes.
    fn compare(&self, a: &FileItem, b: &FileItem) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        if self.key == SortKey::Name && a.is_dir != b.is_dir {
            return if a.is_dir { Less } else { Greater };
        }
        let flip = |o: std::cmp::Ordering| if self.ascending { o } else { o.reverse() };
        match self.key {
            SortKey::Name => {
                let o = natural_compare(&a.name, &b.name);
                if o != Equal {
                    return flip(o);
                }
            }
            SortKey::Modified => {
                let (x, y) = (
                    a.modified.unwrap_or(f64::MIN),
                    b.modified.unwrap_or(f64::MIN),
                );
                if x != y {
                    return flip(x.partial_cmp(&y).unwrap_or(Equal));
                }
            }
            SortKey::Size => {
                // A directory has no size of its own: smaller than any file.
                let (x, y) = (
                    if a.is_dir { -1 } else { a.size },
                    if b.is_dir { -1 } else { b.size },
                );
                if x != y {
                    return flip(x.cmp(&y));
                }
            }
        }
        match natural_compare(&a.name, &b.name) {
            Equal => a.name.cmp(&b.name),
            o => o,
        }
    }
}

/// The Finder's order: case-insensitive, with runs of digits compared as
/// numbers ("file2" before "file10").
pub fn natural_compare(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < ac.len() && j < bc.len() {
        if ac[i].is_ascii_digit() && bc[j].is_ascii_digit() {
            let si = i;
            while i < ac.len() && ac[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < bc.len() && bc[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = ac[si..i].iter().collect();
            let nb: String = bc[sj..j].iter().collect();
            let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
            match ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)) {
                Equal => continue,
                o => return o,
            }
        }
        let (x, y) = (
            ac[i].to_lowercase().next().unwrap_or(ac[i]),
            bc[j].to_lowercase().next().unwrap_or(bc[j]),
        );
        if x != y {
            return x.cmp(&y);
        }
        i += 1;
        j += 1;
    }
    (ac.len() - i).cmp(&(bc.len() - j))
}

/// One line of a pane: the item and how deep it sits.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    #[serde(flatten)]
    pub item: FileItem,
    pub depth: usize,
    pub expanded: bool,
    pub loading: bool,
    /// Why this directory could not be read, the last time it was tried.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A pane's rows as last flattened, shared by the pane and the snapshots
/// made from it until something changes them.
#[derive(Debug, Clone, Default)]
pub struct Rows(Arc<Vec<Row>>);

impl std::ops::Deref for Rows {
    type Target = [Row];
    fn deref(&self) -> &[Row] {
        &self.0
    }
}

impl Serialize for Rows {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TransferState {
    Waiting,
    Running,
    Done,
    Cancelled,
    Skipped,
    Failed { why: String },
}

/// A copy, from the moment it is dropped until long after it has finished:
/// the list under the panes keeps them so a transfer is something to look
/// back at, not a bar that flashes past.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRecord {
    pub id: u64,
    pub job: TransferJob,
    pub name: String,
    pub is_upload: bool,
    /// Asked for again after it stopped, it goes on from what landed
    /// instead of starting over (not between two hosts: scp starts over).
    pub can_resume: bool,
    pub state: TransferState,
    /// 0…1 while it runs, None while the size is unknown.
    pub progress: Option<f64>,
    pub total: Option<i64>,
    pub queued_at: f64,
    pub ended_at: Option<f64>,
}

impl TransferRecord {
    fn is_over(&self) -> bool {
        !matches!(self.state, TransferState::Waiting | TransferState::Running)
    }
    /// A copy that did not happen can be asked for again.
    fn can_retry(&self) -> bool {
        matches!(
            self.state,
            TransferState::Failed { .. } | TransferState::Cancelled | TransferState::Skipped
        )
    }
    /// Taken off the queue before it started: nothing of it landed, so
    /// asked for again it is a fresh copy, unless it was itself going on
    /// from an earlier one.
    fn call_off(&mut self) {
        self.state = TransferState::Cancelled;
        self.ended_at = Some(now());
        self.can_resume = self.can_resume && self.job.resume;
    }
}

/// How far the running copy has got: the event that says so, which is
/// all a window needs to move its bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub id: u64,
    pub progress: f64,
}

/// A place a pane can jump to, the way Finder's sidebar and Go menu do.
#[derive(Debug, Clone, Serialize)]
pub struct Shortcut {
    pub title: String,
    pub path: String,
}

/// What to do about a copy that would write over something that is
/// already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplaceAnswer {
    Replace,
    ReplaceAll,
    Skip,
    SkipAll,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceQuestion {
    pub id: u64,
    pub name: String,
    pub destination: String,
    pub to_title: String,
    pub offer_all: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneView {
    pub source: FileSource,
    pub title: String,
    pub root: String,
    pub error: Option<String>,
    pub rows: Rows,
    pub show_hidden: bool,
    pub sort: FileSort,
    pub can_go_back: bool,
    pub selected: Vec<String>,
    pub shortcuts: Vec<Shortcut>,
    /// The home directory on this side, for the folder menu's button.
    pub home: String,
    pub loading_root: bool,
    /// Moves whenever what the pane shows changes, its selection aside: a
    /// window draws a pane again only when it moved.
    pub version: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesSnapshot {
    pub host: String,
    pub hosts: Vec<String>,
    pub panes: BTreeMap<String, PaneView>,
    pub transfers: Vec<TransferRecord>,
    pub active: Option<TransferRecord>,
    pub queued: usize,
    pub last_finished: Option<TransferRecord>,
    pub notice: String,
    pub notice_is_error: bool,
    pub show_history: bool,
    pub question: Option<ReplaceQuestion>,
    pub can_copy: BTreeMap<String, bool>,
    pub can_retry: Vec<u64>,
    /// Counts the snapshots: one that arrives after a later one (the
    /// answer to a call, overtaken by an event) is older than what the
    /// window shows.
    pub seq: u64,
}

#[derive(Default)]
struct PaneState {
    source: Option<FileSource>,
    root: String,
    /// Every entry read so far, by path: the root's and those of the
    /// directories read under it.
    items: HashMap<String, FileItem>,
    /// The paths in each directory read, hidden ones included, in the
    /// pane's order. A directory that is not here has not been read.
    listed: HashMap<String, Vec<String>>,
    /// Why a directory could not be read, the last time it was tried.
    failed: HashMap<String, String>,
    expanded: HashSet<String>,
    /// The directories asked to be read: those waiting for their turn
    /// and those being read.
    loading: HashSet<String>,
    waiting: VecDeque<String>,
    reading: HashSet<String>,
    /// Being read and asked for again meanwhile: what changed may not be
    /// in the listing that lands, so it is read once more after it.
    again: HashSet<String>,
    /// Which tree the listings are for: one that lands after the pane
    /// moved to another side or root is dropped.
    generation: u64,
    /// For every root shown so far, which directories under it were open
    /// when it was last left: a directory shown again starts from that.
    views: HashMap<String, HashSet<String>>,
    show_hidden: bool,
    sort: FileSort,
    selected: HashSet<String>,
    /// Why the side could not be reached.
    error: Option<String>,
    history: Vec<String>,
    version: u64,
    /// The rows as last flattened, until something changes them.
    rows: Option<Rows>,
}

/// Whether `path` is `root` or inside it.
fn under(path: &str, root: &str) -> bool {
    root == "/" || path == root || path.starts_with(&format!("{root}/"))
}

/// Whether the pane shows this entry at all.
fn shows(p: &PaneState, item: &FileItem) -> bool {
    p.show_hidden || !item.name.starts_with('.')
}

/// A directory to read, and the tree its listing is for.
struct Listing {
    pane: Pane,
    dir: String,
    source: FileSource,
    generation: u64,
}

/// What became of the next copy once its destination was asked.
enum Decision {
    Start(Arc<TransferRun>),
    Skipped,
    Asked,
    Gone,
}

struct State {
    hosts: Vec<String>,
    panes: HashMap<Pane, PaneState>,
    homes: HashMap<FileSource, String>,
    /// The Go menu's places, by side and home directory. This machine's
    /// are looked for on disk, so once per window opened.
    shortcuts: HashMap<(FileSource, String), Vec<Shortcut>>,
    /// The listings running, by side.
    listings: HashMap<FileSource, usize>,
    seq: u64,
    transfers: Vec<TransferRecord>,
    next_id: u64,
    show_history: bool,
    notice: String,
    notice_is_error: bool,
    running: Option<Arc<TransferRun>>,
    question: Option<(u64, bool)>,
    replace_all: bool,
    skip_all: bool,
    /// True while the destination is being asked whether the name is taken.
    checking: bool,
}

pub struct FilesModel {
    pub host: String,
    state: Mutex<State>,
    app: OnceLock<AppHandle>,
    /// Whether a snapshot is on its way to the windows, and whether
    /// another was asked for meanwhile.
    sending: Mutex<(bool, bool)>,
}

/// Long enough to look back at, short enough to stay a list.
const HISTORY_LIMIT: usize = 200;

/// How many directories of one side are read at once. On a host each is
/// a session on its master, and sshd allows ten (MaxSessions), the
/// Console's terminals included; a refresh of a tree with many folders
/// open lets the rest wait their turn.
const LISTINGS_AT_ONCE: usize = 3;

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

static MODELS: Mutex<Option<HashMap<String, Arc<FilesModel>>>> = Mutex::new(None);

/// One model per host, kept while the app runs so a copy survives the
/// window being closed.
pub fn for_host(host: &str) -> Arc<FilesModel> {
    let mut guard = MODELS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    map.entry(host.to_string())
        .or_insert_with(|| Arc::new(FilesModel::new(host)))
        .clone()
}

pub fn all() -> Vec<Arc<FilesModel>> {
    MODELS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default()
}

impl FilesModel {
    fn new(host: &str) -> FilesModel {
        let saved = prefs::load().file_sorts;
        let mut panes = HashMap::new();
        for pane in Pane::ALL {
            let mut p = PaneState::default();
            if let Some(s) = saved.get(pane.key()).and_then(|s| FileSort::parse(s)) {
                p.sort = s;
            }
            panes.insert(pane, p);
        }
        FilesModel {
            host: host.to_string(),
            state: Mutex::new(State {
                hosts: vec![],
                panes,
                homes: HashMap::new(),
                shortcuts: HashMap::new(),
                listings: HashMap::new(),
                seq: 0,
                transfers: vec![],
                next_id: 0,
                show_history: true,
                notice: String::new(),
                notice_is_error: false,
                running: None,
                question: None,
                replace_all: false,
                skip_all: false,
                checking: false,
            }),
            app: OnceLock::new(),
            sending: Mutex::new((false, false)),
        }
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn event_name(&self) -> String {
        format!("files:{}", super::windows::files_label(&self.host))
    }

    /// The running copy's progress, apart from the snapshots: a bar moving
    /// is no reason to send both trees again.
    pub fn progress_event_name(&self) -> String {
        format!("files-progress:{}", super::windows::files_label(&self.host))
    }

    /// Send the windows the state as it is now. Asked for while one is on
    /// its way, it is sent once more after that one, with whatever changed
    /// meanwhile: a burst of changes makes a snapshot or two, not one
    /// each, and they leave in the order they were made.
    pub fn publish(&self) {
        let Some(app) = self.app.get() else { return };
        {
            let mut s = self.sending.lock().unwrap_or_else(|e| e.into_inner());
            if s.0 {
                s.1 = true;
                return;
            }
            *s = (true, false);
        }
        loop {
            let _ = app.emit(&self.event_name(), self.snapshot());
            let mut s = self.sending.lock().unwrap_or_else(|e| e.into_inner());
            if !s.1 {
                s.0 = false;
                return;
            }
            s.1 = false;
        }
    }

    /// Only published when the list really changed.
    pub fn set_hosts(&self, connected: Vec<String>) {
        {
            let mut st = self.st();
            if st.hosts == connected {
                return;
            }
            st.hosts = connected;
        }
        self.publish();
    }

    pub fn snapshot(&self) -> FilesSnapshot {
        let mut st = self.st();
        st.seq += 1;
        let mut panes = BTreeMap::new();
        for pane in Pane::ALL {
            let source = st.panes[&pane].source.clone().unwrap_or(FileSource::Local);
            let shortcuts = Self::shortcuts_of(&mut st, &source);
            let home = Self::home_of(&st, &source);
            let p = st.panes.get_mut(&pane).expect("pane");
            let rows = Self::rows_now(p);
            let selected = Self::selection_in(&rows, &p.selected);
            panes.insert(
                pane.key().to_string(),
                PaneView {
                    title: source.title(),
                    root: p.root.clone(),
                    error: p.error.clone().or_else(|| p.failed.get(&p.root).cloned()),
                    rows,
                    show_hidden: p.show_hidden,
                    sort: p.sort,
                    can_go_back: !p.history.is_empty(),
                    selected,
                    shortcuts,
                    home,
                    loading_root: p.loading.contains(&p.root)
                        || p.source.is_some() && p.root.is_empty() && p.error.is_none(),
                    version: p.version,
                    source,
                },
            );
        }
        let active = st
            .transfers
            .iter()
            .find(|t| t.state == TransferState::Running)
            .cloned();
        let question = st.question.and_then(|(id, more)| {
            st.transfers
                .iter()
                .find(|t| t.id == id)
                .map(|t| ReplaceQuestion {
                    id,
                    name: t.name.clone(),
                    destination: t.job.destination.clone(),
                    to_title: t.job.to.title(),
                    offer_all: more,
                })
        });
        let can_copy = Pane::ALL
            .iter()
            .map(|p| (p.key().to_string(), Self::can_copy_selection(&st, *p)))
            .collect();
        FilesSnapshot {
            host: self.host.clone(),
            hosts: st.hosts.clone(),
            panes,
            active,
            queued: st
                .transfers
                .iter()
                .filter(|t| t.state == TransferState::Waiting)
                .count(),
            last_finished: st.transfers.iter().rev().find(|t| t.is_over()).cloned(),
            notice: st.notice.clone(),
            notice_is_error: st.notice_is_error,
            show_history: st.show_history,
            question,
            can_copy,
            can_retry: st
                .transfers
                .iter()
                .filter(|t| t.can_retry())
                .map(|t| t.id)
                .collect(),
            transfers: st.transfers.clone(),
            seq: st.seq,
        }
    }

    // MARK: sides

    pub fn source(&self, pane: Pane) -> FileSource {
        self.st().panes[&pane]
            .source
            .clone()
            .unwrap_or(FileSource::Local)
    }

    pub fn root(&self, pane: Pane) -> String {
        self.st().panes[&pane].root.clone()
    }

    /// Opens the window: the host it was asked for on the left, this
    /// machine on the right.
    pub fn start(self: &Arc<Self>, app: &AppHandle) {
        let _ = self.app.set(app.clone());
        let fresh = {
            let mut st = self.st();
            // The places on this machine are looked for again.
            st.shortcuts.clear();
            st.panes[&Pane::Left].source.is_none()
        };
        if fresh {
            // The Console switched to this machine shows its own tree.
            let left = if ssh::is_local(&self.host) {
                FileSource::Local
            } else {
                FileSource::Host(self.host.clone())
            };
            self.set_source(Pane::Left, left);
            self.set_source(Pane::Right, FileSource::Local);
        } else {
            self.publish();
        }
    }

    /// Point a pane at another side and open it at that side's home
    /// directory — for a host, at ~/Shellder when it is there.
    pub fn set_source(self: &Arc<Self>, pane: Pane, source: FileSource) {
        let generation = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.source = Some(source.clone());
            p.root.clear();
            Self::forget_listings(p);
            p.expanded.clear();
            p.views.clear();
            p.error = None;
            p.selected.clear();
            p.history.clear();
            p.generation
        };
        self.publish();
        let me = self.clone();
        std::thread::spawn(move || {
            let found = fs::home(&source);
            if let Ok(home) = &found {
                me.st().homes.insert(source.clone(), home.clone());
            }
            // Pointed elsewhere, or sent to a folder, since: that stands.
            if me.st().panes[&pane].generation != generation {
                return;
            }
            match found {
                Ok(home) => {
                    let shelf = remotefs::join(&home, ssh::UPLOAD_DIRECTORY);
                    let start = if source.is_local() || !fs::is_directory(&source, &shelf) {
                        home
                    } else {
                        shelf
                    };
                    if me.st().panes[&pane].generation == generation {
                        me.set_root(pane, &start, true);
                    }
                }
                Err(e) => {
                    {
                        let mut st = me.st();
                        let p = st.panes.get_mut(&pane).expect("pane");
                        p.error = Some(e.0);
                        Self::touch(p);
                    }
                    me.publish();
                }
            }
        });
    }

    /// The home directory on one side: what the host said, or this
    /// machine's; `/` on a host that has not answered yet.
    fn home_of(st: &State, source: &FileSource) -> String {
        st.homes.get(source).cloned().unwrap_or_else(|| {
            if source.is_local() {
                crate::core::config::home().to_string()
            } else {
                "/".to_string()
            }
        })
    }

    /// The places the Go menu offers for this side, found once for each
    /// home directory it has.
    fn shortcuts_of(st: &mut State, source: &FileSource) -> Vec<Shortcut> {
        let key = (source.clone(), Self::home_of(st, source));
        if let Some(found) = st.shortcuts.get(&key) {
            return found.clone();
        }
        let made = Self::places(&key.0, &key.1);
        st.shortcuts.insert(key, made.clone());
        made
    }

    fn places(source: &FileSource, home: &str) -> Vec<Shortcut> {
        if source.is_local() {
            let mut out = vec![Shortcut {
                title: remotefs::last_component(home),
                path: home.to_string(),
            }];
            for (title, sub) in [
                ("Desktop", "Desktop"),
                ("Documents", "Documents"),
                ("Downloads", "Downloads"),
                ("Movies", "Movies"),
                ("Music", "Music"),
                ("Pictures", "Pictures"),
                ("Applications", "Applications"),
            ] {
                let path = format!("{home}/{sub}");
                if std::path::Path::new(&path).is_dir() {
                    out.push(Shortcut {
                        title: title.into(),
                        path,
                    });
                }
            }
            out.push(Shortcut {
                title: "/".into(),
                path: "/".into(),
            });
            return out;
        }
        vec![
            Shortcut {
                title: "Home".into(),
                path: home.to_string(),
            },
            Shortcut {
                title: ssh::UPLOAD_DIRECTORY.into(),
                path: remotefs::join(home, ssh::UPLOAD_DIRECTORY),
            },
            Shortcut {
                title: "/".into(),
                path: "/".into(),
            },
            Shortcut {
                title: "/tmp".into(),
                path: "/tmp".into(),
            },
            Shortcut {
                title: "/etc".into(),
                path: "/etc".into(),
            },
            Shortcut {
                title: "/var/log".into(),
                path: "/var/log".into(),
            },
        ]
    }

    /// The directories inside one on this pane's side, for the folder
    /// menu's tree: what the pane would list of it, minus the files.
    pub fn subdirectories(&self, pane: Pane, path: &str) -> Result<Vec<Shortcut>, String> {
        let (source, hidden) = {
            let st = self.st();
            let p = &st.panes[&pane];
            (p.source.clone().unwrap_or(FileSource::Local), p.show_hidden)
        };
        let mut dirs: Vec<FileItem> = fs::list(&source, path)
            .map_err(|e| e.0)?
            .into_iter()
            .filter(|i| i.is_dir && (hidden || !i.name.starts_with('.')))
            .collect();
        dirs.sort_by(|a, b| natural_compare(&a.name, &b.name));
        Ok(dirs
            .into_iter()
            .map(|i| Shortcut {
                title: i.name,
                path: i.path,
            })
            .collect())
    }

    // MARK: trees

    pub fn set_root(self: &Arc<Self>, pane: Pane, path: &str, remember: bool) {
        self.show_root(pane, path, remember, None);
    }

    /// Show `path` as the root, its tree as it was when this pane last
    /// left it: the directories open then are read again, each once its
    /// parent's listing has come. The view being left is kept the same
    /// way, and `remember` keeps its root for `back`; `select` is the row
    /// to be selected once it is there, the directory just left when
    /// coming back out of it.
    fn show_root(self: &Arc<Self>, pane: Pane, path: &str, remember: bool, select: Option<String>) {
        {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            if !p.root.is_empty() && p.root != path {
                let leaving = p.root.clone();
                let open = std::mem::take(&mut p.expanded);
                p.views.insert(leaving.clone(), open);
                if remember {
                    p.history.push(leaving);
                    if p.history.len() > 100 {
                        p.history.remove(0);
                    }
                }
            }
            p.root = path.to_string();
            Self::forget_listings(p);
            p.expanded = p.views.get(path).cloned().unwrap_or_default();
            p.error = None;
            p.selected = select.into_iter().collect();
        }
        self.load(pane, vec![path.to_string()]);
    }

    /// Back to the directory this pane was showing before, its folders
    /// open as they were and the directory just left selected.
    pub fn back(self: &Arc<Self>, pane: Pane) {
        let (previous, left) = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            (p.history.pop(), p.root.clone())
        };
        if let Some(root) = previous {
            self.show_root(pane, &root, false, Some(left));
        }
    }

    /// The directory above the root becomes the root, the one just left
    /// selected in it: the way out of a tree.
    pub fn up(self: &Arc<Self>, pane: Pane) {
        let root = self.root(pane);
        if let Some(parent) = remotefs::parent(&root) {
            self.show_root(pane, &parent, true, Some(root));
        }
    }

    /// Read the root and every open directory in view again, keeping the
    /// shape. What was read inside a shut directory is let go, to be read
    /// afresh when it shows again.
    pub fn reload(self: &Arc<Self>, pane: Pane) {
        let dirs = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            let open = Self::open_in_view(p);
            let keep: HashSet<&String> = open.iter().chain([&p.root]).collect();
            let stale: Vec<String> = p
                .listed
                .keys()
                .filter(|d| !keep.contains(d))
                .cloned()
                .collect();
            for d in stale {
                Self::forget_listing(p, &d);
            }
            std::iter::once(p.root.clone()).chain(open).collect()
        };
        self.load(pane, dirs);
    }

    /// Entries whose name starts with a dot in or out of the rows. They
    /// were read with the rest: only what is open among them and was
    /// never read is read now.
    pub fn toggle_hidden(self: &Arc<Self>, pane: Pane) {
        let dirs = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.show_hidden = !p.show_hidden;
            Self::touch(p);
            Self::unread_in_view(p)
        };
        self.load(pane, dirs);
    }

    pub fn sort(&self, pane: Pane) -> FileSort {
        self.st().panes[&pane].sort
    }

    /// Order the pane another way. What has been read is sorted again where
    /// it is, open directories included: nothing is listed again for it.
    pub fn set_sort(&self, pane: Pane, sort: FileSort) {
        {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            if p.sort == sort {
                return;
            }
            p.sort = sort;
            let _ = prefs::update(|pr| {
                pr.file_sorts.insert(pane.key().to_string(), sort.saved());
            });
            let PaneState { listed, items, .. } = p;
            for kids in listed.values_mut() {
                sort.order(kids, items);
            }
            Self::touch(p);
        }
        self.publish();
    }

    /// Fold a directory open or shut, reading it the first time it opens.
    pub fn toggle(self: &Arc<Self>, pane: Pane, path: &str) {
        let (is_dir, open) = {
            let st = self.st();
            let p = &st.panes[&pane];
            (
                p.items.get(path).is_some_and(|i| i.is_dir),
                p.expanded.contains(path),
            )
        };
        if !is_dir {
            return;
        }
        if open {
            self.collapse(pane, path);
        } else {
            self.expand(pane, path);
        }
    }

    /// Open a directory. Read already, it shows what it had, and what is
    /// open inside it and was never read is read now; otherwise it is
    /// read.
    pub fn expand(self: &Arc<Self>, pane: Pane, path: &str) {
        let dirs = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            if !p.items.get(path).is_some_and(|i| i.is_dir) {
                return;
            }
            p.expanded.insert(path.to_string());
            Self::touch(p);
            Self::unread_in_view(p)
        };
        self.load(pane, dirs);
    }

    pub fn collapse(&self, pane: Pane, path: &str) {
        {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.expanded.remove(path);
            Self::touch(p);
        }
        self.publish();
    }

    /// Shut a directory and every directory inside it.
    pub fn collapse_all(&self, pane: Pane, path: &str) {
        {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.expanded.retain(|d| !under(d, path));
            Self::touch(p);
        }
        self.publish();
    }

    /// What the pane shows has changed, its selection aside: the rows are
    /// flattened again when next asked for, and a window draws the pane
    /// again.
    fn touch(p: &mut PaneState) {
        p.version += 1;
        p.rows = None;
    }

    /// The pane shows another tree from now on: what was read, and what
    /// is being read, belong to the one before.
    fn forget_listings(p: &mut PaneState) {
        p.generation += 1;
        p.items.clear();
        p.listed.clear();
        p.failed.clear();
        p.loading.clear();
        p.waiting.clear();
        p.reading.clear();
        p.again.clear();
        Self::touch(p);
    }

    /// Let go of what was read in `dir` and in everything under it.
    fn forget_listing(p: &mut PaneState, dir: &str) {
        if let Some(kids) = p.listed.remove(dir) {
            for k in kids {
                Self::forget_listing(p, &k);
                p.items.remove(&k);
                p.failed.remove(&k);
            }
        }
    }

    /// Read these directories, a few of a side at a time.
    fn load(self: &Arc<Self>, pane: Pane, dirs: Vec<String>) {
        let started = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            for d in &dirs {
                Self::ask(p, d);
            }
            Self::start_listings(&mut st)
        };
        self.publish();
        for l in started {
            self.read(l);
        }
    }

    /// Ask for `dir` to be read. One being read is read once more when
    /// that listing lands, since what changed meanwhile may not be in it;
    /// one waiting for its turn will be read as it is then anyway.
    fn ask(p: &mut PaneState, dir: &str) {
        if dir.is_empty() {
            return;
        }
        if p.reading.contains(dir) {
            p.again.insert(dir.to_string());
        } else if p.loading.insert(dir.to_string()) {
            p.waiting.push_back(dir.to_string());
            Self::touch(p);
        }
    }

    /// Start the listings that wait, as many as each side allows, the two
    /// panes taking turns.
    fn start_listings(st: &mut State) -> Vec<Listing> {
        let State {
            panes, listings, ..
        } = st;
        let mut out = vec![];
        loop {
            let mut started = false;
            for pane in Pane::ALL {
                let p = panes.get_mut(&pane).expect("pane");
                let source = p.source.clone().unwrap_or(FileSource::Local);
                let running = listings.entry(source.clone()).or_insert(0);
                if *running >= LISTINGS_AT_ONCE {
                    continue;
                }
                let Some(dir) = p.waiting.pop_front() else {
                    continue;
                };
                *running += 1;
                p.reading.insert(dir.clone());
                out.push(Listing {
                    pane,
                    dir,
                    source,
                    generation: p.generation,
                });
                started = true;
            }
            if !started {
                return out;
            }
        }
    }

    fn read(self: &Arc<Self>, l: Listing) {
        let me = self.clone();
        std::thread::spawn(move || {
            let listed = fs::list(&l.source, &l.dir);
            me.listed(l, listed);
        });
    }

    /// A listing landed: into the tree it goes, unless the pane has moved
    /// to another tree since it was asked for. A directory that could not
    /// be read keeps what it showed before, if anything, and says why on
    /// its row (or the pane's, for the root); it is not taken as read, so
    /// opening it again tries again.
    fn listed(self: &Arc<Self>, l: Listing, listed: Result<Vec<FileItem>, FsError>) {
        let (fresh, started) = {
            let mut st = self.st();
            if let Some(n) = st.listings.get_mut(&l.source) {
                *n = n.saturating_sub(1);
            }
            let p = st.panes.get_mut(&l.pane).expect("pane");
            let fresh = p.generation == l.generation;
            if fresh {
                p.reading.remove(&l.dir);
                p.loading.remove(&l.dir);
                match listed {
                    Ok(items) => {
                        Self::place(p, items, &l.dir);
                        p.failed.remove(&l.dir);
                    }
                    Err(e) => {
                        log::warn(format!(
                            "{}: {} {}: {}",
                            self.host,
                            l.source.id(),
                            l.dir,
                            e.0
                        ));
                        p.failed.insert(l.dir.clone(), e.0);
                    }
                }
                if p.again.remove(&l.dir) {
                    Self::ask(p, &l.dir);
                }
                // A directory in it that is to be open (a view coming
                // back) and has not been read is read now that its parent
                // is in the tree; read earlier, its listing would have had
                // nowhere to go.
                for d in Self::unread_open_children(p, &l.dir) {
                    Self::ask(p, &d);
                }
                Self::touch(p);
            }
            (fresh, Self::start_listings(&mut st))
        };
        if fresh {
            self.publish();
        }
        for l in started {
            self.read(l);
        }
    }

    /// The directories right under `dir` that are marked open but have
    /// not been read, when `dir` itself shows open.
    fn unread_open_children(p: &PaneState, dir: &str) -> Vec<String> {
        if dir != p.root && !p.expanded.contains(dir) {
            return vec![];
        }
        p.listed
            .get(dir)
            .into_iter()
            .flatten()
            .filter(|path| {
                p.items.get(*path).is_some_and(|i| i.is_dir && shows(p, i))
                    && p.expanded.contains(*path)
                    && !p.listed.contains_key(*path)
                    && !p.loading.contains(*path)
            })
            .cloned()
            .collect()
    }

    /// The open directories that show, read or not, in the order of the
    /// rows.
    fn open_in_view(p: &PaneState) -> Vec<String> {
        fn walk(p: &PaneState, dir: &str, out: &mut Vec<String>) {
            for path in p.listed.get(dir).into_iter().flatten() {
                let open = p.expanded.contains(path)
                    && p.items.get(path).is_some_and(|i| i.is_dir && shows(p, i));
                if open {
                    out.push(path.clone());
                    walk(p, path, out);
                }
            }
        }
        let mut out = vec![];
        walk(p, &p.root, &mut out);
        out
    }

    /// The open directories that show and have not been read.
    fn unread_in_view(p: &PaneState) -> Vec<String> {
        Self::open_in_view(p)
            .into_iter()
            .filter(|d| !p.listed.contains_key(d) && !p.loading.contains(d))
            .collect()
    }

    /// Put a listing into the tree, in the pane's order. A directory that
    /// was there before keeps what was read in it, so its open folders
    /// stay open; what is gone, or is no longer a directory, takes what
    /// was read under it along.
    fn place(p: &mut PaneState, items: Vec<FileItem>, dir: &str) {
        let fresh: HashMap<&str, bool> =
            items.iter().map(|i| (i.path.as_str(), i.is_dir)).collect();
        for old in p.listed.remove(dir).unwrap_or_default() {
            let was_dir = p.items.get(&old).is_some_and(|i| i.is_dir);
            match fresh.get(old.as_str()) {
                Some(&is_dir) if is_dir == was_dir => {}
                Some(_) => Self::forget_listing(p, &old),
                None => {
                    Self::forget_listing(p, &old);
                    p.items.remove(&old);
                    p.failed.remove(&old);
                }
            }
        }
        let mut kids: Vec<String> = items.iter().map(|i| i.path.clone()).collect();
        for item in items {
            p.items.insert(item.path.clone(), item);
        }
        p.sort.order(&mut kids, &p.items);
        p.listed.insert(dir.to_string(), kids);
    }

    /// The lines a pane shows right now, flattened: what the view draws.
    fn rows_of(p: &PaneState) -> Vec<Row> {
        fn walk(p: &PaneState, dir: &str, depth: usize, out: &mut Vec<Row>) {
            for path in p.listed.get(dir).into_iter().flatten() {
                let Some(item) = p.items.get(path) else {
                    continue;
                };
                if !shows(p, item) {
                    continue;
                }
                let expanded = p.expanded.contains(path);
                out.push(Row {
                    item: item.clone(),
                    depth,
                    expanded,
                    loading: p.loading.contains(path),
                    error: p.failed.get(path).cloned(),
                });
                if expanded {
                    walk(p, path, depth + 1, out);
                }
            }
        }
        let mut out = vec![];
        walk(p, &p.root, 0, &mut out);
        out
    }

    /// The rows, flattened once for every change.
    fn rows_now(p: &mut PaneState) -> Rows {
        if p.rows.is_none() {
            p.rows = Some(Rows(Arc::new(Self::rows_of(p))));
        }
        p.rows.clone().unwrap_or_default()
    }

    pub fn rows(&self, pane: Pane) -> Vec<Row> {
        Self::rows_now(self.st().panes.get_mut(&pane).expect("pane")).to_vec()
    }

    /// Where a drop on this item goes: into a directory, or into the
    /// directory a file sits in. The root and the directories read are
    /// directories too, though not items of the tree (the root never is):
    /// an upload into the folder shown went into its parent.
    fn destination_in(p: &PaneState, path: &str) -> String {
        if path == p.root || p.listed.contains_key(path) {
            return path.to_string();
        }
        match p.items.get(path) {
            Some(i) if i.is_dir => path.to_string(),
            _ => remotefs::parent(path).unwrap_or_else(|| p.root.clone()),
        }
    }

    pub fn destination(&self, pane: Pane, path: &str) -> String {
        Self::destination_in(&self.st().panes[&pane], path)
    }

    /// What is selected among `rows`, in the order they are drawn.
    fn selection_in(rows: &[Row], selected: &HashSet<String>) -> Vec<String> {
        if selected.is_empty() {
            return vec![];
        }
        rows.iter()
            .filter(|r| selected.contains(&r.item.path))
            .map(|r| r.item.path.clone())
            .collect()
    }

    fn selection_of(p: &mut PaneState) -> Vec<String> {
        let rows = Self::rows_now(p);
        Self::selection_in(&rows, &p.selected)
    }

    pub fn selection(&self, pane: Pane) -> Vec<String> {
        Self::selection_of(self.st().panes.get_mut(&pane).expect("pane"))
    }

    pub fn set_selected(&self, pane: Pane, paths: Vec<String>) {
        self.st().panes.get_mut(&pane).expect("pane").selected = paths.into_iter().collect();
        self.publish();
    }

    /// Where a new folder or file goes: into what is selected when that is
    /// a directory, into the directory around it when it is a file, and
    /// into the pane's root when nothing is selected.
    pub fn target(&self, pane: Pane) -> String {
        let mut st = self.st();
        let p = st.panes.get_mut(&pane).expect("pane");
        match Self::selection_of(p).first() {
            Some(path) if p.items.contains_key(path) => Self::destination_in(p, path),
            _ => p.root.clone(),
        }
    }

    // MARK: making and deleting

    pub fn create(self: &Arc<Self>, pane: Pane, name: &str, directory: bool) {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() || trimmed.contains('/') {
            self.report("A name cannot be empty or contain a slash.", true);
            return;
        }
        let source = self.source(pane);
        let parent = self.target(pane);
        let path = remotefs::join(&parent, &trimmed);
        let me = self.clone();
        std::thread::spawn(move || {
            let failed = if directory {
                fs::make_directory(&source, &path)
            } else {
                fs::make_file(&source, &path)
            };
            match failed {
                Err(e) => {
                    me.report(&e.0, true);
                    log::error(format!("{}: {}: {}", me.host, source.id(), e.0));
                }
                Ok(()) => {
                    me.report(&format!("{trimmed} → {parent}"), false);
                    log::info(format!("{}: {}: created {path}", me.host, source.id()));
                    me.refresh(&parent, &source);
                }
            }
        });
    }

    /// Delete what these paths name. On this machine they go to the Trash,
    /// on a host they are gone — the window says which before asking.
    pub fn delete(self: &Arc<Self>, pane: Pane, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        let source = self.source(pane);
        let root = self.root(pane);
        let parents: HashSet<String> = paths
            .iter()
            .map(|p| remotefs::parent(p).unwrap_or_else(|| root.clone()))
            .collect();
        let name = remotefs::last_component(&paths[0]);
        let me = self.clone();
        std::thread::spawn(move || {
            let mut failures = vec![];
            let mut gone = vec![];
            for path in &paths {
                match fs::remove(&source, path) {
                    Err(e) => failures.push(e),
                    Ok(()) => gone.push(path.clone()),
                }
            }
            {
                let mut st = me.st();
                let p = st.panes.get_mut(&pane).expect("pane");
                for path in &gone {
                    log::info(format!("{}: {}: deleted {path}", me.host, source.id()));
                    p.selected.remove(path);
                    p.expanded.remove(path);
                }
                Self::touch(p);
            }
            for parent in parents {
                me.refresh(&parent, &source);
            }
            if let Some(f) = failures.first() {
                me.report(&f.0, true);
                log::error(format!("{}: {}: {}", me.host, source.id(), f.0));
            } else if paths.len() == 1 {
                me.report(
                    &if source.is_local() {
                        format!("{name} moved to the Trash")
                    } else {
                        format!("{name} deleted")
                    },
                    false,
                );
            } else {
                me.report(
                    &if source.is_local() {
                        format!("{} items moved to the Trash", paths.len())
                    } else {
                        format!("{} items deleted", paths.len())
                    },
                    false,
                );
            }
        });
    }

    fn report(&self, text: &str, is_error: bool) {
        {
            let mut st = self.st();
            st.notice = text.to_string();
            st.notice_is_error = is_error;
        }
        self.publish();
    }

    // MARK: drag and drop

    /// What a pane would do with rows dragged from the other side: copy
    /// them over. Rows from its own side mean nothing.
    pub fn can_accept(&self, dragged: &[(FileSource, String)], pane: Pane) -> bool {
        let to = self.source(pane);
        !dragged.is_empty() && dragged.iter().any(|(s, _)| *s != to)
    }

    /// A drop on `directory` of one pane: every row of the drag is queued.
    pub fn accept(&self, dragged: Vec<(FileSource, String)>, directory: &str, pane: Pane) -> bool {
        if !self.can_accept(&dragged, pane) {
            return false;
        }
        let to = self.source(pane);
        for (from, path) in dragged.into_iter().filter(|(s, _)| *s != to) {
            self.enqueue(TransferJob {
                from,
                to: to.clone(),
                source: path,
                destination: directory.to_string(),
                resume: false,
            });
        }
        true
    }

    /// Files dragged in from the desktop are copied to a host.
    pub fn accept_local_files(&self, paths: Vec<String>, directory: &str, pane: Pane) -> bool {
        let to = self.source(pane);
        if to.is_local() || paths.is_empty() {
            return false;
        }
        for p in paths {
            self.enqueue(TransferJob {
                from: FileSource::Local,
                to: to.clone(),
                source: p,
                destination: directory.to_string(),
                resume: false,
            });
        }
        true
    }

    // MARK: copies

    fn can_copy_selection(st: &State, pane: Pane) -> bool {
        let p = &st.panes[&pane];
        let o = &st.panes[&pane.other()];
        !p.selected.is_empty() && p.source != o.source && !o.root.is_empty()
    }

    /// What one pane has selected, copied into what the other pane shows.
    pub fn copy_selection(&self, pane: Pane) {
        let jobs: Vec<TransferJob> = {
            let mut st = self.st();
            if !Self::can_copy_selection(&st, pane) {
                return;
            }
            let (to, destination) = {
                let o = &st.panes[&pane.other()];
                (
                    o.source.clone().unwrap_or(FileSource::Local),
                    o.root.clone(),
                )
            };
            let p = st.panes.get_mut(&pane).expect("pane");
            let from = p.source.clone().unwrap_or(FileSource::Local);
            Self::selection_of(p)
                .into_iter()
                .map(|path| TransferJob {
                    from: from.clone(),
                    to: to.clone(),
                    source: path,
                    destination: destination.clone(),
                    resume: false,
                })
                .collect()
        };
        for j in jobs {
            self.enqueue(j);
        }
    }

    pub fn enqueue(&self, job: TransferJob) {
        {
            let mut st = self.st();
            st.notice.clear();
            st.next_id += 1;
            let id = st.next_id;
            st.transfers.push(TransferRecord {
                id,
                name: job.name(),
                is_upload: job.is_upload(),
                can_resume: job.can_resume(),
                job,
                state: TransferState::Waiting,
                progress: None,
                total: None,
                queued_at: now(),
                ended_at: None,
            });
            // The oldest of those that are over go first; one still to
            // run, or running, stays however long the list.
            while st.transfers.len() > HISTORY_LIMIT {
                match st.transfers.iter().position(|t| t.is_over()) {
                    Some(i) => {
                        st.transfers.remove(i);
                    }
                    None => break,
                }
            }
        }
        self.next();
    }

    /// Stop the copy that is running. What has already landed stays, and
    /// the next one waiting starts.
    pub fn cancel(&self) {
        if let Some(r) = self.st().running.clone() {
            r.cancel();
        }
    }

    /// Take one copy off the queue before it starts. The one the window
    /// is asking about counts as waiting: the question goes with it.
    pub fn cancel_waiting(&self, id: u64) {
        let asked = {
            let mut st = self.st();
            let Some(r) = st
                .transfers
                .iter_mut()
                .find(|t| t.id == id && t.state == TransferState::Waiting)
            else {
                return;
            };
            r.call_off();
            log::info(format!("{}: {} taken off the queue", self.host, r.name));
            let asked = st.question.is_some_and(|(q, _)| q == id);
            if asked {
                st.question = None;
            }
            Self::settle(&mut st);
            asked
        };
        self.publish();
        if asked {
            self.next();
        }
    }

    /// Take every copy still waiting off the queue; the one running goes
    /// on.
    pub fn cancel_all_waiting(&self) {
        {
            let mut st = self.st();
            let mut n = 0;
            for r in st
                .transfers
                .iter_mut()
                .filter(|t| t.state == TransferState::Waiting)
            {
                r.call_off();
                n += 1;
            }
            if n == 0 {
                return;
            }
            log::info(format!("{}: {n} copies taken off the queue", self.host));
            st.question = None;
            Self::settle(&mut st);
        }
        self.publish();
    }

    /// Stop the copy running and every one waiting: nothing starts after.
    pub fn cancel_all(&self) {
        self.cancel_all_waiting();
        self.cancel();
    }

    /// Forget the copies that are over; the ones still to run stay.
    pub fn clear_history(&self) {
        self.st().transfers.retain(|t| !t.is_over());
        self.publish();
    }

    /// Copy this one again, at the end of the queue. One that was stopped
    /// or failed goes on from what has landed; one that was skipped is a
    /// fresh copy and gets its question again.
    pub fn retry(&self, id: u64) {
        let job = self
            .st()
            .transfers
            .iter()
            .find(|t| t.id == id && t.can_retry())
            .map(|t| {
                let mut j = t.job.clone();
                j.resume = t.can_resume && t.state != TransferState::Skipped;
                j
            });
        if let Some(j) = job {
            self.enqueue(j);
        }
    }

    pub fn set_show_history(&self, on: bool) {
        self.st().show_history = on;
        self.publish();
    }

    /// The next copy, once the destination has been asked whether the name
    /// is taken — a copy writes over what is there without a word, so the
    /// window asks first. A copy going on from what landed is the one case
    /// where something being there is the point, so it is not asked.
    fn next(&self) {
        let job = {
            let mut st = self.st();
            let active = st
                .transfers
                .iter()
                .any(|t| t.state == TransferState::Running);
            if active || st.checking || st.question.is_some() {
                return;
            }
            let Some(t) = st
                .transfers
                .iter()
                .find(|t| t.state == TransferState::Waiting)
                .map(|t| (t.id, t.job.clone()))
            else {
                return;
            };
            st.checking = true;
            t
        };
        self.publish();
        let me = self.me();
        std::thread::spawn(move || {
            let (id, job) = job;
            let taken = !job.resume && fs::exists(&job.to, &job.landing());
            // Decided and marked in one go: a copy queued meanwhile looks
            // for the next one waiting, and must not find this one still
            // waiting and start it a second time.
            let decision = {
                let mut st = me.st();
                st.checking = false;
                if !st
                    .transfers
                    .iter()
                    .any(|t| t.id == id && t.state == TransferState::Waiting)
                {
                    Decision::Gone
                } else if !taken || job.resume || st.replace_all {
                    Decision::Start(Self::begin(&mut st, id))
                } else if st.skip_all {
                    Self::mark_skipped(&mut st, id);
                    Decision::Skipped
                } else {
                    let more = st
                        .transfers
                        .iter()
                        .any(|t| t.id != id && t.state == TransferState::Waiting);
                    st.question = Some((id, more));
                    Decision::Asked
                }
            };
            match decision {
                Decision::Start(run) => me.launch(id, job, run),
                Decision::Skipped => me.skipped(&job),
                Decision::Asked | Decision::Gone => {
                    me.publish();
                    if me.st().question.is_none() {
                        me.next();
                    }
                }
            }
        });
    }

    fn me(&self) -> Arc<FilesModel> {
        for_host(&self.host)
    }

    /// The window answered the question about a name that is taken.
    pub fn answer_replace(&self, answer: ReplaceAnswer) {
        let decided = {
            let mut st = self.st();
            let Some((id, _)) = st.question.take() else {
                return;
            };
            match answer {
                ReplaceAnswer::ReplaceAll => st.replace_all = true,
                ReplaceAnswer::SkipAll => st.skip_all = true,
                _ => {}
            }
            st.transfers
                .iter()
                .find(|t| t.id == id && t.state == TransferState::Waiting)
                .map(|t| t.job.clone())
                .map(|job| match answer {
                    ReplaceAnswer::Replace | ReplaceAnswer::ReplaceAll => {
                        (id, job, Some(Self::begin(&mut st, id)))
                    }
                    ReplaceAnswer::Skip | ReplaceAnswer::SkipAll => {
                        Self::mark_skipped(&mut st, id);
                        (id, job, None)
                    }
                })
        };
        match decided {
            Some((id, job, Some(run))) => self.launch(id, job, run),
            Some((_, job, None)) => self.skipped(&job),
            None => self.next(),
        }
    }

    /// The copy `id` is the one running from now on: marked so under the
    /// same lock that found it waiting.
    fn begin(st: &mut State, id: u64) -> Arc<TransferRun> {
        let run = TransferRun::new();
        st.running = Some(run.clone());
        if let Some(r) = st.transfers.iter_mut().find(|t| t.id == id) {
            r.state = TransferState::Running;
            r.progress = None;
        }
        run
    }

    fn mark_skipped(st: &mut State, id: u64) {
        if let Some(r) = st.transfers.iter_mut().find(|t| t.id == id) {
            r.state = TransferState::Skipped;
            r.ended_at = Some(now());
        }
    }

    /// A copy the user did not want to write over anything.
    fn skipped(&self, job: &TransferJob) {
        log::info(format!(
            "{}: {} left alone, {} is already there",
            self.host,
            job.name(),
            job.landing()
        ));
        Self::settle(&mut self.st());
        self.publish();
        self.next();
    }

    fn launch(&self, id: u64, job: TransferJob, run: Arc<TransferRun>) {
        log::info(format!(
            "{}: {} {} {} → {} {}",
            self.host,
            if job.resume { "continuing" } else { "copying" },
            job.from.id(),
            job.source,
            job.to.id(),
            job.destination
        ));
        self.publish();
        let me = self.me();
        std::thread::spawn(move || {
            let total = TransferRun::total(&job);
            me.update(id, |r| r.total = total);
            let m2 = me.clone();
            let result = run.run(&job, total, move |fraction| m2.progress(id, fraction));
            me.finished(id, &job, &run, result);
        });
    }

    fn update(&self, id: u64, change: impl FnOnce(&mut TransferRecord)) {
        {
            let mut st = self.st();
            if let Some(r) = st.transfers.iter_mut().find(|t| t.id == id) {
                change(r);
            }
        }
        self.publish();
    }

    /// The running copy got further: kept for the next snapshot, and told
    /// the windows on its own.
    fn progress(&self, id: u64, fraction: f64) {
        {
            let mut st = self.st();
            match st
                .transfers
                .iter_mut()
                .find(|t| t.id == id && t.state == TransferState::Running)
            {
                Some(r) => r.progress = Some(fraction),
                None => return,
            }
        }
        if let Some(app) = self.app.get() {
            let _ = app.emit(
                &self.progress_event_name(),
                TransferProgress {
                    id,
                    progress: fraction,
                },
            );
        }
    }

    fn finished(&self, id: u64, job: &TransferJob, run: &TransferRun, result: ssh::Output) {
        self.st().running = None;
        let mut body = result.text();
        if body.len() > 2000 {
            let cut = body.len() - 2000;
            let cut = (cut..body.len())
                .find(|&i| body.is_char_boundary(i))
                .unwrap_or(0);
            body = body[cut..].to_string();
        }
        for line in body.lines() {
            log::ssh(&self.host, line);
        }
        let cancelled = run.is_cancelled();
        self.update(id, |r| {
            r.progress = None;
            r.ended_at = Some(now());
            r.state = if cancelled {
                TransferState::Cancelled
            } else if result.status == 0 {
                TransferState::Done
            } else {
                // sftp says it in a line or two; one line is what there is room for.
                let said = body
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                TransferState::Failed {
                    why: if said.is_empty() {
                        format!(
                            "The copy exited with status {}. See the log for details.",
                            result.status
                        )
                    } else {
                        said
                    },
                }
            };
        });
        if cancelled {
            log::info(format!("{}: copy of {} cancelled", self.host, job.name()));
        } else if result.status == 0 {
            log::info(format!(
                "{}: copied {} to {}",
                self.host,
                job.name(),
                job.destination
            ));
        } else {
            log::error(format!(
                "{}: copy of {} failed with status {}",
                self.host,
                job.name(),
                result.status
            ));
        }
        // However it ended: scp leaves what it had already written, so the
        // pane shows what is really there.
        self.refresh(&job.destination, &job.to);
        Self::settle(&mut self.st());
        self.next();
    }

    /// "Apply to all" lasts as long as the queue it was answered for.
    fn settle(st: &mut State) {
        if !st
            .transfers
            .iter()
            .any(|t| t.state == TransferState::Waiting)
        {
            st.replace_all = false;
            st.skip_all = false;
        }
    }

    /// Show what just changed: read that directory again in every pane
    /// looking at it.
    fn refresh(&self, directory: &str, source: &FileSource) {
        let me = self.me();
        for pane in Pane::ALL {
            let wants = {
                let st = self.st();
                let p = &st.panes[&pane];
                p.source.as_ref() == Some(source)
                    && (directory == p.root || p.listed.contains_key(directory))
            };
            if wants {
                me.load(pane, vec![directory.to_string()]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        use std::cmp::Ordering::*;
        assert_eq!(natural_compare("file2", "file10"), Less);
        assert_eq!(natural_compare("A", "b"), Less);
        assert_eq!(natural_compare("b", "A"), Greater);
        assert_eq!(natural_compare("a", "a"), Equal);
    }

    #[test]
    fn sort_forms() {
        let s = FileSort::parse("modified descending").unwrap();
        assert_eq!(
            s,
            FileSort {
                key: SortKey::Modified,
                ascending: false
            }
        );
        assert_eq!(s.saved(), "modified descending");
        assert!(FileSort::parse("nope").is_none());
    }
}
