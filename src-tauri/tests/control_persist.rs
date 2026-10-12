//! An ssh the app starts for a host whose config says `ControlMaster auto`
//! and `ControlPersist` (a session here; a terminal or a copy is the same
//! ssh) becomes the master itself when none is up, and must not fork a
//! background master that outlives it: macOS 26 holds the app as still
//! running, and says so, while a process it started is alive after it
//! quit. Against the throwaway sshd started as this user.

mod common;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use shellder::core::prompt::{Kind, PromptHandler, PromptRequest};
use shellder::core::session;
use shellder::core::ssh;

const HOST: &str = "shjt-persist";

struct Answers;

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        match req.kind {
            Kind::Confirm => Some("yes".into()),
            Kind::Passphrase => Some(common::PASSPHRASE.into()),
            _ => None,
        }
    }
}

/// Whether a master answers on the host's ControlPath.
fn master_answers() -> bool {
    ssh::run(&["-O", "check", HOST]).status == 0
}

/// The `ps` lines that name the ControlPath directory: a ControlPersist
/// master calls itself `ssh: PATH [mux]`.
fn holders(cp_dir: &str) -> Vec<String> {
    let out = Command::new("ps")
        .args(["-axo", "command="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains(cp_dir))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_session_leaves_no_master_behind() {
    let Some(sshd) = common::Sshd::start("persist") else {
        return;
    };
    // Unix socket paths are short: keep the ControlPath under /tmp.
    let cp_dir = format!("/tmp/shjt-persist-{}", std::process::id());
    let _ = std::fs::remove_dir_all(&cp_dir);
    std::fs::create_dir_all(&cp_dir).unwrap();
    sshd.configure(
        HOST,
        &format!(
            "    ControlMaster auto\n    ControlPath {cp_dir}/cm-%C\n    ControlPersist 300\n"
        ),
    );
    session::set_prompt_handler(Arc::new(Answers));
    assert!(!master_answers(), "no master before the session");

    let s = session::for_host(HOST).expect("session");
    let out = s.run("echo hi");
    assert_eq!(
        (out.status, out.stdout.as_str()),
        (0, "hi\n"),
        "{}",
        out.text()
    );
    assert!(
        master_answers(),
        "the session's ssh is the master while the session is open"
    );

    // What quitting does. The ssh is reaped before close_all returns,
    // or nearly: give the socket a moment to go.
    session::close_all();
    let t = Instant::now();
    while (master_answers() || !holders(&cp_dir).is_empty()) && t.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !master_answers(),
        "a master still answers after the session closed"
    );
    assert!(
        holders(&cp_dir).is_empty(),
        "a master of ours persists after the session closed:\n{}",
        holders(&cp_dir).join("\n")
    );
    let _ = std::fs::remove_dir_all(&cp_dir);
}
