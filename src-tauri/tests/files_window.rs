//! The Files window's model driven the way the panes do, against the two
//! Docker hosts Tests/files-window.sh brings up (ignored unless it does).
//! The first host lists with BusyBox's ls, the second with GNU's, and this
//! machine's own ls stands in for the BSDs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use shellder::app::files::{
    self, FileSort, FilesModel, Pane, ReplaceAnswer, SortKey, TransferState,
};
use shellder::core::remotefs::{self, local, remote};
use shellder::core::ssh::FileSource;

struct Checks(Vec<String>);

impl Checks {
    fn check(&mut self, what: &str, ok: bool, detail: impl std::fmt::Display) {
        let d = detail.to_string();
        println!(
            "{} {what}{}",
            if ok { "ok  " } else { "FAIL" },
            if d.is_empty() {
                String::new()
            } else {
                format!(": {d}")
            }
        );
        if !ok {
            self.0.push(what.to_string());
        }
    }
}

fn pump(seconds: f64) {
    std::thread::sleep(Duration::from_secs_f64(seconds));
}

fn names(m: &FilesModel, pane: Pane) -> Vec<String> {
    m.rows(pane).into_iter().map(|r| r.item.name).collect()
}

fn active(m: &FilesModel) -> Option<files::TransferRecord> {
    m.snapshot().active
}

fn last_state(m: &FilesModel) -> Option<TransferState> {
    m.snapshot().transfers.last().map(|t| t.state.clone())
}

/// Wait for a drop to turn into a copy and for that copy to end.
fn settle(m: &FilesModel) -> Vec<f64> {
    let t = Instant::now();
    while active(m).is_none() && t.elapsed() < Duration::from_secs(5) {
        pump(0.1);
    }
    let mut seen = vec![];
    while let Some(a) = active(m) {
        pump(0.25);
        if let Some(p) = a.progress {
            if seen.last() != Some(&p) {
                seen.push(p);
            }
        }
    }
    pump(0.5);
    seen
}

/// Wait for the whole queue, including copies that never start because the
/// answer to the question was Skip, answering every question with `answer`.
fn settle_all(m: &FilesModel, answer: ReplaceAnswer, asked: &mut u32) {
    let t = Instant::now();
    loop {
        let s = m.snapshot();
        if let Some(q) = &s.question {
            *asked += 1;
            m.answer_replace(q.id, answer);
        }
        let busy = s.active.is_some()
            || s.transfers
                .iter()
                .any(|t| t.state == TransferState::Waiting)
            || s.question.is_some();
        if !busy || t.elapsed() > Duration::from_secs(60) {
            break;
        }
        pump(0.2);
    }
    pump(0.6);
}

/// Wait for a new folder, a new file or a delete to report back.
fn settle_notice(m: &FilesModel) -> (String, bool) {
    let before = m.snapshot().notice;
    let t = Instant::now();
    while m.snapshot().notice == before && t.elapsed() < Duration::from_secs(5) {
        pump(0.1);
    }
    pump(0.6);
    let s = m.snapshot();
    (s.notice, s.notice_is_error)
}

fn utc(s: &str) -> f64 {
    chrono_parse(s)
}

fn chrono_parse(s: &str) -> f64 {
    // "2024-01-02T03:04:05Z"
    let (date, time) = s.trim_end_matches('Z').split_once('T').unwrap();
    let d: Vec<i64> = date.split('-').map(|x| x.parse().unwrap()).collect();
    let t: Vec<i64> = time.split(':').map(|x| x.parse().unwrap()).collect();
    let days = days_from_civil(d[0], d[1], d[2]);
    (days * 86400 + t[0] * 3600 + t[1] * 60 + t[2]) as f64
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn item(m: &FilesModel, pane: Pane, name: &str) -> Option<remotefs::FileItem> {
    m.rows(pane)
        .into_iter()
        .map(|r| r.item)
        .find(|i| i.name == name)
}

fn dragged(rows: &[(&FileSource, &str)]) -> Vec<(FileSource, String)> {
    rows.iter()
        .map(|(s, p)| ((*s).clone(), p.to_string()))
        .collect()
}

#[test]
#[ignore]
fn the_files_model_does_what_the_panes_ask() {
    let dir = std::env::var("SHELLDER_FILES_TEST_DIR").expect("run through Tests/files-window.sh");
    let here = format!("{dir}/here");
    let host_a = FileSource::Host("shjt-files".into());
    let host_b = FileSource::Host("shjt-files-b".into());
    let local_src = FileSource::Local;
    let mut c = Checks(vec![]);

    // A sort left over from an earlier run would reorder every pane below.
    let _ = shellder::core::prefs::update(|p| p.file_sorts.clear());
    let model: Arc<FilesModel> = files::for_host("shjt-files");
    model.set_hosts(vec!["shjt-files".into()]);
    model.set_source(Pane::Left, host_a.clone());
    model.set_source(Pane::Right, FileSource::Local);
    pump(5.0);

    // MARK: the trees
    c.check(
        "the host opens on the left, in ~/Shellder",
        model.source(Pane::Left) == host_a && model.root(Pane::Left).ends_with("/Shellder"),
        model.root(Pane::Left),
    );
    c.check(
        "this machine opens on the right",
        model.source(Pane::Right).is_local()
            && model.root(Pane::Right) == shellder::core::config::home(),
        model.root(Pane::Right),
    );
    c.check(
        "the machine menu offers what is connected",
        model.snapshot().hosts == ["shjt-files"],
        "",
    );
    model.set_hosts(vec!["shjt-files".into(), "shjt-files-b".into()]);
    c.check(
        "and picks up a host connected while the window is open",
        model.snapshot().hosts == ["shjt-files", "shjt-files-b"],
        "",
    );
    c.check(
        "it lists what is there",
        names(&model, Pane::Left) == ["sub", "big.bin", "one.txt"],
        format!("{:?}", names(&model, Pane::Left)),
    );
    c.check(
        "dot entries are out of the way",
        !names(&model, Pane::Left).contains(&".hidden".to_string()),
        "",
    );
    model.toggle_hidden(Pane::Left);
    pump(1.0);
    c.check(
        "until the pane is asked for them",
        names(&model, Pane::Left).contains(&".hidden".to_string()),
        format!("{:?}", names(&model, Pane::Left)),
    );
    model.toggle_hidden(Pane::Left);
    pump(1.0);

    if let Some(sub) = item(&model, Pane::Left, "sub") {
        model.toggle(Pane::Left, &sub.path);
        pump(2.0);
        c.check(
            "a directory opens under its own line",
            names(&model, Pane::Left) == ["sub", "two.txt", "big.bin", "one.txt"],
            format!("{:?}", names(&model, Pane::Left)),
        );
        model.toggle(Pane::Left, &sub.path);
        pump(0.3);
        c.check(
            "and folds shut again",
            names(&model, Pane::Left) == ["sub", "big.bin", "one.txt"],
            "",
        );
    }

    model.up(Pane::Left);
    pump(2.0);
    c.check(
        "up one level reaches the home directory",
        model.root(Pane::Left) == "/home/t",
        model.root(Pane::Left),
    );
    c.check(
        "and there is a way back",
        model.snapshot().panes["left"].can_go_back,
        "",
    );
    model.back(Pane::Left);
    pump(2.0);
    c.check(
        "back is where it was before, not the parent",
        model.root(Pane::Left) == "/home/t/Shellder" && !model.snapshot().panes["left"].can_go_back,
        model.root(Pane::Left),
    );
    model.up(Pane::Left);
    pump(2.0);
    c.check(
        "a symlinked directory is listed as one",
        names(&model, Pane::Left).contains(&"link".to_string()),
        format!("{:?}", names(&model, Pane::Left)),
    );
    let shortcuts: Vec<String> = model.snapshot().panes["left"]
        .shortcuts
        .iter()
        .map(|s| s.path.clone())
        .collect();
    c.check(
        "the Go menu offers the home directory and ~/Shellder",
        shortcuts.contains(&"/home/t".to_string())
            && shortcuts.contains(&"/home/t/Shellder".to_string()),
        format!("{shortcuts:?}"),
    );
    let mine: Vec<String> = model.snapshot().panes["right"]
        .shortcuts
        .iter()
        .map(|s| s.path.clone())
        .collect();
    c.check(
        "and this machine's own places",
        mine.contains(&shellder::core::config::home().to_string()),
        format!("{mine:?}"),
    );

    model.set_root(Pane::Right, &here, true);
    pump(1.0);
    c.check(
        "a pane goes where it is told",
        model.root(Pane::Right) == here && names(&model, Pane::Right) == ["clash.txt", "two.txt"],
        format!(
            "{} {:?}",
            model.root(Pane::Right),
            names(&model, Pane::Right)
        ),
    );

    // MARK: copies
    model.accept(
        dragged(&[(&host_a, "/home/t/Shellder")]),
        &here,
        Pane::Right,
    );
    let down = settle(&model);
    c.check(
        "dragging a directory over copies it here",
        last_state(&model) == Some(TransferState::Done),
        format!("{:?}", last_state(&model)),
    );
    c.check(
        "the pane shows what arrived",
        names(&model, Pane::Right).contains(&"Shellder".to_string()),
        format!("{:?}", names(&model, Pane::Right)),
    );
    c.check(
        "with progress along the way",
        !down.is_empty() && down.iter().all(|p| *p > 0.0 && *p < 1.0),
        format!("{down:?}"),
    );
    c.check(
        "and every byte of it",
        Some(local::bytes(&format!("{here}/Shellder")))
            == remote::bytes("shjt-files", "/home/t/Shellder"),
        "",
    );

    model.accept(
        dragged(&[(&local_src, &format!("{here}/Shellder"))]),
        "/home/t/my dir",
        Pane::Left,
    );
    settle(&model);
    c.check(
        "dragging it back copies it to the host",
        last_state(&model) == Some(TransferState::Done),
        "",
    );
    c.check(
        "into a directory whose name has a space",
        remote::bytes("shjt-files", "/home/t/my dir/Shellder")
            == Some(local::bytes(&format!("{here}/Shellder"))),
        "",
    );

    model.accept_local_files(
        vec![format!("{here}/Shellder/one.txt")],
        "/home/t/my dir",
        Pane::Left,
    );
    settle(&model);
    c.check(
        "a file dragged in from the desktop uploads too",
        remote::bytes("shjt-files", "/home/t/my dir/one.txt") == Some(4),
        "",
    );

    // MARK: the other pane can be another host
    model.set_source(Pane::Right, host_b.clone());
    pump(4.0);
    c.check(
        "a pane can be pointed at another host",
        model.source(Pane::Right) == host_b && model.root(Pane::Right) == "/home/t/Shellder",
        model.root(Pane::Right),
    );
    model.accept(
        dragged(&[(&host_a, "/home/t/Shellder/one.txt")]),
        "/home/t/Shellder",
        Pane::Right,
    );
    settle(&model);
    c.check(
        "and a copy between two hosts goes through",
        last_state(&model) == Some(TransferState::Done),
        format!("{:?}", last_state(&model)),
    );
    c.check(
        "landing on the second host",
        remote::bytes("shjt-files-b", "/home/t/Shellder/one.txt") == Some(4),
        "",
    );
    c.check(
        "the same host on both sides refuses the drag",
        !model.can_accept(&dragged(&[(&host_b, "/home/t")]), Pane::Right),
        "",
    );

    // MARK: new folders, new files, deleting
    model.set_selected(Pane::Right, vec![]);
    model.create(Pane::Right, "made here", true);
    let (n, _) = settle_notice(&model);
    c.check(
        "a new folder on a host reports where it went",
        n.contains("made here"),
        &n,
    );
    c.check(
        "and shows up in the pane",
        names(&model, Pane::Right).contains(&"made here".to_string()),
        format!("{:?}", names(&model, Pane::Right)),
    );
    model.create(Pane::Right, "note.txt", false);
    settle_notice(&model);
    c.check(
        "a new file lands in the same place",
        names(&model, Pane::Right).contains(&"note.txt".to_string()),
        "",
    );
    model.create(Pane::Right, "note.txt", false);
    let (n, e) = settle_notice(&model);
    c.check("a name that is taken is refused", e, &n);
    model.create(Pane::Right, "bad/name", true);
    c.check(
        "and so is a name with a slash in it",
        model.snapshot().notice_is_error,
        "",
    );

    model.delete(Pane::Right, vec!["/home/t/Shellder/made here".into()]);
    let (n, e) = settle_notice(&model);
    c.check(
        "deleting on a host says it is gone",
        !e && n.contains("made here"),
        &n,
    );
    c.check(
        "and it is",
        !names(&model, Pane::Right).contains(&"made here".to_string()),
        "",
    );

    model.set_source(Pane::Right, FileSource::Local);
    pump(2.0);
    model.set_root(Pane::Right, &here, true);
    pump(1.5);
    model.create(Pane::Right, "made on this machine", true);
    settle_notice(&model);
    c.check(
        "a new folder on this machine too",
        names(&model, Pane::Right).contains(&"made on this machine".to_string()),
        "",
    );
    model.delete(Pane::Right, vec![format!("{here}/made on this machine")]);
    let (n, _) = settle_notice(&model);
    c.check(
        "and deleting it here means the Trash",
        n.contains("Trash")
            && !std::path::Path::new(&format!("{here}/made on this machine")).exists(),
        &n,
    );

    // MARK: the queue, stopping a copy, and what the list keeps
    model.accept(
        dragged(&[(&host_a, "/home/t/Shellder/big.bin")]),
        &here,
        Pane::Right,
    );
    model.accept(
        dragged(&[(&host_a, "/home/t/Shellder/one.txt")]),
        &here,
        Pane::Right,
    );
    let t = Instant::now();
    while active(&model).is_none() && t.elapsed() < Duration::from_secs(5) {
        pump(0.05);
    }
    c.check(
        "a second drop waits for the first",
        model.snapshot().queued == 1,
        format!("queued {}", model.snapshot().queued),
    );
    while active(&model).is_some() {
        pump(0.2);
    }
    pump(0.5);
    c.check(
        "and both of them land",
        names(&model, Pane::Right).contains(&"big.bin".to_string())
            && names(&model, Pane::Right).contains(&"one.txt".to_string()),
        "",
    );

    model.accept(
        dragged(&[(&host_a, "/home/t/huge.bin")]),
        &here,
        Pane::Right,
    );
    let t = Instant::now();
    while active(&model).is_none() && t.elapsed() < Duration::from_secs(5) {
        pump(0.05);
    }
    model.cancel();
    while active(&model).is_some() {
        pump(0.1);
    }
    c.check(
        "a copy can be stopped in the middle",
        last_state(&model) == Some(TransferState::Cancelled),
        format!("{:?}", last_state(&model)),
    );
    let partial = local::bytes(&format!("{here}/huge.bin"));
    c.check(
        "and it really stopped",
        partial < 600 * 1024 * 1024,
        format!("{partial} bytes of 600 MB"),
    );
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(&here)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    let shown: std::collections::BTreeSet<String> =
        names(&model, Pane::Right).into_iter().collect();
    c.check(
        "and the pane shows what is really there, half a file or none",
        shown == on_disk,
        format!("pane {shown:?} disk {on_disk:?}"),
    );

    model.accept(
        dragged(&[(&local_src, &format!("{here}/Shellder/one.txt"))]),
        "/etc",
        Pane::Left,
    );
    settle(&model);
    match last_state(&model) {
        Some(TransferState::Failed { why }) => c.check(
            "a refused copy keeps what sftp said",
            why.to_lowercase().contains("permission"),
            &why,
        ),
        other => c.check(
            "a refused copy keeps what sftp said",
            false,
            format!("{other:?}"),
        ),
    }
    let states: Vec<TransferState> = model
        .snapshot()
        .transfers
        .iter()
        .map(|t| t.state.clone())
        .collect();
    c.check(
        "the list kept every copy, in order",
        states.len() == 8
            && states.iter().filter(|s| **s == TransferState::Done).count() == 6
            && states.contains(&TransferState::Cancelled)
            && states
                .iter()
                .any(|s| matches!(s, TransferState::Failed { .. })),
        format!("{states:?}"),
    );
    let first = model.snapshot().transfers.first().cloned();
    c.check(
        "a finished copy remembers its size and when it ended",
        first
            .as_ref()
            .map(|f| f.total.unwrap_or(0) > 0 && f.ended_at.is_some())
            .unwrap_or(false),
        "",
    );
    model.clear_history();
    c.check(
        "clearing the list empties it",
        model.snapshot().transfers.is_empty(),
        "",
    );

    // MARK: taking copies off the queue
    let queue_dir = format!("{here}/queue");
    std::fs::create_dir_all(&queue_dir).unwrap();
    model.accept(
        dragged(&[
            (&host_a, "/home/t/huge.bin"),
            (&host_a, "/home/t/Shellder/one.txt"),
            (&host_a, "/home/t/Shellder/sub"),
        ]),
        &queue_dir,
        Pane::Right,
    );
    let t = Instant::now();
    while active(&model).is_none() && t.elapsed() < Duration::from_secs(5) {
        pump(0.05);
    }
    let ids: Vec<u64> = model.snapshot().transfers.iter().map(|t| t.id).collect();
    model.cancel_waiting(ids[1]);
    c.check(
        "a copy still waiting comes off the queue",
        model.snapshot().transfers[1].state == TransferState::Cancelled
            && model.snapshot().queued == 1
            && active(&model).map(|a| a.id) == Some(ids[0]),
        format!("{:?}", model.snapshot().transfers),
    );
    model.cancel_all_waiting();
    c.check(
        "and so do all the rest, the running one going on",
        model.snapshot().queued == 0 && active(&model).map(|a| a.id) == Some(ids[0]),
        format!("{:?}", model.snapshot().transfers),
    );
    model.cancel();
    while active(&model).is_some() {
        pump(0.1);
    }
    pump(0.5);
    let states: Vec<TransferState> = model
        .snapshot()
        .transfers
        .iter()
        .map(|t| t.state.clone())
        .collect();
    c.check(
        "stopping the running one starts nothing after it",
        states.iter().all(|s| *s == TransferState::Cancelled)
            && !std::path::Path::new(&format!("{queue_dir}/one.txt")).exists()
            && !std::path::Path::new(&format!("{queue_dir}/sub")).exists(),
        format!("{states:?}"),
    );
    c.check(
        "one taken off before it started is asked for again afresh",
        !model.snapshot().transfers[1].can_resume && model.snapshot().can_retry.contains(&ids[1]),
        "",
    );
    model.clear_history();
    let _ = std::fs::remove_dir_all(&queue_dir);

    // MARK: a name that is already taken, and asking for a copy again
    let text = |p: &str| {
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    model.set_root(Pane::Right, &here, true);
    pump(1.5);
    model.clear_history();
    let mut asked = 0;
    model.accept(
        dragged(&[(&host_a, "/home/t/clash.txt")]),
        &here,
        Pane::Right,
    );
    settle_all(&model, ReplaceAnswer::Skip, &mut asked);
    c.check(
        "a copy that would write over something asks first",
        asked == 1,
        format!("asked {asked} times"),
    );
    c.check(
        "and Skip leaves what was there alone",
        last_state(&model) == Some(TransferState::Skipped)
            && text(&format!("{here}/clash.txt")) == "local",
        format!(
            "{:?} {}",
            last_state(&model),
            text(&format!("{here}/clash.txt"))
        ),
    );

    let skipped = model.snapshot().transfers.last().unwrap().id;
    asked = 0;
    model.retry(skipped);
    settle_all(&model, ReplaceAnswer::Replace, &mut asked);
    c.check(
        "a copy can be asked for again",
        model.snapshot().transfers.len() == 2 && last_state(&model) == Some(TransferState::Done),
        format!(
            "{:?}",
            model
                .snapshot()
                .transfers
                .iter()
                .map(|t| t.state.clone())
                .collect::<Vec<_>>()
        ),
    );
    c.check(
        "and Replace writes over it",
        text(&format!("{here}/clash.txt")) == "remote",
        text(&format!("{here}/clash.txt")),
    );

    // one answer for a whole queue
    std::fs::write(format!("{here}/clash.txt"), "local").unwrap();
    asked = 0;
    model.accept(
        dragged(&[(&host_a, "/home/t/clash.txt"), (&host_a, "/home/t/two.txt")]),
        &here,
        Pane::Right,
    );
    settle_all(&model, ReplaceAnswer::ReplaceAll, &mut asked);
    c.check(
        "apply to all answers for the rest of the queue",
        asked == 1,
        format!("asked {asked} times"),
    );
    c.check(
        "and both of them went over the old ones",
        text(&format!("{here}/clash.txt")) == "remote"
            && text(&format!("{here}/two.txt")) == "remote",
        "",
    );
    model.clear_history();

    // MARK: more than one at a time
    model.set_root(Pane::Left, "/home/t/Shellder", true);
    pump(2.0);
    model.set_root(Pane::Right, &here, true);
    pump(1.5);
    // one.txt landed here earlier: its copy asks, and is told to replace.
    asked = 0;
    model.accept(
        dragged(&[
            (&host_a, "/home/t/Shellder/one.txt"),
            (&host_a, "/home/t/Shellder/sub"),
        ]),
        &here,
        Pane::Right,
    );
    settle_all(&model, ReplaceAnswer::Replace, &mut asked);
    let last_two: Vec<TransferState> = model
        .snapshot()
        .transfers
        .iter()
        .rev()
        .take(2)
        .map(|t| t.state.clone())
        .collect();
    c.check(
        "a drag of two rows queues both",
        last_two.iter().all(|s| *s == TransferState::Done)
            && names(&model, Pane::Right).contains(&"sub".to_string())
            && names(&model, Pane::Right).contains(&"one.txt".to_string()),
        format!("{:?}", names(&model, Pane::Right)),
    );

    model.set_selected(
        Pane::Left,
        vec![
            "/home/t/Shellder/one.txt".into(),
            "/home/t/Shellder/big.bin".into(),
        ],
    );
    c.check(
        "what is selected comes back in the order the rows are drawn",
        model.selection(Pane::Left) == ["/home/t/Shellder/big.bin", "/home/t/Shellder/one.txt"],
        format!("{:?}", model.selection(Pane::Left)),
    );
    c.check(
        "and the copy arrow is ready for it",
        model.snapshot().can_copy["left"],
        "",
    );

    let picked: Vec<String> = model
        .rows(Pane::Right)
        .into_iter()
        .filter(|r| r.item.name == "one.txt" || r.item.name == "sub")
        .map(|r| r.item.path)
        .collect();
    model.set_selected(Pane::Right, picked.clone());
    c.check(
        "two rows can be selected at once",
        model.selection(Pane::Right).len() == 2,
        format!("{:?}", model.selection(Pane::Right)),
    );
    model.delete(Pane::Right, model.selection(Pane::Right));
    let (n, _) = settle_notice(&model);
    c.check("deleting several says how many", n.contains("2 items"), &n);
    c.check(
        "and they are all gone",
        !names(&model, Pane::Right).contains(&"sub".to_string())
            && !names(&model, Pane::Right).contains(&"one.txt".to_string()),
        "",
    );

    // MARK: drops that mean nothing, and where one lands
    c.check(
        "a pane refuses a drag from its own side",
        !model.can_accept(&dragged(&[(&local_src, &here)]), Pane::Right)
            && model.can_accept(&dragged(&[(&local_src, &here)]), Pane::Left),
        "",
    );
    c.check(
        "a file from the desktop is for a host's side only",
        !model.accept_local_files(vec![here.clone()], &here, Pane::Right),
        "",
    );
    c.check(
        "a drop on a directory goes into it",
        model.destination(Pane::Left, "/home/t/Shellder/sub") == "/home/t/Shellder/sub",
        "",
    );
    c.check(
        "a drop on a file goes into the directory around it",
        model.destination(Pane::Left, "/home/t/Shellder/one.txt") == "/home/t/Shellder",
        "",
    );

    model.set_root(Pane::Left, "/home/t/nope", true);
    pump(2.0);
    c.check(
        "a directory that is not there shows the error",
        model.snapshot().panes["left"]
            .error
            .clone()
            .unwrap_or_default()
            .contains("No such file"),
        model.snapshot().panes["left"]
            .error
            .clone()
            .unwrap_or_default(),
    );

    // MARK: dates, and the order of the rows
    let sorter = files::for_host("shjt-files-b");
    sorter.set_hosts(vec!["shjt-files".into(), "shjt-files-b".into()]);
    sorter.set_source(Pane::Left, host_a.clone());
    sorter.set_source(Pane::Right, host_b.clone());
    pump(4.0);
    for (pane, ls) in [(Pane::Left, "BusyBox"), (Pane::Right, "GNU")] {
        sorter.set_root(pane, "/home/t/sorted", true);
        pump(2.0);
        if let Some(adir) = item(&sorter, pane, "adir") {
            sorter.expand(pane, &adir.path);
        }
        pump(2.0);
        c.check(
            &format!("{ls}: the rows come by name, directories first"),
            names(&sorter, pane)
                == [
                    "adir", "a.txt", "b.txt", "zdir", "mid.bin", "new.txt", "old.txt",
                ],
            format!("{:?}", names(&sorter, pane)),
        );
        c.check(
            &format!("{ls}: a file's date to the second"),
            item(&sorter, pane, "old.txt").and_then(|i| i.modified)
                == Some(utc("2024-01-02T03:04:05Z")),
            format!(
                "{:?}",
                item(&sorter, pane, "old.txt").and_then(|i| i.modified)
            ),
        );
        c.check(
            &format!("{ls}: and a directory's own"),
            item(&sorter, pane, "zdir").and_then(|i| i.modified)
                == Some(utc("2025-12-01T00:00:00Z")),
            "",
        );
        for (sort, expected) in [
            (
                FileSort {
                    key: SortKey::Name,
                    ascending: false,
                },
                vec![
                    "zdir", "adir", "b.txt", "a.txt", "old.txt", "new.txt", "mid.bin",
                ],
            ),
            (
                FileSort {
                    key: SortKey::Modified,
                    ascending: false,
                },
                vec![
                    "zdir", "new.txt", "mid.bin", "old.txt", "adir", "b.txt", "a.txt",
                ],
            ),
            (
                FileSort {
                    key: SortKey::Modified,
                    ascending: true,
                },
                vec![
                    "adir", "a.txt", "b.txt", "old.txt", "mid.bin", "new.txt", "zdir",
                ],
            ),
            (
                FileSort {
                    key: SortKey::Size,
                    ascending: false,
                },
                vec![
                    "mid.bin", "new.txt", "old.txt", "adir", "a.txt", "b.txt", "zdir",
                ],
            ),
            (
                FileSort {
                    key: SortKey::Size,
                    ascending: true,
                },
                vec![
                    "adir", "a.txt", "b.txt", "zdir", "old.txt", "new.txt", "mid.bin",
                ],
            ),
        ] {
            sorter.set_sort(pane, sort);
            c.check(
                &format!("{ls}: {}, open directories included", sort.saved()),
                names(&sorter, pane) == expected,
                format!("{:?}", names(&sorter, pane)),
            );
        }
    }
    c.check(
        "GNU's fraction of a second is kept",
        item(&sorter, Pane::Right, "new.txt").and_then(|i| i.modified)
            == Some(utc("2025-06-01T12:00:00Z") + 0.5),
        format!(
            "{:?}",
            item(&sorter, Pane::Right, "new.txt").and_then(|i| i.modified)
        ),
    );
    sorter.set_sort(
        Pane::Left,
        FileSort {
            key: SortKey::Modified,
            ascending: false,
        },
    );
    sorter.reload(Pane::Left);
    pump(2.0);
    c.check(
        "a listing read again keeps the order",
        names(&sorter, Pane::Left)
            == [
                "zdir", "new.txt", "mid.bin", "old.txt", "adir", "b.txt", "a.txt",
            ],
        format!("{:?}", names(&sorter, Pane::Left)),
    );
    let saved = shellder::core::prefs::load().file_sorts;
    c.check(
        "and a new window starts with the order each side had last",
        saved.get("left").map(|s| s.as_str()) == Some("modified descending")
            && saved.get("right").map(|s| s.as_str()) == Some("size ascending"),
        format!("{saved:?}"),
    );

    // This machine: its own files, dated by stat, and its ls standing in for
    // the BSD hosts that only know -T.
    let mine = format!("{here}/sorted");
    std::fs::create_dir_all(format!("{mine}/adir")).unwrap();
    for (name, bytes, date) in [
        ("old.txt", "abc".to_string(), "2024-01-02T03:04:05Z"),
        ("new.txt", "0123456789".to_string(), "2025-06-01T12:00:00Z"),
        ("mid.bin", "0".repeat(1000), "2025-01-01T00:00:00Z"),
    ] {
        std::fs::write(format!("{mine}/{name}"), bytes).unwrap();
        touch(&format!("{mine}/{name}"), utc(date));
    }
    touch(&format!("{mine}/adir"), utc("2023-01-01T00:00:00Z"));
    sorter.set_source(Pane::Right, FileSource::Local);
    pump(1.0);
    sorter.set_root(Pane::Right, &mine, true);
    pump(1.0);
    c.check(
        "this machine dates its files too",
        item(&sorter, Pane::Right, "old.txt").and_then(|i| i.modified)
            == Some(utc("2024-01-02T03:04:05Z")),
        format!(
            "{:?}",
            item(&sorter, Pane::Right, "old.txt").and_then(|i| i.modified)
        ),
    );
    sorter.set_sort(
        Pane::Right,
        FileSort {
            key: SortKey::Modified,
            ascending: false,
        },
    );
    c.check(
        "and sorts them the same way",
        names(&sorter, Pane::Right) == ["new.txt", "mid.bin", "old.txt", "adir"],
        format!("{:?}", names(&sorter, Pane::Right)),
    );
    let bsd = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(remote::list_command(&mine))
        .output()
        .unwrap();
    let bsd_text = String::from_utf8_lossy(&bsd.stdout).into_owned();
    let same = |items: Vec<remotefs::FileItem>| -> Vec<String> {
        let mut v: Vec<String> = items
            .iter()
            .map(|i| {
                format!(
                    "{} {} {} {}",
                    i.name,
                    i.is_dir,
                    if i.is_dir { 0 } else { i.size },
                    i.modified.unwrap_or(0.0).floor()
                )
            })
            .collect();
        v.sort();
        v
    };
    match local::list(&mine) {
        Ok(l) => c.check(
            "the BSD ls (-T) reads the same as stat, to the second",
            same(remotefs::parse(&bsd_text, &mine, 0.0)) == same(l.clone()),
            format!(
                "{:?} vs {:?}",
                same(remotefs::parse(&bsd_text, &mine, 0.0)),
                same(l)
            ),
        ),
        Err(_) => c.check(
            "the BSD ls (-T) reads the same as stat, to the second",
            false,
            "no local listing",
        ),
    }

    let _ = shellder::core::prefs::update(|p| p.file_sorts.clear());
    if c.0.is_empty() {
        println!("PASS: the Files window's model does what the panes ask of it");
    } else {
        panic!("FAIL: {} check(s) failed: {:?}", c.0.len(), c.0);
    }
}

/// Wait for the pane's rows to be what `want` says, up to five seconds.
fn rows_become(m: &FilesModel, pane: Pane, want: &[&str]) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5) {
        if names(m, pane) == want {
            return true;
        }
        pump(0.05);
    }
    false
}

/// Wait for every listing of the pane to have landed, up to ten seconds.
fn listings_done(m: &FilesModel, pane: Pane) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(10) {
        let s = m.snapshot();
        let p = &s.panes[if pane == Pane::Left { "left" } else { "right" }];
        if !p.loading_root && !p.rows.iter().any(|r| r.loading) {
            return true;
        }
        pump(0.05);
    }
    false
}

/// The tests against this machine get a state directory of their own, so
/// the sort orders saved by the app in use do not reorder their rows and
/// theirs do not end up in its prefs. Tests/files-window.sh gives one.
fn isolated() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("SHELLDER_STATE_DIR").is_none() {
            let dir = std::env::temp_dir().join(format!("shjt-files-state-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            std::env::set_var("SHELLDER_STATE_DIR", &dir);
        }
        std::env::set_var("SHELLDER_NO_KEYCHAIN", "1");
    });
}

/// A model with its left pane on this machine, showing `root`.
fn local_model(host: &str, root: &str) -> Arc<FilesModel> {
    isolated();
    let m = files::for_host(host);
    m.set_source(Pane::Left, FileSource::Local);
    // The source lists the home on a thread of its own; the root goes in after.
    let t = Instant::now();
    while m.snapshot().panes["left"].root.is_empty() && t.elapsed() < Duration::from_secs(5) {
        pump(0.05);
    }
    m.set_root(Pane::Left, root, false);
    m
}

/// Against this machine, no host needed: shutting a folder whole shuts
/// everything under it; and going back to a directory shows its folders
/// open as they were, the deeper ones read once their parents are.
#[test]
fn a_folder_shuts_whole_and_a_view_comes_back_as_it_was() {
    let dir = std::env::temp_dir().join(format!("shjt-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a/b/c")).unwrap();
    std::fs::create_dir_all(dir.join("d")).unwrap();
    std::fs::write(dir.join("a/b/f.txt"), "x").unwrap();
    let root = dir.to_str().unwrap().to_string();
    let m = local_model("shjt-tree", &root);
    let left = Pane::Left;
    let a = format!("{root}/a");
    assert!(rows_become(&m, left, &["a", "d"]), "{:?}", names(&m, left));
    m.expand(left, &a);
    assert!(
        rows_become(&m, left, &["a", "b", "d"]),
        "{:?}",
        names(&m, left)
    );
    m.expand(left, &format!("{a}/b"));
    let whole = ["a", "b", "c", "f.txt", "d"];
    assert!(rows_become(&m, left, &whole), "{:?}", names(&m, left));
    m.expand(left, &format!("{a}/b/c"));
    pump(0.2);
    m.collapse_all(left, &a);
    assert!(rows_become(&m, left, &["a", "d"]), "{:?}", names(&m, left));
    // Opened by hand again, b is shut: the whole shutting took it too.
    m.expand(left, &a);
    assert!(
        rows_become(&m, left, &["a", "b", "d"]),
        "{:?}",
        names(&m, left)
    );
    m.expand(left, &format!("{a}/b"));
    assert!(rows_become(&m, left, &whole), "{:?}", names(&m, left));
    // Into d and back, several times: a and b open as they were, c shut.
    for _ in 0..5 {
        m.set_root(left, &format!("{root}/d"), true);
        assert!(rows_become(&m, left, &[]), "{:?}", names(&m, left));
        assert!(m.snapshot().panes["left"].can_go_back);
        m.back(left);
        assert!(rows_become(&m, left, &whole), "{:?}", names(&m, left));
        assert!(!m.snapshot().panes["left"].can_go_back);
        assert_eq!(
            m.selection(left),
            [format!("{root}/d")],
            "the directory left is selected"
        );
    }
    // Into a/b, c opened there; up to a, never shown as a root before:
    // a closed tree with b selected. Into a/b again: c open as it was
    // left. Back out: a's tree as it was left, b selected.
    m.set_root(left, &format!("{a}/b"), true);
    assert!(
        rows_become(&m, left, &["c", "f.txt"]),
        "{:?}",
        names(&m, left)
    );
    m.expand(left, &format!("{a}/b/c"));
    pump(0.3);
    assert!(m.rows(left)[0].expanded);
    m.up(left);
    assert!(rows_become(&m, left, &["b"]), "{:?}", names(&m, left));
    assert_eq!(m.selection(left), [format!("{a}/b")]);
    m.set_root(left, &format!("{a}/b"), true);
    assert!(
        rows_become(&m, left, &["c", "f.txt"]),
        "{:?}",
        names(&m, left)
    );
    assert!(m.rows(left)[0].expanded, "c is open as it was left");
    m.back(left);
    assert!(rows_become(&m, left, &["b"]), "{:?}", names(&m, left));
    assert_eq!(m.selection(left), [format!("{a}/b")]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A folder two levels down stays open, with what is in it, when the
/// root is read again (a new file, a delete, a refresh); what changed
/// deep down shows after a refresh; and the entries whose name starts
/// with a dot come and go without a listing.
#[test]
fn a_refresh_keeps_the_deep_folders_open_and_current() {
    let dir = std::env::temp_dir().join(format!("shjt-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a/b/c")).unwrap();
    std::fs::create_dir_all(dir.join("d")).unwrap();
    std::fs::write(dir.join("a/b/f.txt"), "x").unwrap();
    let root = dir.to_str().unwrap().to_string();
    let a = format!("{root}/a");
    let b = format!("{a}/b");
    let m = local_model("shjt-refresh", &root);
    let left = Pane::Left;
    assert!(rows_become(&m, left, &["a", "d"]), "{:?}", names(&m, left));
    m.expand(left, &a);
    assert!(
        rows_become(&m, left, &["a", "b", "d"]),
        "{:?}",
        names(&m, left)
    );
    m.expand(left, &b);
    assert!(
        rows_become(&m, left, &["a", "b", "c", "f.txt", "d"]),
        "{:?}",
        names(&m, left)
    );

    // A new file in the root reads the root again: b keeps what it had.
    m.set_selected(left, vec![]);
    m.create(left, "made.txt", false);
    assert!(
        rows_become(&m, left, &["a", "b", "c", "f.txt", "d", "made.txt"]),
        "the root read again keeps b open with what is in it: {:?}",
        names(&m, left)
    );
    m.delete(left, vec![format!("{root}/made.txt")]);
    assert!(
        rows_become(&m, left, &["a", "b", "c", "f.txt", "d"]),
        "{:?}",
        names(&m, left)
    );
    // Shut and opened again, b shows what it has without a listing.
    m.collapse(left, &b);
    m.expand(left, &b);
    assert_eq!(names(&m, left), ["a", "b", "c", "f.txt", "d"]);

    // Changed on disk, two levels down and at the top: a refresh shows both.
    std::fs::write(dir.join("a/b/g.txt"), "x").unwrap();
    std::fs::write(dir.join("new.txt"), "x").unwrap();
    m.reload(left);
    assert!(
        rows_become(&m, left, &["a", "b", "c", "f.txt", "g.txt", "d", "new.txt"]),
        "{:?}",
        names(&m, left)
    );
    // A new file in b, selected, reads b again.
    std::fs::write(dir.join("a/b/h.txt"), "x").unwrap();
    m.set_selected(left, vec![b.clone()]);
    m.create(left, "made.txt", false);
    assert!(
        rows_become(
            &m,
            left,
            &["a", "b", "c", "f.txt", "g.txt", "h.txt", "made.txt", "d", "new.txt"]
        ),
        "{:?}",
        names(&m, left)
    );

    // What was read in a shut folder is read afresh when it shows again
    // after a refresh.
    m.collapse(left, &a);
    std::fs::remove_file(dir.join("a/b/g.txt")).unwrap();
    m.reload(left);
    assert!(
        rows_become(&m, left, &["a", "d", "new.txt"]),
        "{:?}",
        names(&m, left)
    );
    assert!(listings_done(&m, left));
    m.expand(left, &a);
    assert!(
        rows_become(
            &m,
            left,
            &["a", "b", "c", "f.txt", "h.txt", "made.txt", "d", "new.txt"]
        ),
        "{:?}",
        names(&m, left)
    );

    // Dot entries were read with the rest: showing them takes no listing.
    std::fs::write(dir.join(".dot"), "x").unwrap();
    m.reload(left);
    assert!(listings_done(&m, left));
    assert!(!names(&m, left).contains(&".dot".to_string()));
    m.toggle_hidden(left);
    assert!(
        names(&m, left).contains(&".dot".to_string()),
        "shown at once: {:?}",
        names(&m, left)
    );
    m.toggle_hidden(left);
    assert!(!names(&m, left).contains(&".dot".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Listings that overlap: one for a root already left is dropped, its
/// failure too; one asked for again while it runs is read once more, so
/// what changed meanwhile shows; a folder that cannot be read says so on
/// its own row and is read again when opened again.
#[test]
fn listings_that_overlap_land_where_they_belong() {
    let dir = std::env::temp_dir().join(format!("shjt-race-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let big = dir.join("big");
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..4000 {
        std::fs::write(big.join(format!("f{i:05}")), "").unwrap();
    }
    std::fs::create_dir_all(dir.join("small/locked")).unwrap();
    std::fs::write(dir.join("small/one.txt"), "x").unwrap();
    let big = big.to_str().unwrap().to_string();
    let small = format!("{}/small", dir.to_str().unwrap());
    let m = local_model("shjt-race", &small);
    let left = Pane::Left;
    assert!(
        rows_become(&m, left, &["locked", "one.txt"]),
        "{:?}",
        names(&m, left)
    );

    // Into the big one and straight out: its listing lands after, for a
    // tree no longer shown.
    for _ in 0..3 {
        m.set_root(left, &big, true);
        m.set_root(left, &small, true);
        assert!(listings_done(&m, left));
        pump(0.3);
        assert_eq!(names(&m, left), ["locked", "one.txt"]);
        assert_eq!(m.root(left), small);
    }
    // A root that is not there, left at once: its failure is not this one's.
    for _ in 0..3 {
        m.set_root(left, &format!("{small}/nope"), true);
        m.set_root(left, &small, true);
        assert!(listings_done(&m, left));
        pump(0.2);
        let s = m.snapshot();
        assert_eq!(s.panes["left"].error, None);
        assert_eq!(names(&m, left), ["locked", "one.txt"]);
    }

    // Asked for again while it is being read: read once more after.
    m.set_root(left, &big, true);
    std::fs::write(format!("{big}/zz-late"), "").unwrap();
    m.reload(left);
    assert!(listings_done(&m, left));
    assert!(
        names(&m, left).contains(&"zz-late".to_string()),
        "a file made while the listing ran shows"
    );
    m.set_root(left, &small, true);
    assert!(rows_become(&m, left, &["locked", "one.txt"]));

    // A folder that cannot be read (as root it can, and this part is
    // skipped).
    let locked = format!("{small}/locked");
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&locked).is_err() {
        m.expand(left, &locked);
        assert!(listings_done(&m, left));
        let s = m.snapshot();
        let row = s.panes["left"].rows[0].clone();
        assert!(row.item.name == "locked" && row.error.is_some(), "{row:?}");
        assert_eq!(s.panes["left"].error, None, "the pane itself is fine");
        std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        std::fs::write(format!("{locked}/inside.txt"), "x").unwrap();
        m.collapse(left, &locked);
        m.expand(left, &locked);
        assert!(
            rows_become(&m, left, &["locked", "inside.txt", "one.txt"]),
            "opened again, it is read again: {:?}",
            names(&m, left)
        );
        assert_eq!(m.snapshot().panes["left"].rows[0].error, None);
    }
    let _ = std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755));
    let _ = std::fs::remove_dir_all(&dir);
}

/// What is dropped or uploaded lands in the folder it is on: the folder
/// shown (the Console's "Upload here…" on the blank went into its parent),
/// a folder of the tree, or the folder around a file.
#[test]
fn a_drop_lands_in_the_folder_it_is_on() {
    let dir = std::env::temp_dir().join(format!("shjt-drop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("one.txt"), "x").unwrap();
    std::fs::write(dir.join("sub/two.txt"), "x").unwrap();
    let root = dir.to_str().unwrap().to_string();
    let m = local_model("shjt-drop", &root);
    let left = Pane::Left;
    let sub = format!("{root}/sub");
    assert!(
        rows_become(&m, left, &["sub", "one.txt"]),
        "{:?}",
        names(&m, left)
    );
    assert_eq!(m.destination(left, &root), root, "the folder shown");
    assert_eq!(m.destination(left, &sub), sub);
    assert_eq!(m.destination(left, &format!("{root}/one.txt")), root);
    m.expand(left, &sub);
    assert!(
        rows_become(&m, left, &["sub", "two.txt", "one.txt"]),
        "{:?}",
        names(&m, left)
    );
    assert_eq!(m.destination(left, &format!("{sub}/two.txt")), sub);
    // A pane with no folder yet takes nothing: "" is no folder to copy into.
    let rows = vec![(
        FileSource::Host("shjt-drop-elsewhere".into()),
        "/x".to_string(),
    )];
    assert!(!m.accept(rows, "", left));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Wait for the copy queue to be asking about `id`, up to five seconds.
fn asks_about(m: &FilesModel, id: u64) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5) {
        if m.snapshot().question.is_some_and(|q| q.id == id) {
            return true;
        }
        pump(0.05);
    }
    false
}

fn state_of(m: &FilesModel, id: u64) -> Option<TransferState> {
    m.snapshot()
        .transfers
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.state.clone())
}

/// Copies still waiting come off the queue, one or all, the running (or
/// asked about) one included only when named. No host needed: a copy
/// whose name is taken waits for its question, which holds the queue,
/// and a copy from this machine onto itself fails at once.
#[test]
fn copies_waiting_can_be_taken_off_the_queue() {
    isolated();
    let dir = std::env::temp_dir().join(format!("shjt-queue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("dst")).unwrap();
    for n in ["a", "b", "c", "d", "e", "f"] {
        std::fs::write(dir.join(format!("src/{n}.txt")), n).unwrap();
    }
    std::fs::write(dir.join("dst/a.txt"), "taken").unwrap();
    std::fs::write(dir.join("dst/d.txt"), "taken").unwrap();
    let base = dir.to_str().unwrap().to_string();
    let job = |n: &str| shellder::core::transfer::TransferJob {
        from: FileSource::Local,
        to: FileSource::Local,
        source: format!("{base}/src/{n}.txt"),
        destination: format!("{base}/dst"),
        resume: false,
    };
    let m = files::for_host("shjt-queue");
    let id_of = |n: &str| {
        m.snapshot()
            .transfers
            .iter()
            .rev()
            .find(|t| t.name == format!("{n}.txt"))
            .map(|t| t.id)
            .unwrap()
    };
    for n in ["a", "b", "c"] {
        m.enqueue(job(n));
    }
    let (a, b, c) = (id_of("a"), id_of("b"), id_of("c"));
    assert!(asks_about(&m, a), "a's name is taken");
    assert_eq!(m.snapshot().queued, 3, "the one asked about still waits");

    m.cancel_waiting(b);
    assert_eq!(state_of(&m, b), Some(TransferState::Cancelled));
    let s = m.snapshot();
    assert_eq!(s.queued, 2);
    assert!(
        s.question.is_some_and(|q| q.id == a),
        "a is still asked about"
    );
    let rec = s.transfers.iter().find(|t| t.id == b).unwrap();
    assert!(
        !rec.can_resume && s.can_retry.contains(&b),
        "taken off before it started, it can be asked for again, afresh"
    );

    // The one asked about, taken off: the question goes, the queue moves on.
    m.cancel_waiting(a);
    assert_eq!(state_of(&m, a), Some(TransferState::Cancelled));
    let t = Instant::now();
    while !matches!(state_of(&m, c), Some(TransferState::Failed { .. }))
        && t.elapsed() < Duration::from_secs(5)
    {
        pump(0.05);
    }
    assert!(
        matches!(state_of(&m, c), Some(TransferState::Failed { .. })),
        "c ran: {:?}",
        state_of(&m, c)
    );
    assert!(m.snapshot().question.is_none());

    // All of them at once.
    for n in ["d", "e", "f"] {
        m.enqueue(job(n));
    }
    let (d, e, f) = (id_of("d"), id_of("e"), id_of("f"));
    assert!(asks_about(&m, d));
    m.cancel_all_waiting();
    pump(0.3);
    let s = m.snapshot();
    assert!(s.question.is_none() && s.queued == 0 && s.active.is_none());
    for id in [d, e, f] {
        assert_eq!(state_of(&m, id), Some(TransferState::Cancelled));
    }
    assert_eq!(
        std::fs::read_to_string(dir.join("dst/d.txt")).unwrap(),
        "taken",
        "nothing was written over"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The question about a taken name, against this machine (a copy from it
/// onto itself fails at once, after the question): "Apply to all" answers
/// for the copies of its own drop, not for one dropped meanwhile; an
/// answer to a question no longer asked changes nothing; a copy that was
/// to write over a file is not one to go on from; and the folder a copy
/// went into is read again, open below the pane's root as it is.
#[test]
fn a_drop_is_asked_about_on_its_own() {
    isolated();
    let dir = std::env::temp_dir().join(format!("shjt-ask-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/site")).unwrap();
    std::fs::create_dir_all(dir.join("dst/site")).unwrap();
    for n in ["a", "b", "c", "e"] {
        std::fs::write(dir.join(format!("src/{n}.txt")), n).unwrap();
    }
    for n in ["a", "b", "c"] {
        std::fs::write(dir.join(format!("dst/{n}.txt")), "taken").unwrap();
    }
    let base = dir.to_str().unwrap().to_string();
    let job = |n: &str| shellder::core::transfer::TransferJob {
        from: FileSource::Local,
        to: FileSource::Local,
        source: format!("{base}/src/{n}"),
        destination: format!("{base}/dst"),
        resume: false,
    };
    let m = local_model("shjt-ask", &format!("{base}/dst"));
    let left = Pane::Left;
    let id_of = |n: &str| {
        m.snapshot()
            .transfers
            .iter()
            .rev()
            .find(|t| t.name == n)
            .map(|t| t.id)
            .unwrap()
    };
    let ended = |id: u64| {
        let t = Instant::now();
        while !matches!(
            state_of(&m, id),
            Some(TransferState::Failed { .. } | TransferState::Skipped)
        ) && t.elapsed() < Duration::from_secs(5)
        {
            pump(0.05);
        }
        state_of(&m, id)
    };

    m.enqueue_all(vec![job("a.txt"), job("b.txt")]);
    let (a, b) = (id_of("a.txt"), id_of("b.txt"));
    assert!(asks_about(&m, a));
    assert!(m.snapshot().question.unwrap().offer_all, "b came with a");
    m.enqueue(job("c.txt"));
    let c = id_of("c.txt");
    m.answer_replace(c, ReplaceAnswer::ReplaceAll);
    assert!(
        m.snapshot().question.is_some_and(|q| q.id == a),
        "an answer for another question is not a's"
    );
    m.answer_replace(a, ReplaceAnswer::ReplaceAll);
    assert!(matches!(ended(b), Some(TransferState::Failed { .. })));
    assert!(
        asks_about(&m, c),
        "c, dropped on its own, is asked about: {:?}",
        state_of(&m, c)
    );
    let q = m.snapshot().question.unwrap();
    assert!(!q.offer_all, "nothing else came with c");
    m.answer_replace(c, ReplaceAnswer::Skip);
    assert_eq!(ended(c), Some(TransferState::Skipped));
    let s = m.snapshot();
    let rec = |id: u64| s.transfers.iter().find(|t| t.id == id).unwrap().clone();
    assert!(
        !rec(a).can_resume && !rec(b).can_resume,
        "a copy that was to write over a file starts over when asked again"
    );
    m.enqueue(job("e.txt"));
    let e = id_of("e.txt");
    assert!(matches!(ended(e), Some(TransferState::Failed { .. })));
    assert!(
        m.snapshot()
            .transfers
            .iter()
            .any(|t| t.id == e && t.can_resume),
        "one that had the name to itself goes on from what it wrote"
    );

    // dst/site open in the tree; a file put there meanwhile shows once a
    // copy into it has ended (here: failed after Replace).
    let site = format!("{base}/dst/site");
    m.expand(left, &site);
    assert!(listings_done(&m, left));
    pump(0.3);
    assert_eq!(names(&m, left), ["site", "a.txt", "b.txt", "c.txt"]);
    std::fs::write(format!("{site}/new.txt"), "x").unwrap();
    m.enqueue(job("site"));
    let s1 = id_of("site");
    assert!(asks_about(&m, s1));
    m.answer_replace(s1, ReplaceAnswer::Replace);
    assert!(matches!(ended(s1), Some(TransferState::Failed { .. })));
    assert!(
        rows_become(&m, left, &["site", "new.txt", "a.txt", "b.txt", "c.txt"]),
        "the folder copied into is read again: {:?}",
        names(&m, left)
    );
    // The folder shown, shown again: re-read, site still open.
    m.set_root(left, &format!("{base}/dst"), true);
    pump(0.3);
    assert!(
        rows_become(&m, left, &["site", "new.txt", "a.txt", "b.txt", "c.txt"]),
        "{:?}",
        names(&m, left)
    );
    assert!(
        !m.snapshot().panes["left"].can_go_back,
        "and it is not a step to go back to"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn touch(path: &str, unix: f64) {
    let t = std::fs::FileTimes::new()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs_f64(unix));
    std::fs::File::options()
        .write(true)
        .open(path)
        .or_else(|_| std::fs::File::open(path))
        .unwrap()
        .set_times(t)
        .unwrap();
}
