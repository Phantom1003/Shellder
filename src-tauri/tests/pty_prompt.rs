//! The pty carries only questions: a process that opens /dev/tty to ask
//! for a password gets its answer, while its stdout stays on the pipe.
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use shellder::core::prompt::{Kind, Origin, PromptHandler, PromptRequest};
use shellder::core::pty::{self, Pty};

struct Answers {
    seen: Mutex<Vec<(Kind, String)>>,
}

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        self.seen
            .lock()
            .unwrap()
            .push((req.kind, req.prompt.clone()));
        match req.kind {
            Kind::Password => Some("pw".into()),
            Kind::Confirm => Some("yes".into()),
            Kind::Unknown => Some("ok".into()),
            _ => None,
        }
    }
}

#[test]
fn questions_on_the_tty_are_answered() {
    let pty = Pty::open().unwrap();
    let script = r#"
printf 'The authenticity of host x cannot be established.\nAre you sure you want to continue connecting (yes/no)? ' > /dev/tty
read a < /dev/tty
stty -echo < /dev/tty
printf 'alice@host'"'"'s password: ' > /dev/tty
read p < /dev/tty
stty echo < /dev/tty
printf '\n' > /dev/tty
# sudo's pwfeedback: a star per character, rubbed out once the password
# is in; and the same from a sudo that leaves them standing.
printf '**\b \b\b \b\n**\n' > /dev/tty
printf 'Something else: ' > /dev/tty
read s < /dev/tty
echo "got $a $p $s"
[ "$a" = yes ] && [ "$p" = pw ] && [ "$s" = ok ]
"#;
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    pty.attach(&mut cmd);
    let child = cmd.spawn().unwrap();
    let handler = Arc::new(Answers {
        seen: Mutex::new(vec![]),
    });
    let h: Arc<dyn PromptHandler> = handler.clone();
    let master = pty.master;
    let t = std::thread::spawn(move || pty::serve("x".into(), 1, Origin::Master, master, h, None));
    let out = child.wait_with_output().unwrap();
    // The process is gone: close the slave, as dropping the MasterProc does
    // in ssh.rs. On macOS the reader would have seen the hang-up anyway
    // (the kernel revokes the terminal when its session leader exits); on
    // Linux the master only hangs up once the last slave fd is closed, and
    // this one would otherwise stay open past the join below, forever.
    drop(pty.slave);
    t.join().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "got yes pw ok");
    assert!(
        out.status.success(),
        "{:?} {:?}",
        out.status.code(),
        out.status.signal()
    );
    let seen = handler.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0].0, Kind::Confirm);
    assert!(seen[0].1.starts_with("The authenticity"), "{}", seen[0].1);
    assert_eq!(seen[1].0, Kind::Password);
    assert_eq!(seen[1].1, "alice@host's password:");
    // Neither form of the feedback is context for the next question.
    assert_eq!(seen[2].0, Kind::Unknown);
    assert_eq!(seen[2].1, "Something else:");
}

/// A terminal on this machine (`ssh::LOCAL`): the login shell on the
/// screen pty, which is its controlling terminal, so that `stty` reads
/// the size there and a resize reaches it; what it prints comes back,
/// and it ends with its status, no ssh anywhere.
#[test]
fn a_local_terminal_is_the_login_shell() {
    use shellder::core::ssh;
    use shellder::core::term::{Launch, Terminal};
    use std::time::{Duration, Instant};
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let exited = Arc::new(Mutex::new(None::<i32>));
    let o = output.clone();
    let x = exited.clone();
    let handler = Arc::new(Answers {
        seen: Mutex::new(vec![]),
    });
    let h: Arc<dyn PromptHandler> = handler.clone();
    let term = Terminal::open(
        ssh::LOCAL,
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
    assert_eq!(term.host, ssh::LOCAL);
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
    // The shell's own tty is the screen: its size is the terminal's.
    term.write(b"echo ma''rco; stty size; tty\n").unwrap();
    wait_for("marco");
    wait_for("24 80");
    wait_for("/dev/");
    term.resize(100, 30);
    term.write(b"stty size\n").unwrap();
    wait_for("30 100");
    term.write(b"exit 3\n").unwrap();
    let t = Instant::now();
    while exited.lock().unwrap().is_none() {
        assert!(
            t.elapsed() < Duration::from_secs(20),
            "the shell did not exit"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(*exited.lock().unwrap(), Some(3));
    assert!(!term.is_running());
    assert!(handler.seen.lock().unwrap().is_empty(), "nothing asked");
}
