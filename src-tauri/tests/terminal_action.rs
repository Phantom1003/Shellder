//! An action of the Console that the host wants a password for runs on
//! a terminal of the app's own: ssh's standard streams are its asking
//! pty, so what the remote command asks on its terminal (sudo, here a
//! script reading its tty) comes to the prompt handler like ssh's own
//! questions, and what it prints comes back as the output, the questions
//! left out. Against a throwaway sshd started as this user.

mod common;

use std::sync::{Arc, Mutex};

use shellder::core::prompt::{Kind, Origin, PromptHandler, PromptRequest};
use shellder::core::ssh;

struct Answers(Mutex<Vec<(Kind, String)>>);

impl PromptHandler for Answers {
    fn ask(&self, req: &PromptRequest) -> Option<String> {
        assert_eq!(req.origin, Origin::Action);
        self.0
            .lock()
            .unwrap()
            .push((req.kind, req.prompt.trim().to_string()));
        match req.kind {
            Kind::Confirm => Some("yes".into()),
            Kind::Passphrase => Some(common::PASSPHRASE.into()),
            Kind::Password => Some("pw".into()),
            _ => None,
        }
    }
}

/// What sudo does on its terminal: a prompt, the answer read with echo
/// off, a star per character (pwfeedback) rubbed out again, a newline;
/// then the command's own words.
const ASKS: &str = "sh -c 'echo before; stty -echo; printf \"[sudo] password for %s: \" \"$(id -un)\"; read -r p; printf \"**\\b \\b\\b \\b\"; stty echo; echo; echo \"got $p\"; [ \"$p\" = pw ]'";

#[test]
fn the_host_asks_on_the_terminal_and_the_output_comes_back() {
    let Some(sshd) = common::Sshd::start("terminal-action") else {
        return;
    };
    sshd.configure("shjt-action", "");
    let handler = Arc::new(Answers(Mutex::new(vec![])));
    let h: Arc<dyn PromptHandler> = handler.clone();
    let out = ssh::run_on_terminal("shjt-action", ASKS, Origin::Action, h);
    let seen = handler.0.lock().unwrap();
    assert_eq!(out.status, 0, "{} / {:?}", out.text(), seen);
    // The host key and the key's passphrase (ssh's own, on /dev/tty),
    // then the command's question (on its stdin, the same pty).
    let kinds: Vec<Kind> = seen.iter().map(|(k, _)| *k).collect();
    assert_eq!(kinds, [Kind::Confirm, Kind::Passphrase, Kind::Password]);
    assert!(
        seen[2].1.ends_with(&format!("password for {}:", sshd.user)),
        "{}",
        seen[2].1
    );
    // What the command printed after its question; the question, and
    // what came before it as its context, left out.
    assert_eq!(out.stdout.trim(), "got pw", "{:?}", out.stdout);
    drop(seen);

    // A command that fails: its status, and its words, which with a
    // remote pty come down it whether they went to stdout or stderr.
    let h: Arc<dyn PromptHandler> = Arc::new(Answers(Mutex::new(vec![])));
    let out = ssh::run_on_terminal(
        "shjt-action",
        "sh -c 'echo nope >&2; exit 3'",
        Origin::Action,
        h,
    );
    assert_eq!(out.status, 3, "{:?}", out);
    assert_eq!(out.stdout.trim(), "nope", "{:?}", out);
}
