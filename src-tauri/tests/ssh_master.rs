//! A real master against a throwaway sshd: the host key confirmation and
//! the key passphrase both come down the pty and are answered, and the
//! master's socket comes up. Needs /usr/sbin/sshd and ssh-keygen; the
//! daemon runs as this user on a high port, so only key authentication is
//! possible (a password needs root).

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shellder::core::prompt::{Kind, PromptHandler, PromptRequest};
use shellder::core::ssh;

struct Answers {
    seen: Mutex<Vec<Kind>>,
}

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        self.seen.lock().unwrap().push(req.kind);
        match req.kind {
            Kind::Confirm => Some("yes".into()),
            Kind::Passphrase => Some("s3cret".into()),
            _ => None,
        }
    }
}

fn run(cmd: &str, args: &[&str]) {
    let st = Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    assert!(st.success(), "{cmd} {args:?}");
}

#[test]
fn a_master_logs_in_through_its_pty() {
    if !std::path::Path::new("/usr/sbin/sshd").exists() {
        eprintln!("no sshd here, skipping");
        return;
    }
    let dir = std::env::temp_dir().join(format!("shjt-master-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_str().unwrap().to_string();
    // Unix socket paths are short: keep the ControlPath under /tmp.
    let cp_dir = format!("/tmp/shjt-cm-{}", std::process::id());
    std::fs::create_dir_all(&cp_dir).unwrap();
    std::env::set_var("SHELLDER_SSH_CONFIG", format!("{d}/config"));
    std::env::set_var("SHELLDER_STATE_DIR", &d);

    run(
        "ssh-keygen",
        &[
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            &format!("{d}/hostkey"),
        ],
    );
    run(
        "ssh-keygen",
        &[
            "-q",
            "-t",
            "ed25519",
            "-N",
            "s3cret",
            "-f",
            &format!("{d}/id"),
        ],
    );
    std::fs::copy(format!("{d}/id.pub"), format!("{d}/authorized_keys")).unwrap();
    let port = 2200 + (std::process::id() % 1000) as u16;
    let user = std::env::var("USER").unwrap();
    std::fs::write(
        format!("{d}/sshd_config"),
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/hostkey\nAuthorizedKeysFile {d}/authorized_keys\n\
             PasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nPidFile none\n\
             LogLevel ERROR\nAllowUsers {user}\n"
        ),
    )
    .unwrap();
    // Killed when the test ends, however it ends: a daemon left behind holds
    // the harness's pipes open.
    struct Daemon(std::process::Child);
    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let sshd = Daemon(
        Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f", &format!("{d}/sshd_config")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let up = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            up.elapsed() < Duration::from_secs(10),
            "sshd did not come up"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // StrictHostKeyChecking ask: the first prompt is the host key, the
    // second the passphrase of the key file.
    std::fs::write(
        format!("{d}/config"),
        format!(
            "Host shjt-master\n    HostName 127.0.0.1\n    Port {port}\n    User {user}\n    IdentityFile {d}/id\n    IdentitiesOnly yes\n\
             ControlMaster auto\n    ControlPath {cp_dir}/cm-%C\n    StrictHostKeyChecking ask\n    UserKnownHostsFile {d}/known_hosts\n    LogLevel ERROR\n"
        ),
    )
    .unwrap();

    let resolved = ssh::resolve("shjt-master").unwrap();
    assert!(resolved.control_path.is_some());
    let handler = Arc::new(Answers {
        seen: Mutex::new(vec![]),
    });
    let h: Arc<dyn PromptHandler> = handler.clone();
    let mut master = ssh::spawn_master("shjt-master", &resolved, ssh::IdleMode::None, h).unwrap();
    let start = Instant::now();
    let mut alive = false;
    while start.elapsed() < Duration::from_secs(30) {
        if ssh::master_alive("shjt-master") {
            alive = true;
            break;
        }
        assert!(
            master.is_running(),
            "ssh exited early: {:?}; prompts seen: {:?}",
            master.last_line(),
            handler.seen.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let seen = handler.seen.lock().unwrap().clone();
    master.terminate();
    std::thread::sleep(Duration::from_millis(300));
    master.kill();
    drop(sshd);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cp_dir);
    assert!(
        alive,
        "the master never came up; prompts seen: {seen:?}, last line: {:?}",
        master.last_line()
    );
    assert_eq!(seen, vec![Kind::Confirm, Kind::Passphrase]);
}
