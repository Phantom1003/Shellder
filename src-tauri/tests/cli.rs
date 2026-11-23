//! The subcommands, run as the binary itself with a state directory and
//! an ssh config of its own and the keychain off: what they print, how
//! they exit, and what they leave in the prefs. Nothing is connected to
//! (`hosts` and `status` only ask ssh what the config says, and whether a
//! master's socket answers).

use std::io::Write;
use std::process::{Command, Stdio};

struct Cli {
    dir: String,
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

impl Cli {
    /// A config with a host that has a ControlPath, one that jumps
    /// through it, and one in an included file.
    fn new(name: &str) -> Cli {
        let dir = std::env::temp_dir().join(format!("shjt-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("state")).unwrap();
        let d = dir.to_str().unwrap().to_string();
        std::fs::write(
            format!("{d}/config"),
            format!(
                "Host shjt-gate\n    HostName 127.0.0.1\n    ControlPath {d}/cm-gate\n\
                 Host shjt-inner\n    HostName 127.0.0.2\n    ProxyJump shjt-gate\n\
                 Include {d}/more.conf\n"
            ),
        )
        .unwrap();
        std::fs::write(
            format!("{d}/more.conf"),
            "Host shjt-more\n    HostName 127.0.0.3\n",
        )
        .unwrap();
        Cli { dir: d }
    }

    fn state(&self, file: &str) -> String {
        format!("{}/state/{file}", self.dir)
    }

    fn run(&self, args: &[&str]) -> Ran {
        self.run_fed(args, None)
    }

    /// With `input` on its standard input (a secret piped in), or nothing
    /// to read there.
    fn run_fed(&self, args: &[&str], input: Option<&str>) -> Ran {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shellder"))
            .args(args)
            .env("SHELLDER_SSH_CONFIG", format!("{}/config", self.dir))
            .env("SHELLDER_STATE_DIR", format!("{}/state", self.dir))
            .env("SHELLDER_NO_KEYCHAIN", "1")
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(text) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        }
        let out = child.wait_with_output().unwrap();
        Ran {
            code: out.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn locked(&self) -> Vec<String> {
        let Ok(text) = std::fs::read_to_string(self.state("prefs.json")) else {
            return vec![];
        };
        let prefs: serde_json::Value = serde_json::from_str(&text).unwrap();
        prefs["lockedHosts"]
            .as_array()
            .map(|a| a.iter().map(|h| h.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The line of `text` about `host`: the one whose first word, after the
/// lock's mark, is its name.
fn line<'a>(text: &'a str, host: &str) -> &'a str {
    text.lines()
        .find(|l| l.trim_start_matches(['*', ' ']).split(' ').next() == Some(host))
        .unwrap_or_else(|| panic!("no line for {host} in:\n{text}"))
}

#[test]
fn help_and_what_is_not_a_command() {
    let cli = Cli::new("help");
    for flag in ["help", "-h", "--help"] {
        let r = cli.run(&[flag]);
        assert_eq!(r.code, 0, "{flag}: {}", r.err);
        assert!(r.out.starts_with("usage: shellder"), "{}", r.out);
        for command in [
            "hosts",
            "status",
            "lock HOST",
            "unlock HOST",
            "add-secret",
            "del-secret",
            "install",
            "uninstall",
        ] {
            assert!(r.out.contains(command), "{command} is not in the help");
        }
    }
    let r = cli.run(&["bogus"]);
    assert_eq!(r.code, 2);
    assert!(r.err.starts_with("unknown command: bogus"), "{}", r.err);
    assert!(
        r.err.contains("usage: shellder"),
        "the usage follows: {}",
        r.err
    );
    assert!(r.out.is_empty());
    // A subcommand's own arguments missing or wrong.
    for args in [
        &["lock"][..],
        &["unlock"],
        &["add-secret", "shjt-gate"],
        &["add-secret", "shjt-gate", "pin"],
        &["del-secret"],
        &["del-secret", "shjt-gate", "password", "more"],
    ] {
        let r = cli.run(args);
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.err.starts_with("usage: "), "{args:?}: {}", r.err);
    }
    // None of that made a prefs file.
    assert!(!std::path::Path::new(&cli.state("prefs.json")).exists());
}

/// `hosts` lists the config's hosts, the included file's too, each with
/// the ControlPath ssh resolves for it, and says which files it read.
#[test]
fn hosts_are_listed_with_their_sockets() {
    let cli = Cli::new("hosts");
    let r = cli.run(&["hosts"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let names: Vec<&str> = r
        .out
        .lines()
        .filter(|l| !l.starts_with('('))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(names, ["shjt-gate", "shjt-inner", "shjt-more"]);
    assert!(
        line(&r.out, "shjt-gate").ends_with(&format!("{}/cm-gate", cli.dir)),
        "{}",
        r.out
    );
    assert!(line(&r.out, "shjt-inner").ends_with("(no ControlPath)"));
    assert!(line(&r.out, "shjt-more").ends_with("(no ControlPath)"));
    let files = r.out.lines().last().unwrap();
    assert!(files.starts_with("(* = locked, from "), "{files}");
    assert!(
        files.contains("/config, ") && files.ends_with("/more.conf)"),
        "{files}"
    );
    assert!(
        !r.out.lines().any(|l| l.starts_with("* ")),
        "nothing is locked: {}",
        r.out
    );
}

/// Locking takes the jump host along, unlocking the jump host takes the
/// hosts behind it; `hosts` marks the locked ones and `status` lists
/// them, a master that is not up as down.
#[test]
fn a_lock_takes_the_jump_host_along() {
    let cli = Cli::new("lock");
    let r = cli.run(&["status"]);
    assert_eq!(r.code, 0);
    assert!(r.out.starts_with("no hosts are locked"), "{}", r.out);

    let r = cli.run(&["lock", "shjt-inner"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.starts_with("shjt-inner: locked"), "{}", r.out);
    assert!(
        r.out
            .contains("shjt-gate: locked (jump host for shjt-inner)"),
        "{}",
        r.out
    );
    assert!(r.err.is_empty(), "{}", r.err);
    assert_eq!(cli.locked(), ["shjt-inner", "shjt-gate"]);
    // Again changes nothing.
    assert_eq!(cli.run(&["lock", "shjt-inner"]).code, 0);
    assert_eq!(cli.locked(), ["shjt-inner", "shjt-gate"]);

    let r = cli.run(&["hosts"]);
    assert!(line(&r.out, "shjt-gate").starts_with("* "), "{}", r.out);
    assert!(line(&r.out, "shjt-inner").starts_with("* "), "{}", r.out);
    assert!(line(&r.out, "shjt-more").starts_with("  "), "{}", r.out);

    let r = cli.run(&["status"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.out.lines().count(), 2, "{}", r.out);
    let gate = line(&r.out, "shjt-gate");
    assert!(
        gate.contains(" down ") && gate.ends_with("/cm-gate"),
        "{gate}"
    );
    assert!(
        line(&r.out, "shjt-inner").contains("no ControlPath"),
        "{}",
        r.out
    );

    // The state directory is this account's alone.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let state = std::fs::metadata(format!("{}/state", cli.dir)).unwrap();
        assert_eq!(state.permissions().mode() & 0o777, 0o700);
    }

    let r = cli.run(&["unlock", "shjt-gate"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.starts_with("shjt-gate: unlocked"), "{}", r.out);
    assert!(
        r.out
            .contains("shjt-inner: unlocked (jumps through shjt-gate)"),
        "{}",
        r.out
    );
    assert!(cli.locked().is_empty(), "{:?}", cli.locked());

    // A name the config does not have is locked all the same, with a word
    // about it, and unlocking a host leaves the others.
    let r = cli.run(&["lock", "shjt-elsewhere"]);
    assert_eq!(r.code, 0);
    assert!(r.err.contains("is not a Host entry"), "{}", r.err);
    assert_eq!(cli.run(&["lock", "shjt-more"]).code, 0);
    assert_eq!(cli.run(&["unlock", "shjt-elsewhere"]).code, 0);
    assert_eq!(cli.locked(), ["shjt-more"]);
}

/// A secret comes on standard input when that is no terminal. With the
/// keychain off nothing is stored, and the command says so; a TOTP
/// secret that is none is refused before the keychain is asked. Whatever
/// happens, the secret is in no file the command leaves behind.
#[test]
fn a_secret_is_read_from_a_pipe_and_written_nowhere() {
    let cli = Cli::new("secret");
    let secret = "correct-horse-battery-7391";
    let r = cli.run_fed(
        &["add-secret", "shjt-gate", "password"],
        Some(&format!("{secret}\n")),
    );
    assert_eq!(r.code, 1);
    assert!(r.err.contains("SHELLDER_NO_KEYCHAIN"), "{}", r.err);
    assert!(
        r.out.is_empty(),
        "no prompt is printed for a pipe: {}",
        r.out
    );

    let r = cli.run_fed(
        &["add-secret", "shjt-gate", "totp"],
        Some("not base32 at all!\n"),
    );
    assert_eq!(r.code, 1);
    assert!(r.err.starts_with("error: "), "{}", r.err);
    assert!(!r.err.contains("SHELLDER_NO_KEYCHAIN"), "{}", r.err);

    // Nothing to read, or an empty line: nothing to store.
    for input in [None, Some("\n"), Some("")] {
        let r = cli.run_fed(&["add-secret", "shjt-gate", "passphrase"], input);
        assert_eq!((r.code, r.err.trim()), (1, "aborted"), "{input:?}");
    }

    let r = cli.run(&["del-secret", "shjt-gate", "password"]);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("SHELLDER_NO_KEYCHAIN"), "{}", r.err);

    for entry in std::fs::read_dir(format!("{}/state", cli.dir)).unwrap() {
        let path = entry.unwrap().path();
        let Ok(text) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&text);
        assert!(!text.contains(secret), "the secret is in {path:?}");
    }
}
