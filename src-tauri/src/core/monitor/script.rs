//! The scripts, kept as shell files beside this one: `common.sh` (what
//! the reading and the tree start with), `read.sh` (the reading),
//! `tree.sh` (the process tree) and `actions.sh` (what the Status tab
//! does on the host). Each is joined at load time into one line of POSIX
//! sh, so that it means the same under any login shell: it is wrapped in
//! `sh -c '…'`, which is why it carries no single quote, csh would take a
//! `!` for its history and choke on a newline, and fish's single quotes
//! make one backslash of two.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const COMMON: &str = include_str!("common.sh");
const READ: &str = include_str!("read.sh");
pub(super) const TREE: &str = include_str!("tree.sh");
pub(super) const ACTIONS: &str = include_str!("actions.sh");

/// The parts of the Status tab that can be left unread: a card each.
pub const PARTS: &[&str] = &[
    "cpu",
    "memory",
    "gpu",
    "disks",
    "network",
    "processes",
    "logins",
];

/// The parts the other tabs read, left unread like a card while the
/// window does not show them: `sessions` (tmux, screen, VNC: the
/// snapshot's `tmux`, `screen` and `vnc`), `jobs` (Slurm and PBS:
/// `slurm`, `pbs`, whose scheduler is asked over the network),
/// `containers` (`docker`) and `ports` (`listeners`).
pub const TAB_PARTS: &[&str] = &["sessions", "jobs", "containers", "ports"];

/// What starts a section in the scripts' output: this, the run's nonce,
/// a colon, and the section's name.
const MARK: &str = "@@shellder-console";

/// The marker of a run: `@@shellder-console-NONCE:`, or with an empty
/// nonce `@@shellder-console:`, what `command` and `parse` use.
pub(super) fn marker(nonce: &str) -> String {
    if nonce.is_empty() {
        format!("{MARK}:")
    } else {
        format!("{MARK}-{nonce}:")
    }
}

/// A marker nothing on the host can know beforehand: a directory name
/// with a newline in it (a tmux pane's, a screen shell's, a job's) would
/// otherwise start a section of its choosing, and the first section of a
/// name is the one read.
pub(super) fn fresh_marker() -> String {
    static RUNS: AtomicU64 = AtomicU64::new(0);
    // RandomState's keys come from the OS, fresh for each process.
    let mut h = RandomState::new().build_hasher();
    h.write_u64(RUNS.fetch_add(1, Ordering::Relaxed));
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    marker(&format!("{:016x}", h.finish()))
}

/// The pieces of a script file in order, each with its tag. A line
/// `#@ tag` starts a piece and the lines after it make it up, joined into
/// one with a space between them: so each line of the file ends where the
/// next may follow on the same line (after a `;`, `then`, `do`, `{`, a
/// pipe…), and the file is a script that `sh` runs as it is. Other lines
/// that start with `#` are comments, so no line of a piece may start with
/// one (a tmux format goes after something on its line).
pub(super) fn pieces(src: &str) -> Vec<(&str, String)> {
    let mut out: Vec<(&str, String)> = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if let Some(tag) = line.strip_prefix("#@") {
            out.push((tag.trim(), String::new()));
        } else if line.is_empty() || line.starts_with('#') {
            continue;
        } else if let Some((_, text)) = out.last_mut() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(line);
        }
    }
    out
}

/// The pieces of `src` whose tag `keep` takes, in the file's order, as
/// one line.
pub(super) fn join(src: &str, keep: impl Fn(&str) -> bool) -> String {
    pieces(src)
        .into_iter()
        .filter(|(tag, _)| keep(tag))
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the reading and the tree start with, its sections started by
/// `marker`.
pub(super) fn common(marker: &str) -> String {
    join(COMMON, |_| true).replace("@MARK@", marker)
}

/// The reading of the parts in `parts`, under `marker`.
pub(super) fn reading_script(parts: &[String], marker: &str) -> String {
    let body = join(READ, |tag| {
        tag == "always" || parts.iter().any(|p| p == tag)
    });
    format!("{} {body}", common(marker))
}

/// The reading as the remote login shell gets it, under `marker`.
pub(super) fn reading(parts: &[String], marker: &str) -> String {
    format!("sh -c '{}'", reading_script(parts, marker))
}

/// The script as the remote login shell gets it, reading the parts in
/// `parts` (`PARTS`, `TAB_PARTS`; a name not among them is ignored) and
/// the head every reading has: the host's name, system, uptime, load and
/// processors. Its sections start with the plain marker that `parse`
/// reads; `fetch` gives every run a marker of its own.
pub fn command(parts: &[String]) -> String {
    reading(parts, &marker(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every script that goes inside the single quotes of a `sh -c`,
    /// put together the ways the code does.
    fn every_script() -> Vec<String> {
        let every: Vec<String> = PARTS
            .iter()
            .chain(TAB_PARTS)
            .map(|p| p.to_string())
            .collect();
        let mut all = vec![
            reading_script(&every, &fresh_marker()),
            reading_script(&[], &marker("")),
        ];
        all.extend(super::super::tree::every_script());
        all.extend(super::super::actions::every_script());
        all
    }

    #[test]
    fn the_scripts_fit_in_single_quotes() {
        for inner in every_script() {
            assert!(
                !inner.contains('\''),
                "a single quote would end the sh -c argument: {inner}"
            );
            assert!(
                !inner.contains('\n'),
                "a csh login shell would choke on a newline"
            );
            assert!(!inner.contains('!'), "csh would expand it as history");
            assert!(
                !inner.contains("\\\\"),
                "fish's single quotes would make one backslash of two"
            );
            assert!(!inner.contains("@MARK@"));
        }
    }

    /// What `sh -n` says of each script: a line of a file that does not
    /// end where the next may follow comes out as a syntax error.
    #[test]
    fn the_scripts_are_sh() {
        for script in every_script() {
            let out = std::process::Command::new("/bin/sh")
                .args(["-n", "-c", &script])
                .output()
                .expect("sh");
            assert!(
                out.status.success(),
                "{script}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn a_file_becomes_pieces() {
        let src = "# a comment\n#@ one\nif x; then\n  y;\nfi;\n\n# inside\nz;\n#@ two\nw\n";
        assert_eq!(
            pieces(src),
            vec![
                ("one", "if x; then y; fi; z;".to_string()),
                ("two", "w".to_string())
            ]
        );
        assert_eq!(join(src, |t| t == "two"), "w");
        for (tag, _) in pieces(READ) {
            assert!(
                tag == "always" || PARTS.contains(&tag) || TAB_PARTS.contains(&tag),
                "{tag} is not a part"
            );
        }
    }

    #[test]
    fn the_parts_not_asked_for_are_not_read() {
        let all: Vec<String> = PARTS.iter().map(|p| p.to_string()).collect();
        let full = command(&all);
        assert!(full.starts_with("sh -c '"));
        // Without the processes and the disks their commands are not
        // run; the head stays.
        let fewer = command(&["cpu".to_string(), "memory".to_string()]);
        assert!(full.contains("S ps;") && full.contains("S df;"));
        assert!(!fewer.contains("S ps;") && !fewer.contains("S df;"));
        assert!(!fewer.contains("nvidia-smi") && !fewer.contains("S who;"));
        assert!(fewer.contains("S stat;") && fewer.contains("S mem;"));
        for head in [
            "S hostname;",
            "S uname;",
            "S os;",
            "S now;",
            "S uptime;",
            "S load;",
            "S ncpu;",
        ] {
            assert!(command(&[]).contains(head), "{head}");
        }
        // The tabs' parts: none of their commands unless asked for, each
        // on its own.
        let tabs = [
            (
                "sessions",
                &["S tmux;", "S screen;", "S vnc;", "launchctl"][..],
            ),
            ("jobs", &["squeue", "qstat", "qselect"][..]),
            ("containers", &["S docker;", "podman"][..]),
            ("ports", &["S ports;", "ss -ltnp", "lsof"][..]),
        ];
        assert_eq!(tabs.len(), TAB_PARTS.len());
        for (part, words) in tabs {
            assert!(TAB_PARTS.contains(&part));
            let with = command(&[part.to_string()]);
            for w in words {
                assert!(!full.contains(w), "{w} without {part}");
                assert!(with.contains(w), "{w} with {part}");
            }
            for (other, others) in tabs {
                if other != part {
                    assert!(
                        others.iter().all(|w| !with.contains(w)),
                        "{other} with {part}"
                    );
                }
            }
        }
        assert_eq!(command(&["nonsense".to_string()]), command(&[]));
    }

    #[test]
    fn every_run_has_a_marker_of_its_own() {
        let (a, b) = (fresh_marker(), fresh_marker());
        assert_ne!(a, b);
        assert!(a.starts_with("@@shellder-console-") && a.ends_with(':'));
        assert_eq!(marker(""), "@@shellder-console:");
        let script = reading(&[], &a);
        assert!(script.contains(&format!("printf \"\\n{a}%s\\n\"")));
    }
}
