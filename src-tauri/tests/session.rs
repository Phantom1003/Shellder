//! A host without a master: the session logs in once (host key and
//! passphrase answered on the controlling pty) and runs commands; a copy
//! is an sftp that logs in on its own the same way, both ways, going on
//! from where a copy stopped; and a shell window's terminal gets a shell
//! whose output comes back. Against a throwaway sshd started as this user.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shellder::core::prompt::{Kind, Origin, PromptHandler, PromptRequest};
use shellder::core::remotefs::{fs, remote};
use shellder::core::session;
use shellder::core::ssh::FileSource;
use shellder::core::term::{Launch, Terminal};
use shellder::core::transfer::{TransferJob, TransferRun};

const HOST: &str = "shjt-session";

struct Answers {
    seen: Mutex<Vec<(Origin, Kind)>>,
}

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        self.seen.lock().unwrap().push((req.origin, req.kind));
        match req.kind {
            Kind::Confirm => Some("yes".into()),
            Kind::Passphrase => Some(common::PASSPHRASE.into()),
            _ => None,
        }
    }
}

fn copy(
    from: FileSource,
    source: &str,
    to: FileSource,
    destination: &str,
    resume: bool,
) -> (i32, String) {
    let job = TransferJob {
        from,
        to,
        source: source.into(),
        destination: destination.into(),
        resume,
    };
    assert!(
        job.logs_in(),
        "a host without a master makes the copy log in"
    );
    let run = TransferRun::new();
    let total = TransferRun::total(&job);
    let out = run.run(&job, total, |_| {});
    (out.status, out.text())
}

fn truncate(path: &str, to: usize) {
    let mut data = std::fs::read(path).unwrap();
    data.truncate(to);
    std::fs::write(path, data).unwrap();
}

#[test]
fn a_session_logs_in_once_and_copies_log_in_on_their_own() {
    let Some(sshd) = common::Sshd::start("session") else {
        return;
    };
    // No ControlPath: nothing could go through a master even if one were up.
    sshd.configure(HOST, "");
    let d = sshd.dir.clone();
    let handler = Arc::new(Answers {
        seen: Mutex::new(vec![]),
    });
    let h: Arc<dyn PromptHandler> = handler.clone();
    session::set_prompt_handler(h);
    assert!(!remote::through_master(HOST));
    let asked = || handler.seen.lock().unwrap().clone();

    // Several callers at once (the Console's readings, its Files tab and
    // its sessions list, say) share one login: the later ones wait for the
    // first and take its session.
    let callers: Vec<_> = (0..4)
        .map(|_| std::thread::spawn(|| session::for_host(HOST).expect("session")))
        .collect();
    let all: Vec<Arc<session::Session>> = callers.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(
        all.iter().all(|x| Arc::ptr_eq(x, &all[0])),
        "one session for them all"
    );
    drop(all);

    // Commands: stdout and stderr in order, the exit status, and a second
    // command on the same login (no new questions).
    let s = session::for_host(HOST).expect("session");
    let out = s.run("echo hello; echo oops >&2; printf no-newline; exit 3");
    assert_eq!(out.status, 3, "{}", out.text());
    assert_eq!(out.stdout, "hello\noops\nno-newline");
    let out = s.run("printf 'x\\n'");
    assert_eq!((out.status, out.stdout.as_str()), (0, "x\n"));
    let again = session::for_host(HOST).expect("session");
    assert!(Arc::ptr_eq(&s, &again), "the same session is reused");
    assert_eq!(
        asked(),
        vec![
            (Origin::Session, Kind::Confirm),
            (Origin::Session, Kind::Passphrase)
        ],
        "one login, two questions"
    );
    // The listings go through it too.
    let home = fs::home(&FileSource::Host(HOST.into())).unwrap();
    assert!(home.starts_with('/'));
    let items = fs::list(&FileSource::Host(HOST.into()), &d).unwrap();
    assert!(items.iter().any(|i| i.name == "config"), "{items:?}");
    assert_eq!(asked().len(), 2, "no login for a listing");
    // A file's bytes come through it as they are (base64 on the host),
    // only as many as asked for, and a path that is not a readable file
    // is refused in words.
    let noise = common::noise(10_000, 3);
    let quoted = format!("{d}/it's.bin");
    std::fs::write(&quoted, &noise).unwrap();
    let there = FileSource::Host(HOST.into());
    assert_eq!(
        fs::read(&there, &quoted, 20_000).unwrap(),
        (noise.clone(), false)
    );
    assert_eq!(
        fs::read(&there, &quoted, 4_000).unwrap(),
        (noise[..4_000].to_vec(), true)
    );
    assert!(fs::read(&there, &d, 10)
        .unwrap_err()
        .0
        .ends_with("is a directory"));
    assert!(fs::read(&there, &format!("{d}/nothing"), 10)
        .unwrap_err()
        .0
        .contains("no such file"));
    assert_eq!(asked().len(), 2, "no login for a read");

    // Up: a file of 3 MB lands whole in a directory on the host. The copy
    // logged in on its own: one more question (the host key is known by
    // now), answered under the session's origin.
    let here = format!("{d}/here");
    let there = format!("{d}/there");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(&there).unwrap();
    let data = common::noise(3 * 1024 * 1024 + 123, 7);
    std::fs::write(format!("{here}/big.bin"), &data).unwrap();
    let host = FileSource::Host(HOST.into());
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/big.bin"),
        host.clone(),
        &there,
        false,
    );
    assert_eq!(
        rc,
        0,
        "{said}\n{}",
        std::fs::read_to_string(format!("{d}/shellder.log")).unwrap_or_default()
    );
    assert_eq!(std::fs::read(format!("{there}/big.bin")).unwrap(), data);
    assert_eq!(asked().len(), 3);
    assert_eq!(asked()[2], (Origin::Session, Kind::Passphrase));

    // Down: the same file back, into another directory.
    let back = format!("{d}/back");
    std::fs::create_dir_all(&back).unwrap();
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/big.bin"),
        FileSource::Local,
        &back,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(std::fs::read(format!("{back}/big.bin")).unwrap(), data);

    // Going on: a download cut to a third is completed, and so is an
    // upload the host has a piece of (sftp refuses to resume over a whole one).
    truncate(&format!("{back}/big.bin"), data.len() / 3);
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/big.bin"),
        FileSource::Local,
        &back,
        true,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(std::fs::read(format!("{back}/big.bin")).unwrap(), data);
    truncate(&format!("{there}/big.bin"), 1000);
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/big.bin"),
        host.clone(),
        &there,
        true,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(std::fs::read(format!("{there}/big.bin")).unwrap(), data);

    // A directory, with a space in a name and an empty file.
    std::fs::create_dir_all(format!("{here}/a dir/sub")).unwrap();
    std::fs::write(format!("{here}/a dir/one.txt"), b"one\n").unwrap();
    std::fs::write(format!("{here}/a dir/sub/empty"), b"").unwrap();
    std::fs::write(format!("{here}/a dir/sub/two's.txt"), b"two\n").unwrap();
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/a dir"),
        host.clone(),
        &there,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(
        std::fs::read(format!("{there}/a dir/one.txt")).unwrap(),
        b"one\n"
    );
    assert_eq!(
        std::fs::read(format!("{there}/a dir/sub/empty")).unwrap(),
        b""
    );
    assert_eq!(
        std::fs::read(format!("{there}/a dir/sub/two's.txt")).unwrap(),
        b"two\n"
    );
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/a dir"),
        FileSource::Local,
        &back,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(
        std::fs::read(format!("{back}/a dir/sub/two's.txt")).unwrap(),
        b"two\n"
    );

    // Failures are reported, and the session is not concerned.
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/nope"),
        FileSource::Local,
        &back,
        false,
    );
    assert_ne!(rc, 0);
    assert!(said.contains("nope"), "{said}");
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/big.bin"),
        host.clone(),
        "/nonexistent-dir",
        false,
    );
    assert_ne!(rc, 0);
    assert!(!said.is_empty());
    assert!(s.is_alive());
    assert_eq!(s.run("echo still").stdout, "still\n");

    // A cancelled copy stops; the session, a process of its own, goes on.
    let run = TransferRun::new();
    let job = TransferJob {
        from: host.clone(),
        to: FileSource::Local,
        source: format!("{there}/big.bin"),
        destination: format!("{d}/cancelled"),
        resume: false,
    };
    std::fs::create_dir_all(format!("{d}/cancelled")).unwrap();
    let r2 = run.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        r2.cancel();
    });
    let out = run.run(&job, Some(data.len() as i64), |_| {});
    assert!(run.is_cancelled());
    assert!(out.status != 0 || std::fs::read(format!("{d}/cancelled/big.bin")).unwrap() == data);
    assert!(s.is_alive());
    assert_eq!(s.run("echo back").stdout, "back\n");

    // The Console's readings come through the session too, and this
    // machine answers the script's questions one way or another.
    let before = asked().len();
    let parts: Vec<String> = shellder::core::monitor::PARTS
        .iter()
        .map(|p| p.to_string())
        .collect();
    let reading = shellder::core::monitor::fetch(HOST, &parts).expect("console readings");
    assert!(!reading.hostname.is_empty());
    assert!(reading.load.is_some(), "{reading:?}");
    assert!(reading.memory.is_some(), "{reading:?}");
    assert!(!reading.disks.is_empty(), "{reading:?}");
    assert_eq!(asked().len(), before, "no new login for the readings");

    // A shell window's terminal: what is typed runs, what it says comes back.
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let exited = Arc::new(Mutex::new(None));
    let o = output.clone();
    let x = exited.clone();
    let h: Arc<dyn PromptHandler> = handler.clone();
    let term = Terminal::open(
        HOST,
        Launch::Shell {
            dir: None,
            integration: false,
        },
        80,
        24,
        h,
        move |bytes| o.lock().unwrap().extend_from_slice(bytes),
        move |code| {
            *x.lock().unwrap() = Some(code);
        },
    )
    .expect("terminal");
    let wait_for = |needle: &str| {
        let t = Instant::now();
        loop {
            if String::from_utf8_lossy(&output.lock().unwrap()).contains(needle) {
                return;
            }
            assert!(
                t.elapsed() < Duration::from_secs(20),
                "no {needle:?} in {:?}",
                String::from_utf8_lossy(&output.lock().unwrap())
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    term.write(b"echo ma''rco; stty size\n").unwrap();
    wait_for("marco");
    wait_for("24 80");
    term.resize(100, 30);
    term.write(b"stty size\n").unwrap();
    wait_for("30 100");
    term.write(b"exit\n").unwrap();
    let t = Instant::now();
    while exited.lock().unwrap().is_none() && t.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(*exited.lock().unwrap(), Some(0));
    assert!(!term.is_running());
    assert_eq!(
        asked().len(),
        before + 1,
        "the terminal logged in on its own"
    );
    assert_eq!(asked()[before], (Origin::Terminal, Kind::Passphrase));

    // A command that does not answer in time (a `df` on a dead mount) is
    // given up, and the session with it: its marker would come after the
    // next command's output. The next command logs in again.
    let t = Instant::now();
    let out = s.run_within("sleep 10", Duration::from_millis(500));
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    assert_eq!(out.status, -1);
    assert!(out.stderr.contains("timed out"), "{}", out.stderr);
    assert!(!s.is_alive());
    let out = remote::shell_within(HOST, "echo fresh", Duration::from_secs(20));
    assert_eq!(
        (out.status, out.stdout.as_str()),
        (0, "fresh\n"),
        "{}",
        out.text()
    );
    assert_eq!(asked().len(), before + 2, "one login more");
    let fresh = session::existing(HOST).expect("the new session");
    assert!(!Arc::ptr_eq(&s, &fresh));

    // A session whose ssh went away while it sat idle (the link dropped)
    // is found out, and the command goes to a new one: it had not run.
    unsafe { libc::kill(fresh.pid as i32, libc::SIGKILL) };
    std::thread::sleep(Duration::from_millis(300));
    let out = remote::shell(HOST, "echo again");
    assert_eq!(
        (out.status, out.stdout.as_str()),
        (0, "again\n"),
        "{}",
        out.text()
    );
    assert!(!fresh.is_alive());
    assert_eq!(asked().len(), before + 3, "one login more");
    session::close_all();
}

/// The same listings and copies against a BusyBox host (Alpine in Docker)
/// without a ControlPath, the way Tests/files-window.sh sets it up: its
/// `ash` reads its input in chunks, which the session's protocol has to be
/// safe against. Ignored unless the script runs it.
#[test]
#[ignore]
fn a_busybox_host_through_its_session() {
    let Ok(dir) = std::env::var("SHELLDER_FILES_TEST_DIR") else {
        eprintln!("SHELLDER_FILES_TEST_DIR is not set, skipping");
        return;
    };
    const PLAIN: &str = "shjt-files-plain";
    assert!(!remote::through_master(PLAIN));
    let here = format!("{dir}/plain");
    let _ = std::fs::remove_dir_all(&here);
    std::fs::create_dir_all(format!("{here}/a dir/sub")).unwrap();
    let data = common::noise(3 * 1024 * 1024 + 77, 11);
    std::fs::write(format!("{here}/big.bin"), &data).unwrap();
    std::fs::write(format!("{here}/a dir/one.txt"), b"one\n").unwrap();
    std::fs::write(format!("{here}/a dir/sub/empty"), b"").unwrap();
    let host = FileSource::Host(PLAIN.into());
    let home = fs::home(&host).unwrap();
    assert_eq!(home, "/home/t");
    let there = format!("{home}/plain");
    let _ = fs::remove(&host, &there);
    fs::make_directory(&host, &there).unwrap();
    let names = |dir: &str| -> Vec<String> {
        fs::list(&host, dir)
            .unwrap()
            .into_iter()
            .map(|i| i.name)
            .collect()
    };
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/big.bin"),
        host.clone(),
        &there,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(
        fs::bytes(&host, &format!("{there}/big.bin")),
        Some(data.len() as i64)
    );
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/a dir"),
        host.clone(),
        &there,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    let mut listed = names(&there);
    listed.sort();
    assert_eq!(listed, vec!["a dir", "big.bin"]);
    assert_eq!(names(&format!("{there}/a dir/sub")), vec!["empty"]);
    // Back down, whole and then the rest of a cut copy, byte for byte.
    let back = format!("{here}/back");
    std::fs::create_dir_all(&back).unwrap();
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/big.bin"),
        FileSource::Local,
        &back,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(std::fs::read(format!("{back}/big.bin")).unwrap(), data);
    truncate(&format!("{back}/big.bin"), 1_000_000);
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/big.bin"),
        FileSource::Local,
        &back,
        true,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(std::fs::read(format!("{back}/big.bin")).unwrap(), data);
    // And up again from a third, onto the end of what is there.
    let out = session::for_host(PLAIN).unwrap().run(&format!(
        "head -c 1000000 {0} > {0}.part && mv {0}.part {0}",
        shellder::core::remotefs::quote(&format!("{there}/big.bin"))
    ));
    assert_eq!(out.status, 0, "{}", out.text());
    let (rc, said) = copy(
        FileSource::Local,
        &format!("{here}/big.bin"),
        host.clone(),
        &there,
        true,
    );
    assert_eq!(rc, 0, "{said}");
    let (rc, said) = copy(
        host.clone(),
        &format!("{there}/big.bin"),
        FileSource::Local,
        &back,
        false,
    );
    assert_eq!(rc, 0, "{said}");
    assert_eq!(std::fs::read(format!("{back}/big.bin")).unwrap(), data);
    fs::make_file(&host, &format!("{there}/new.txt")).unwrap();
    assert!(fs::exists(&host, &format!("{there}/new.txt")).unwrap());
    fs::remove(&host, &there).unwrap();
    assert!(!fs::exists(&host, &there).unwrap());
    session::close_all();
}
