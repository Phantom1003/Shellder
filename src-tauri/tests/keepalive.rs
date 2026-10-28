//! A host without a ControlPath gets a master all the same: the same ssh
//! without the socket, kept alive for its port forwards, and the daemon
//! learns it is up from ssh's own "Authenticated to" line instead of a
//! socket check. Against a throwaway sshd started as this user.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shellder::core::prompt::{Kind, PromptHandler, PromptRequest};
use shellder::core::ssh;

struct Answers(Mutex<Vec<Kind>>);

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        self.0.lock().unwrap().push(req.kind);
        match req.kind {
            Kind::Confirm => Some("yes".into()),
            Kind::Passphrase => Some(common::PASSPHRASE.into()),
            _ => None,
        }
    }
}

#[test]
fn a_master_without_a_socket_comes_up() {
    let Some(sshd) = common::Sshd::start("keepalive") else {
        return;
    };
    sshd.configure("shjt-plain", "");
    let resolved = ssh::resolve("shjt-plain").unwrap();
    assert!(resolved.control_path.is_none());
    assert!(!ssh::multiplexed(&resolved));
    let args = ssh::master_args("shjt-plain", &resolved, ssh::IdleMode::None);
    assert!(!args.iter().any(|a| a == "-M"), "{args:?}");
    assert!(args.iter().any(|a| a == "LogLevel=VERBOSE"), "{args:?}");
    let handler = Arc::new(Answers(Mutex::new(vec![])));
    let h: Arc<dyn PromptHandler> = handler.clone();
    let mut master = ssh::spawn_master("shjt-plain", &resolved, ssh::IdleMode::None, h).unwrap();
    let start = Instant::now();
    while !master.authenticated() && start.elapsed() < Duration::from_secs(30) {
        assert!(
            master.is_running(),
            "ssh exited early: {:?}; prompts: {:?}",
            master.last_line(),
            handler.0.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        master.authenticated(),
        "never authenticated: {:?}",
        master.last_line()
    );
    assert!(!ssh::master_alive("shjt-plain"), "no socket to check");
    assert!(master.is_running());
    assert_eq!(
        *handler.0.lock().unwrap(),
        vec![Kind::Confirm, Kind::Passphrase]
    );
    master.terminate();
    let t = Instant::now();
    while master.is_running() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    master.kill();
    // The byte counts VERBOSE prints at exit are not the reason it ended.
    assert!(
        master
            .last_line()
            .map(|l| !l.starts_with("Transferred") && !l.starts_with("Bytes"))
            .unwrap_or(true),
        "{:?}",
        master.last_line()
    );

    // What a terminal or a session resolves comes from the last `ssh -G`
    // until the config changes, and from a new one then.
    assert_eq!(ssh::resolve_cached("shjt-plain").unwrap(), resolved);
    let config = format!("{}/config", sshd.dir);
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("    ServerAliveInterval 7\n");
    std::fs::write(&config, text).unwrap();
    let changed = ssh::resolve_cached("shjt-plain").unwrap();
    assert_eq!(changed.server_alive_interval, 7);
    assert!(ssh::keepalive_args(&changed).is_empty(), "the user's own");
}
