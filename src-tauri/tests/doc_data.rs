//! The data the documentation's pictures are drawn from: `docs/shots`
//! draws the app's pages in a web view of its own, over a stand-in for
//! the app that answers their calls with what this test writes, so that
//! a picture shows what the real model, the real parser of a host's
//! readings and the real Files model say, of hosts that do not exist.
//! Ignored unless `docs/shots/shoot.sh` runs it, naming the directory to
//! write in (`DOC_DATA_OUT`): the state every window draws from, of a
//! made-up ssh config (`state.json`); a host's readings, from the
//! fixture the parser is tested with (`reading.json`); and a Files view
//! of a small tree on this machine (`files.json`, its root `ROOT`).

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use shellder::app::files::{self, Pane};
use shellder::app::model::Model;
use shellder::core::monitor;
use shellder::core::ssh::FileSource;

const CONFIG: &str = "\
Host bastion
    HostName bastion.example.org
    User alice

Host web-1
    HostName 10.0.3.11
    User deploy
    ProxyJump bastion

Host gpu-box
    HostName gpu-box.example.org
    User alice
    Port 2222

Host db-staging
    HostName 10.0.3.40
    User postgres
    ProxyJump bastion

Host backup
    HostName backup.example.org
    User alice

Host *
    ControlMaster auto
    ControlPath ~/.ssh/cm-%C
    ServerAliveInterval 30
";

fn write(dir: &Path, name: &str, value: &impl serde::Serialize) {
    fs::write(dir.join(name), serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn tree(root: &Path) {
    for d in [
        "project/src",
        "project/docs",
        "project/data",
        "notes",
        ".config",
    ] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    for (f, size) in [
        ("project/README.md", 2_400),
        ("project/Makefile", 900),
        ("project/train.py", 14_200),
        ("project/src/model.py", 31_000),
        ("project/src/data.py", 8_700),
        ("project/docs/report.pdf", 1_840_000),
        ("project/data/results.csv", 652_000),
        ("project/data/run-42.log", 96_000),
        ("notes/todo.md", 1_100),
        ("figure.png", 412_000),
        (".bashrc", 3_800),
    ] {
        fs::write(root.join(f), vec![b'x'; size]).unwrap();
    }
}

#[test]
#[ignore]
fn doc_data() {
    let out = std::env::var("DOC_DATA_OUT").expect("DOC_DATA_OUT names the directory to write in");
    let out = Path::new(&out);
    let work = out.join("work");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(work.join("state")).unwrap();
    fs::write(work.join("config"), CONFIG).unwrap();
    std::env::set_var("SHELLDER_STATE_DIR", work.join("state"));
    std::env::set_var("SHELLDER_SSH_CONFIG", work.join("config"));
    std::env::set_var("SHELLDER_NO_KEYCHAIN", "1");

    // The state: the hosts of the config, each resolved by ssh -G.
    let model = Model::new();
    model.reload_catalog(true);
    let t = Instant::now();
    loop {
        let s = model.snapshot();
        if !s.reloading && s.resolved.len() == s.hosts.len() && !s.hosts.is_empty() {
            break;
        }
        assert!(
            t.elapsed() < Duration::from_secs(30),
            "the hosts were never resolved"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    write(out, "state.json", &model.snapshot());

    // A host's readings.
    write(
        out,
        "reading.json",
        &monitor::parse(include_str!("../src/core/monitor/fixtures/linux.txt")),
    );

    // A Files view: the three panes on a tree of this machine.
    let root = work.join("home");
    tree(&root);
    let root = fs::canonicalize(&root).unwrap();
    let m = files::for_host("gpu-box");
    for pane in Pane::ALL {
        m.set_source(pane, FileSource::Local);
        m.set_root(pane, root.to_str().unwrap(), false);
    }
    std::thread::sleep(Duration::from_millis(500));
    for pane in Pane::ALL {
        m.toggle(pane, root.join("project").to_str().unwrap());
    }
    std::thread::sleep(Duration::from_millis(500));
    m.toggle(Pane::Tab, root.join("project/src").to_str().unwrap());
    let t = Instant::now();
    while m.rows(Pane::Tab).iter().any(|r| r.loading) && t.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
    write(out, "files.json", &m.snapshot());
    fs::write(out.join("ROOT"), root.to_str().unwrap()).unwrap();
}
