//! The two trees of a Files window and the copies dragged between them.
//! Each pane looks at its own side — this machine or any host, through its
//! master when one is up and through a session of its own otherwise — so
//! the same window also copies between two hosts. Listings and copies run
//! on their own threads; every change is pushed to the window as one
//! snapshot.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::core::log;
use crate::core::prefs;
use crate::core::remotefs::{self, fs, FileItem};
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

    pub fn ordered(&self, mut items: Vec<Node>) -> Vec<Node> {
        items.sort_by(|a, b| self.compare(&a.item, &b.item));
        items
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

/// One entry as the pane holds it, with its children once that directory
/// has been read.
#[derive(Debug, Clone)]
pub struct Node {
    pub item: FileItem,
    /// None until the directory has been read.
    pub children: Option<Vec<Node>>,
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
    pub rows: Vec<Row>,
    pub show_hidden: bool,
    pub sort: FileSort,
    pub can_go_back: bool,
    pub selected: Vec<String>,
    pub shortcuts: Vec<Shortcut>,
    /// The home directory on this side, for the folder menu's button.
    pub home: String,
    pub loading_root: bool,
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
}

#[derive(Default)]
struct PaneState {
    source: Option<FileSource>,
    root: String,
    tree: Vec<Node>,
    nodes: HashMap<String, Node>,
    expanded: HashSet<String>,
    loading: HashSet<String>,
    /// For every root shown so far, which directories under it were open
    /// when it was last left: a directory shown again starts from that.
    views: HashMap<String, HashSet<String>>,
    show_hidden: bool,
    sort: FileSort,
    selected: HashSet<String>,
    error: Option<String>,
    history: Vec<String>,
}

/// Whether `path` is `root` or inside it.
fn under(path: &str, root: &str) -> bool {
    root == "/" || path == root || path.starts_with(&format!("{root}/"))
}

struct State {
    hosts: Vec<String>,
    panes: HashMap<Pane, PaneState>,
    homes: HashMap<FileSource, String>,
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
}

/// Long enough to look back at, short enough to stay a list.
const HISTORY_LIMIT: usize = 200;

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
        }
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn event_name(&self) -> String {
        format!("files:{}", super::windows::files_label(&self.host))
    }

    pub fn publish(&self) {
        if let Some(app) = self.app.get() {
            let _ = app.emit(&self.event_name(), self.snapshot());
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
        let st = self.st();
        let mut panes = BTreeMap::new();
        for pane in Pane::ALL {
            let p = &st.panes[&pane];
            let source = p.source.clone().unwrap_or(FileSource::Local);
            panes.insert(
                pane.key().to_string(),
                PaneView {
                    title: source.title(),
                    root: p.root.clone(),
                    error: p.error.clone(),
                    rows: Self::rows_of(p),
                    show_hidden: p.show_hidden,
                    sort: p.sort,
                    can_go_back: !p.history.is_empty(),
                    selected: Self::selection_of(p),
                    shortcuts: self.shortcuts_of(&st, pane),
                    home: Self::home_of(&st, &source),
                    loading_root: p.loading.contains(&p.root)
                        || p.source.is_some() && p.root.is_empty() && p.error.is_none(),
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
        let fresh = self.st().panes[&Pane::Left].source.is_none();
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
        {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.source = Some(source.clone());
            p.root.clear();
            p.tree.clear();
            p.nodes.clear();
            p.expanded.clear();
            p.views.clear();
            p.error = None;
            p.selected.clear();
            p.history.clear();
        }
        self.publish();
        let me = self.clone();
        std::thread::spawn(move || {
            let found = fs::home(&source);
            if me.source(pane) != source {
                return;
            }
            match found {
                Ok(home) => {
                    me.st().homes.insert(source.clone(), home.clone());
                    let shelf = remotefs::join(&home, ssh::UPLOAD_DIRECTORY);
                    let start = if source.is_local() || !fs::is_directory(&source, &shelf) {
                        home
                    } else {
                        shelf
                    };
                    me.set_root(pane, &start, true);
                }
                Err(e) => {
                    me.st().panes.get_mut(&pane).expect("pane").error = Some(e.0);
                    me.publish();
                }
            }
        });
    }

    /// The places the Go menu offers for this pane's side.
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

    fn shortcuts_of(&self, st: &State, pane: Pane) -> Vec<Shortcut> {
        let source = st.panes[&pane].source.clone().unwrap_or(FileSource::Local);
        let home = Self::home_of(st, &source);
        if source.is_local() {
            let mut out = vec![Shortcut {
                title: remotefs::last_component(&home),
                path: home.clone(),
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
                path: home.clone(),
            },
            Shortcut {
                title: ssh::UPLOAD_DIRECTORY.into(),
                path: remotefs::join(&home, ssh::UPLOAD_DIRECTORY),
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
            p.tree.clear();
            p.nodes.clear();
            p.expanded = p.views.get(path).cloned().unwrap_or_default();
            p.error = None;
            p.selected = select.into_iter().collect();
        }
        self.publish();
        self.load(pane, path.to_string());
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

    /// Read the root and every open directory again, keeping the shape.
    pub fn reload(self: &Arc<Self>, pane: Pane) {
        let (root, open) = {
            let st = self.st();
            let p = &st.panes[&pane];
            (
                p.root.clone(),
                p.expanded.iter().cloned().collect::<Vec<_>>(),
            )
        };
        self.load(pane, root);
        for dir in open {
            self.load(pane, dir);
        }
    }

    pub fn toggle_hidden(self: &Arc<Self>, pane: Pane) {
        {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.show_hidden = !p.show_hidden;
        }
        self.reload(pane);
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
            let tree = std::mem::take(&mut p.tree);
            p.tree = sort.ordered(tree);
            Self::resort_children(&mut p.tree, &sort);
        }
        self.publish();
    }

    fn resort_children(nodes: &mut [Node], sort: &FileSort) {
        for n in nodes.iter_mut() {
            if let Some(kids) = n.children.take() {
                let mut kids = sort.ordered(kids);
                Self::resort_children(&mut kids, sort);
                n.children = Some(kids);
            }
        }
    }

    /// Fold a directory open or shut, reading it the first time it opens.
    pub fn toggle(self: &Arc<Self>, pane: Pane, path: &str) {
        let (is_dir, open) = {
            let st = self.st();
            let p = &st.panes[&pane];
            (
                p.nodes.get(path).map(|n| n.item.is_dir).unwrap_or(false),
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

    pub fn expand(self: &Arc<Self>, pane: Pane, path: &str) {
        let read = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            p.expanded.insert(path.to_string());
            p.nodes
                .get(path)
                .map(|n| n.children.is_some())
                .unwrap_or(false)
        };
        if read {
            self.publish();
        } else {
            self.load(pane, path.to_string());
        }
    }

    pub fn collapse(&self, pane: Pane, path: &str) {
        self.st()
            .panes
            .get_mut(&pane)
            .expect("pane")
            .expanded
            .remove(path);
        self.publish();
    }

    /// Shut a directory and every directory inside it.
    pub fn collapse_all(&self, pane: Pane, path: &str) {
        self.st()
            .panes
            .get_mut(&pane)
            .expect("pane")
            .expanded
            .retain(|d| !under(d, path));
        self.publish();
    }

    fn load(self: &Arc<Self>, pane: Pane, dir: String) {
        let source = {
            let mut st = self.st();
            let p = st.panes.get_mut(&pane).expect("pane");
            if dir.is_empty() || p.loading.contains(&dir) {
                return;
            }
            p.loading.insert(dir.clone());
            p.source.clone().unwrap_or(FileSource::Local)
        };
        self.publish();
        let me = self.clone();
        std::thread::spawn(move || {
            let listed = fs::list(&source, &dir);
            let to_load = {
                let mut st = me.st();
                let p = st.panes.get_mut(&pane).expect("pane");
                p.loading.remove(&dir);
                if p.source.as_ref() != Some(&source) {
                    return;
                }
                match listed {
                    Ok(items) => {
                        Self::place(p, items, &dir);
                        if dir == p.root {
                            p.error = None;
                        }
                    }
                    Err(e) => {
                        Self::place(p, vec![], &dir);
                        p.error = Some(e.0.clone());
                        log::warn(format!("{}: {} {dir}: {}", me.host, source.id(), e.0));
                    }
                }
                // A directory in it that is to be open (a view coming back)
                // and has not been read is read now that its parent is in
                // the tree; read earlier, its listing would have had nowhere
                // to go.
                Self::unread_open_children(p, &dir)
            };
            me.publish();
            for d in to_load {
                me.load(pane, d);
            }
        });
    }

    /// The directories right under `dir` that are marked open but have
    /// not been read.
    fn unread_open_children(p: &PaneState, dir: &str) -> Vec<String> {
        let kids = if dir == p.root {
            Some(&p.tree)
        } else {
            p.nodes.get(dir).and_then(|n| n.children.as_ref())
        };
        kids.map(|kids| {
            kids.iter()
                .filter(|k| {
                    k.item.is_dir
                        && k.children.is_none()
                        && p.expanded.contains(&k.item.path)
                        && !p.loading.contains(&k.item.path)
                })
                .map(|k| k.item.path.clone())
                .collect()
        })
        .unwrap_or_default()
    }

    /// Put a listing into the tree, in the pane's order, keeping the nodes
    /// of entries that were there before so their open directories stay open.
    fn place(p: &mut PaneState, items: Vec<FileItem>, dir: &str) {
        let hidden = p.show_hidden;
        let kept: Vec<Node> = items
            .into_iter()
            .filter(|i| hidden || !i.name.starts_with('.'))
            .map(|item| match p.nodes.get(&item.path) {
                Some(old) if old.item.is_dir == item.is_dir => Node {
                    item,
                    children: old.children.clone(),
                },
                _ => Node {
                    item,
                    children: None,
                },
            })
            .collect();
        let kept = p.sort.ordered(kept);
        for n in &kept {
            p.nodes.insert(n.item.path.clone(), n.clone());
        }
        if dir == p.root {
            p.tree = kept;
        } else {
            Self::set_children(&mut p.tree, dir, kept.clone());
            if let Some(n) = p.nodes.get_mut(dir) {
                n.children = Some(kept);
            }
        }
    }

    fn set_children(nodes: &mut [Node], dir: &str, kids: Vec<Node>) -> bool {
        for n in nodes.iter_mut() {
            if n.item.path == dir {
                n.children = Some(kids);
                return true;
            }
            if let Some(c) = n.children.as_mut() {
                if Self::set_children(c, dir, kids.clone()) {
                    return true;
                }
            }
        }
        false
    }

    /// The lines a pane shows right now, flattened: what the view draws.
    fn rows_of(p: &PaneState) -> Vec<Row> {
        let mut out = vec![];
        fn walk(list: &[Node], depth: usize, p: &PaneState, out: &mut Vec<Row>) {
            for n in list {
                let expanded = p.expanded.contains(&n.item.path);
                out.push(Row {
                    item: n.item.clone(),
                    depth,
                    expanded,
                    loading: p.loading.contains(&n.item.path),
                });
                if expanded {
                    if let Some(kids) = &n.children {
                        walk(kids, depth + 1, p, out);
                    }
                }
            }
        }
        walk(&p.tree, 0, p, &mut out);
        out
    }

    pub fn rows(&self, pane: Pane) -> Vec<Row> {
        Self::rows_of(&self.st().panes[&pane])
    }

    /// Where a drop on this item goes: into a directory, or into the
    /// directory a file sits in.
    fn destination_in(p: &PaneState, path: &str) -> String {
        match p.nodes.get(path) {
            Some(n) if n.item.is_dir => path.to_string(),
            _ => remotefs::parent(path).unwrap_or_else(|| p.root.clone()),
        }
    }

    pub fn destination(&self, pane: Pane, path: &str) -> String {
        Self::destination_in(&self.st().panes[&pane], path)
    }

    /// What a pane has selected, in the order the rows are drawn.
    fn selection_of(p: &PaneState) -> Vec<String> {
        if p.selected.is_empty() {
            return vec![];
        }
        Self::rows_of(p)
            .into_iter()
            .map(|r| r.item.path)
            .filter(|path| p.selected.contains(path))
            .collect()
    }

    pub fn selection(&self, pane: Pane) -> Vec<String> {
        Self::selection_of(&self.st().panes[&pane])
    }

    pub fn set_selected(&self, pane: Pane, paths: Vec<String>) {
        self.st().panes.get_mut(&pane).expect("pane").selected = paths.into_iter().collect();
        self.publish();
    }

    /// Where a new folder or file goes: into what is selected when that is
    /// a directory, into the directory around it when it is a file, and
    /// into the pane's root when nothing is selected.
    pub fn target(&self, pane: Pane) -> String {
        let st = self.st();
        let p = &st.panes[&pane];
        match Self::selection_of(p).first() {
            Some(path) if p.nodes.contains_key(path) => Self::destination_in(p, path),
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
            let st = self.st();
            if !Self::can_copy_selection(&st, pane) {
                return;
            }
            let p = &st.panes[&pane];
            let o = &st.panes[&pane.other()];
            Self::selection_of(p)
                .into_iter()
                .map(|path| TransferJob {
                    from: p.source.clone().unwrap_or(FileSource::Local),
                    to: o.source.clone().unwrap_or(FileSource::Local),
                    source: path,
                    destination: o.root.clone(),
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
            if st.transfers.len() > HISTORY_LIMIT {
                let extra = st.transfers.len() - HISTORY_LIMIT;
                st.transfers.drain(..extra);
            }
        }
        self.next();
    }

    /// Stop the copy that is running. What has already landed stays.
    pub fn cancel(&self) {
        if let Some(r) = self.st().running.clone() {
            r.cancel();
        }
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
            let decision = {
                let mut st = me.st();
                st.checking = false;
                if !st
                    .transfers
                    .iter()
                    .any(|t| t.id == id && t.state == TransferState::Waiting)
                {
                    None
                } else if !taken || job.resume || st.replace_all {
                    Some(true)
                } else if st.skip_all {
                    Some(false)
                } else {
                    let more = st
                        .transfers
                        .iter()
                        .any(|t| t.id != id && t.state == TransferState::Waiting);
                    st.question = Some((id, more));
                    None
                }
            };
            match decision {
                Some(true) => me.start_job(id, job),
                Some(false) => me.skip(id, &job),
                None => {
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
        let pending = {
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
                .find(|t| t.id == id)
                .map(|t| (id, t.job.clone()))
        };
        let Some((id, job)) = pending else { return };
        match answer {
            ReplaceAnswer::Replace | ReplaceAnswer::ReplaceAll => self.start_job(id, job),
            ReplaceAnswer::Skip | ReplaceAnswer::SkipAll => self.skip(id, &job),
        }
    }

    /// A copy the user did not want to write over anything.
    fn skip(&self, id: u64, job: &TransferJob) {
        self.update(id, |r| {
            r.state = TransferState::Skipped;
            r.ended_at = Some(now());
        });
        log::info(format!(
            "{}: {} left alone, {} is already there",
            self.host,
            job.name(),
            job.landing()
        ));
        self.settle_queue();
        self.next();
    }

    fn start_job(&self, id: u64, job: TransferJob) {
        let run = TransferRun::new();
        {
            let mut st = self.st();
            st.running = Some(run.clone());
        }
        self.update(id, |r| {
            r.state = TransferState::Running;
            r.progress = None;
        });
        log::info(format!(
            "{}: {} {} {} → {} {}",
            self.host,
            if job.resume { "continuing" } else { "copying" },
            job.from.id(),
            job.source,
            job.to.id(),
            job.destination
        ));
        let me = self.me();
        std::thread::spawn(move || {
            let total = TransferRun::total(&job);
            me.update(id, |r| r.total = total);
            let m2 = me.clone();
            let result = run.run(&job, total, move |fraction| {
                m2.update(id, |r| r.progress = Some(fraction))
            });
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
        self.settle_queue();
        self.next();
    }

    /// "Apply to all" lasts as long as the queue it was answered for.
    fn settle_queue(&self) {
        let mut st = self.st();
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
                    && (directory == p.root
                        || p.nodes
                            .get(directory)
                            .map(|n| n.children.is_some())
                            .unwrap_or(false))
            };
            if wants {
                me.load(pane, directory.to_string());
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
