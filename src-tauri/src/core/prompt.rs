//! What ssh asks on its terminal, and how a question is told apart from
//! the others. The text comes off the master's pty, see `pty`.

use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Password,
    Passphrase,
    Totp,
    Confirm,
    Unknown,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Password => "password",
            Kind::Passphrase => "passphrase",
            Kind::Totp => "totp",
            Kind::Confirm => "confirm",
            Kind::Unknown => "unknown",
        }
    }
}

/// What kind of ssh process is asking: the daemon's master, a terminal
/// window's shell, the remote shell a Files window works through, or an
/// action of the Console run on a terminal of its own (a mount, a swap
/// file, a signal: the host itself asks on it, through sudo or polkit).
/// The answer comes from the same place; what differs is what a
/// cancelled or rejected question means (a master's switch goes off, an
/// action's handler ends it, a terminal's or a session's process is hung
/// up by the reader, see `pty::serve`) and what a repeated one means (a
/// master's stored secret was rejected; an action asks the user instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Master,
    Terminal,
    Session,
    Action,
}

/// One question, as the reader thread hands it to the app.
#[derive(Debug, Clone)]
pub struct PromptRequest {
    /// The host whose master asked (a ProxyJump hop asks under the final
    /// destination's name; the prompt text says who it really is).
    pub host: String,
    pub prompt: String,
    pub kind: Kind,
    /// pid of the asking process, so a second ask by the same process can
    /// be told from a hop or a retry asking the same thing.
    pub run_id: u32,
    pub origin: Origin,
}

/// Answers the questions. `ask` blocks for as long as it likes (keychain,
/// the user typing); None means nobody answers, and the attempt fails.
pub trait PromptHandler: Send + Sync {
    fn ask(&self, req: &PromptRequest) -> Option<String>;
    /// The master that asked is gone: a question still open can be dropped.
    fn master_gone(&self, _host: &str, _run_id: u32) {}
}

fn ci(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .expect("pattern")
}

fn totp_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        ci("verification code|one[- ]?time|otp|authenticat|passcode|token|2fa|two[- ]factor|\\bcode\\b|验证码|动态口令")
    })
}

/// What `classify` takes off the front of a question before it reads it,
/// in this order: the user/host prefixes ssh adds ("(alice@otp-gw)
/// Password:" / "alice@otp-gw's password:") and what sudo puts in
/// brackets.
fn prefixes() -> &'static [Regex; 3] {
    static RES: OnceLock<[Regex; 3]> = OnceLock::new();
    RES.get_or_init(|| {
        [
            Regex::new("^\\([^)]*\\)\\s*").unwrap(),
            Regex::new("^\\[[^\\]]*\\]\\s*").unwrap(),
            Regex::new("^\\S+@\\S+'s\\s+").unwrap(),
        ]
    })
}

/// The last line is the question itself; the lines above it (a host key's
/// fingerprint, a banner) are context.
pub fn last_line(prompt: &str) -> &str {
    prompt.trim_end().lines().last().unwrap_or("").trim()
}

pub fn classify(prompt: &str) -> Kind {
    let mut p = last_line(prompt).to_lowercase();
    // Drop the user/host prefixes ssh adds ("(alice@otp-gw) Password:" /
    // "alice@otp-gw's password:") so host or user names cannot be mistaken
    // for a prompt type, and what sudo puts in brackets ("[sudo] password
    // for alice:", "[sudo: authenticate] Password:" on some hosts), whose
    // "authenticate" is not a code's.
    for re in prefixes() {
        p = re.replace(&p, "").into_owned();
    }
    if p.contains("passphrase") {
        return Kind::Passphrase;
    }
    if totp_pattern().is_match(&p) {
        return Kind::Totp;
    }
    if p.contains("password") || p.contains("密码") {
        return Kind::Password;
    }
    if p.contains("yes/no") || p.contains("are you sure") {
        return Kind::Confirm;
    }
    Kind::Unknown
}

/// "user@host" that ssh put in front of a password prompt, if any.
pub fn user_host(prompt: &str) -> Option<(String, String)> {
    static RES: OnceLock<[Regex; 2]> = OnceLock::new();
    let res = RES.get_or_init(|| {
        [
            Regex::new("^\\(([^@\\s()]+)@([^)\\s]+)\\)").unwrap(),
            Regex::new("^([^@\\s]+)@(\\S+)'s\\s").unwrap(),
        ]
    });
    let line = last_line(prompt);
    for re in res {
        if let Some(m) = re.captures(line) {
            return Some((m[1].to_string(), m[2].to_string()));
        }
    }
    None
}

/// Key file named in "Enter passphrase for key '/path':".
pub fn key_path(prompt: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new("for key '([^']+)'").unwrap());
    re.captures(prompt).map(|m| m[1].to_string())
}

/// Does the text ssh left on the terminal without a newline look like it is
/// waiting for an answer? ssh's own prompts end in ": " or "? ".
pub fn looks_like_question(tail: &str) -> bool {
    let t = tail.trim_end();
    !t.is_empty() && (t.ends_with(':') || t.ends_with('?'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_ssh_prompts() {
        assert_eq!(classify("alice@host's password: "), Kind::Password);
        assert_eq!(classify("(alice@otp-gw) Password:"), Kind::Password);
        assert_eq!(classify("(alice@otp-gw) Verification code: "), Kind::Totp);
        assert_eq!(
            classify("Enter passphrase for key '/home/a/.ssh/id_ed25519': "),
            Kind::Passphrase
        );
        assert_eq!(
            classify("The authenticity of host 'h (1.2.3.4)' can't be established.\nED25519 key fingerprint is SHA256:x.\nAre you sure you want to continue connecting (yes/no/[fingerprint])? "),
            Kind::Confirm
        );
        assert_eq!(classify("(t@127.0.0.1) 密码："), Kind::Password);
        assert_eq!(classify("Something else: "), Kind::Unknown);
    }

    #[test]
    fn sudo_prompts_are_passwords() {
        assert_eq!(classify("[sudo] password for alice: "), Kind::Password);
        assert_eq!(classify("[sudo: authenticate] Password:"), Kind::Password);
        assert_eq!(
            classify("Sorry, try again.\n[sudo] password for alice: "),
            Kind::Password
        );
        // polkit's text agent, its frame as context.
        assert_eq!(
            classify("==== AUTHENTICATING FOR org.freedesktop.udisks2.filesystem-mount ====\nAuthentication is required to mount X\nAuthenticating as: alice\nPassword: "),
            Kind::Password
        );
    }

    #[test]
    fn host_names_do_not_count_as_kinds() {
        // "otp" in the host name must not make it a TOTP prompt.
        assert_eq!(classify("(alice@otp-gw) Password:"), Kind::Password);
        assert_eq!(classify("code@token-host's password:"), Kind::Password);
    }

    #[test]
    fn user_host_forms() {
        assert_eq!(
            user_host("(alice@gw) Password:"),
            Some(("alice".into(), "gw".into()))
        );
        assert_eq!(
            user_host("bob@h.example's password: "),
            Some(("bob".into(), "h.example".into()))
        );
        assert_eq!(user_host("Password:"), None);
    }

    #[test]
    fn key_path_form() {
        assert_eq!(
            key_path("Enter passphrase for key '/x/id': ").as_deref(),
            Some("/x/id")
        );
    }

    #[test]
    fn questions_end_in_colon_or_question_mark() {
        assert!(looks_like_question("Password: "));
        assert!(looks_like_question("continue connecting (yes/no)? "));
        assert!(!looks_like_question("Warning: something happened"));
        assert!(!looks_like_question(""));
    }
}
