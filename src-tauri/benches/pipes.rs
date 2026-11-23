//! How fast things move through the processes the app starts: the lines
//! a pty reader takes before a question, a terminal's output, a listing
//! and a reading of this machine; and, against a throwaway sshd
//! (`tests/common`, skipped where there is none), a session's login and
//! its commands, the same through a master, a file read, and copies.
//! The lines that begin "plain" are the same work without the app, for
//! the line above each to be read against: the bytes on a pipe instead
//! of a pty, ssh and scp run as a script would, their questions answered
//! by an askpass program instead of on the asking pty.
//!
//! `cargo bench --bench pipes`, or `-- session` for the measurements
//! with that word in their name. A terminal on this machine runs the
//! account's login shell, profile and all, as the app does.

#[path = "../tests/common/mod.rs"]
mod common;
mod harness;

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use shellder::core::monitor;
use shellder::core::prompt::{Kind, Origin, PromptHandler, PromptRequest};
use shellder::core::pty::{self, Pty};
use shellder::core::remotefs::{fs, remote};
use shellder::core::session;
use shellder::core::ssh::{self, FileSource};
use shellder::core::term::{Launch, Terminal};
use shellder::core::transfer::{TransferJob, TransferRun};

const HOST: &str = "shjt-bench";

const fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

struct Answers;

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        match req.kind {
            Kind::Confirm => Some("yes".into()),
            Kind::Passphrase => Some(common::PASSPHRASE.into()),
            Kind::Password => Some("pw".into()),
            _ => None,
        }
    }
}

fn handler() -> Arc<dyn PromptHandler> {
    Arc::new(Answers)
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|p| p.to_string()).collect()
}

/// A line a program might print, 64 bytes with its newline.
const LINE: &str = "the quick brown fox jumps over the lazy dog 0123456789 abcdefgh";

/// `lines` lines written to the controlling terminal and then a
/// question: how long until the script has its answer and is gone. The
/// reader logs every line and waits a fifth of a second of silence
/// before it takes the last text for a question, which is in the time.
fn flood_then_ask(lines: usize) -> Duration {
    ask(lines, true)
}

/// The same, the question asked as a secret is with `echo` false: the
/// terminal's echo off before it is written, which the reader takes for
/// a question sooner.
fn ask(lines: usize, echo: bool) -> Duration {
    let pty = Pty::open().unwrap();
    if !echo {
        // As `readpassphrase` does, without a `stty` to start.
        let fd = std::os::fd::AsRawFd::as_raw_fd(&pty.slave);
        let mut settings = std::mem::MaybeUninit::<libc::termios>::uninit();
        unsafe {
            assert_eq!(libc::tcgetattr(fd, settings.as_mut_ptr()), 0);
            let mut settings = settings.assume_init();
            settings.c_lflag &= !libc::ECHO;
            assert_eq!(libc::tcsetattr(fd, libc::TCSANOW, &settings), 0);
        }
    }
    let script = format!(
        "{}printf 'Password: ' > /dev/tty\nread p < /dev/tty\n[ \"$p\" = pw ]",
        if lines > 0 {
            format!("yes '{LINE}' | head -n {lines} > /dev/tty\n")
        } else {
            String::new()
        }
    );
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    pty.attach(&mut cmd);
    let t = Instant::now();
    let mut child = cmd.spawn().unwrap();
    let pid = child.id();
    let master = pty.master;
    let reader = std::thread::spawn(move || {
        pty::serve("flood".into(), pid, Origin::Master, master, handler(), None)
    });
    let status = child.wait().unwrap();
    let took = t.elapsed();
    drop(pty.slave);
    reader.join().unwrap();
    assert!(
        status.success(),
        "the question was not answered: {:?} {:?}",
        status.code(),
        status.signal()
    );
    took
}

/// A terminal that prints `bytes` and ends: how many came out of it, and
/// how long from the first of them to its end (the login before it, a
/// shell's profile or ssh's, is not the terminal's speed).
fn terminal_output(host: &str, bytes: usize) -> (usize, Duration) {
    let command = format!("yes '{LINE}' | head -c {bytes}");
    let count = Arc::new(AtomicUsize::new(0));
    let first = Arc::new(Mutex::new(None::<Instant>));
    let (done, ended) = mpsc::channel();
    let (c, f) = (count.clone(), first.clone());
    let term = Terminal::open(
        host,
        Launch::Command(&command),
        120,
        40,
        handler(),
        move |chunk| {
            f.lock().unwrap().get_or_insert_with(Instant::now);
            c.fetch_add(chunk.len(), Ordering::Relaxed);
        },
        move |code| {
            let _ = done.send((code, Instant::now()));
        },
    )
    .expect("a terminal");
    let (code, end) = ended
        .recv_timeout(Duration::from_secs(120))
        .expect("the terminal ended");
    assert_eq!(code, 0, "the terminal's command failed");
    drop(term);
    let start = first.lock().unwrap().expect("some output");
    let got = count.load(Ordering::Relaxed);
    // The pty writes a line's end as two bytes.
    assert!(got >= bytes, "{got} of {bytes} bytes came out");
    (got, end.duration_since(start))
}

/// The same bytes from a process on a pipe: how many, and how long from
/// the first of them to the end, as `terminal_output` counts.
fn pipe_output(mut cmd: Command) -> (usize, Duration) {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut buf = [0u8; 16384];
    let mut got = 0;
    let mut first = None;
    loop {
        match out.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                first.get_or_insert_with(Instant::now);
                got += n;
            }
        }
    }
    let took = first.expect("some output").elapsed();
    assert!(child.wait().unwrap().success());
    (got, took)
}

/// ssh or scp as a script would run it, with the bench's config and
/// nothing of the app's: no asking pty, the passphrase and the host key's
/// question answered by the askpass program in `dir`.
fn plain(program: &str, dir: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(["-F", &format!("{dir}/config"), "-o", "ControlPersist=no"])
        .env("SSH_ASKPASS", format!("{dir}/askpass"))
        .env("SSH_ASKPASS_REQUIRE", "force")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    cmd
}

fn run_timed(mut cmd: Command) -> Duration {
    let t = Instant::now();
    let status = cmd.status().unwrap();
    let took = t.elapsed();
    assert!(status.success(), "{cmd:?}");
    took
}

fn copy(from: FileSource, source: &str, to: FileSource, destination: &str) -> Duration {
    let job = TransferJob {
        from,
        to,
        source: source.into(),
        destination: destination.into(),
        resume: false,
    };
    let total = TransferRun::total(&job);
    let t = Instant::now();
    let out = TransferRun::new().run(&job, total, |_| {});
    let took = t.elapsed();
    assert_eq!(out.status, 0, "{}", out.text());
    took
}

/// What goes to a host the same way whichever of the two carries it, a
/// session or a master: `through` says which, in the names.
fn host_work(b: &mut harness::Bench, through: &str, tree: &str, budgets: [Duration; 3]) {
    let host = FileSource::Host(HOST.into());
    let [command, listing, reading] = budgets;
    b.each(&format!("a command through the {through}"), command, || {
        let out = remote::shell(HOST, "true");
        assert_eq!(out.status, 0, "{}", out.text());
    });
    b.each(
        &format!("a listing of 2,000 files through the {through}"),
        listing,
        || {
            let items = fs::list(&host, tree).expect("a listing");
            assert_eq!(items.len(), 2_000);
        },
    );
    b.timed(
        &format!("a host's readings through the {through}"),
        5,
        reading,
        || {
            let t = Instant::now();
            monitor::fetch(HOST, &strings(monitor::PARTS)).expect("a reading");
            t.elapsed()
        },
    );
    let file = format!("{tree}/../blob");
    b.rate(
        &format!("4 MB of a file read through the {through}"),
        3,
        15.0,
        || {
            let t = Instant::now();
            let (bytes, whole) = fs::read(&host, &file, 4 * 1_048_576).expect("the file");
            assert!(!whole || bytes.len() == 4 * 1_048_576);
            (bytes.len(), t.elapsed())
        },
    );
}

fn main() {
    // Before the sshd's own directory is made the state directory.
    let dir = harness::scratch("pipes");
    let mut b = harness::Bench::from_args("pipes (processes, this machine)");
    let here = FileSource::Local;

    b.timed(
        "20,000 lines on the pty, then a question answered",
        5,
        ms(1_500),
        || flood_then_ask(20_000),
    );
    b.timed("a question answered on a quiet pty", 5, ms(1_000), || {
        flood_then_ask(0)
    });
    b.timed(
        "a secret's question answered (the echo off)",
        5,
        ms(200),
        || ask(0, false),
    );

    b.rate("a terminal's output, this machine (32 MB)", 3, 10.0, || {
        terminal_output(ssh::LOCAL, 32 * 1_048_576)
    });
    b.compare_rate("plain: the same output on a pipe, no pty", 3, || {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg(format!("yes '{LINE}' | head -c {}", 32 * 1_048_576));
        pipe_output(cmd)
    });

    let tree = format!("{dir}/tree");
    std::fs::create_dir_all(&tree).unwrap();
    for i in 0..2_000 {
        std::fs::write(format!("{tree}/file {i:04}.txt"), b"x").unwrap();
    }
    b.each("a listing of 2,000 files, this machine", ms(30), || {
        let items = fs::list(&here, &tree).expect("a listing");
        assert_eq!(items.len(), 2_000);
    });
    b.timed(
        "this machine's readings (monitor::fetch)",
        5,
        ms(5_000),
        || {
            let t = Instant::now();
            monitor::fetch(ssh::LOCAL, &strings(monitor::PARTS)).expect("a reading");
            t.elapsed()
        },
    );
    let blob = format!("{dir}/blob");
    std::fs::write(&blob, common::noise(64 * 1_048_576, 7)).unwrap();
    b.rate("16 MB of a file read, this machine", 5, 500.0, || {
        let t = Instant::now();
        let (bytes, _) = fs::read(&here, &blob, 16 * 1_048_576).expect("the file");
        (bytes.len(), t.elapsed())
    });

    // The rest needs a host.
    let wants_host = [
        "session",
        "master",
        "over ssh",
        "upload",
        "download",
        "plain ssh",
        "plain scp",
    ]
    .iter()
    .any(|w| b.wants(w));
    let sshd = if wants_host {
        common::Sshd::start("bench")
    } else {
        None
    };
    let Some(sshd) = sshd else {
        if wants_host {
            b.skipped("the session, the master and the copies", "no sshd here");
        }
        let _ = std::fs::remove_dir_all(&dir);
        b.finish()
    };
    println!("\npipes (a throwaway sshd on this machine)");
    // Unix socket paths are short: the ControlPath stays under /tmp.
    let sockets = format!("/tmp/shjt-bench-cm-{}", std::process::id());
    // `configure` would point the app at its own directory; the config
    // goes where `scratch` said it is instead.
    sshd.configure(HOST, &format!("    ControlPath {sockets}/cm-%C\n"));
    std::fs::copy(format!("{}/config", sshd.dir), format!("{dir}/config")).unwrap();
    std::env::set_var("SHELLDER_STATE_DIR", &dir);
    std::env::set_var("SHELLDER_SSH_CONFIG", format!("{dir}/config"));
    session::set_prompt_handler(handler());
    let host = FileSource::Host(HOST.into());
    let landing = format!("{dir}/landing");

    let askpass = format!("{dir}/askpass");
    std::fs::write(
        &askpass,
        format!(
            "#!/bin/sh\ncase \"$1\" in *yes/no*) echo yes;; *) echo '{}';; esac\n",
            common::PASSPHRASE
        ),
    )
    .unwrap();
    std::fs::set_permissions(&askpass, std::fs::Permissions::from_mode(0o700)).unwrap();

    // No master is up: everything goes through the session.
    assert!(!remote::through_master(HOST));
    b.timed(
        "a session's login (two questions answered)",
        3,
        ms(3_000),
        || {
            session::close_all();
            let _ = std::fs::remove_file(format!("{}/known_hosts", sshd.dir));
            let t = Instant::now();
            session::for_host(HOST).expect("a session");
            t.elapsed()
        },
    );
    b.compare_timed("plain ssh: a login (askpass answering twice)", 3, || {
        let _ = std::fs::remove_file(format!("{}/known_hosts", sshd.dir));
        let mut cmd = plain("ssh", &dir);
        cmd.args([HOST, "true"]);
        run_timed(cmd)
    });
    host_work(&mut b, "session", &tree, [ms(10), ms(150), ms(6_000)]);
    b.rate("a terminal's output over ssh (16 MB)", 3, 8.0, || {
        terminal_output(HOST, 16 * 1_048_576)
    });
    b.compare_rate("plain ssh: the same output on a pipe", 3, || {
        let mut cmd = plain("ssh", &dir);
        cmd.arg(HOST)
            .arg(format!("yes '{LINE}' | head -c {}", 16 * 1_048_576));
        pipe_output(cmd)
    });
    b.rate("an upload of 64 MB, sftp logging in", 3, 15.0, || {
        let _ = std::fs::remove_dir_all(&landing);
        std::fs::create_dir_all(&landing).unwrap();
        let took = copy(here.clone(), &blob, host.clone(), &landing);
        (64 * 1_048_576, took)
    });
    b.compare_rate("plain scp: an upload of 64 MB, logging in", 3, || {
        let _ = std::fs::remove_dir_all(&landing);
        std::fs::create_dir_all(&landing).unwrap();
        let mut cmd = plain("scp", &dir);
        cmd.arg("-q").arg(&blob).arg(format!("{HOST}:{landing}/"));
        (64 * 1_048_576, run_timed(cmd))
    });
    session::close_all();

    // With a master up the same work goes through its socket.
    let resolved = ssh::resolve(HOST).expect("the host's config");
    let mut master = None;
    b.timed(
        "a master coming up (two questions answered)",
        1,
        ms(4_000),
        || {
            let _ = std::fs::remove_file(format!("{}/known_hosts", sshd.dir));
            let t = Instant::now();
            let mut m = ssh::spawn_master(HOST, &resolved, ssh::IdleMode::None, handler())
                .expect("a master");
            while !ssh::master_alive(HOST) {
                assert!(m.is_running(), "ssh exited early: {:?}", m.last_line());
                assert!(t.elapsed() < Duration::from_secs(30), "no master");
                std::thread::sleep(ms(20));
            }
            let took = t.elapsed();
            master = Some(m);
            took
        },
    );
    if let Some(mut master) = master {
        // The answer kept from before it was up is forgotten.
        ssh::forget_master(HOST);
        assert!(remote::through_master(HOST));
        host_work(&mut b, "master", &tree, [ms(80), ms(250), ms(6_000)]);
        b.compare_timed(
            "plain ssh: a command through the master's socket",
            15,
            || {
                let mut cmd = plain("ssh", &dir);
                cmd.args([HOST, "true"]);
                run_timed(cmd)
            },
        );
        b.rate("a download of 64 MB through the master", 3, 25.0, || {
            let _ = std::fs::remove_dir_all(&landing);
            std::fs::create_dir_all(&landing).unwrap();
            let took = copy(host.clone(), &blob, here.clone(), &landing);
            (64 * 1_048_576, took)
        });
        b.compare_rate(
            "plain scp: a download of 64 MB through the socket",
            3,
            || {
                let _ = std::fs::remove_dir_all(&landing);
                std::fs::create_dir_all(&landing).unwrap();
                let mut cmd = plain("scp", &dir);
                cmd.arg("-q").arg(format!("{HOST}:{blob}")).arg(&landing);
                (64 * 1_048_576, run_timed(cmd))
            },
        );
        // Nothing started here outlives the measurements.
        master.terminate();
        let t = Instant::now();
        while master.is_running() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(ms(50));
        }
        master.kill();
    }
    session::close_all();
    drop(sshd);
    let _ = std::fs::remove_dir_all(&sockets);
    let _ = std::fs::remove_dir_all(&dir);
    b.finish()
}
