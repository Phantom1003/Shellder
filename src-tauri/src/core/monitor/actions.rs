//! What the Status tab does on a host: a swap file added or removed, a
//! signal sent, a removable volume mounted or unmounted, a drive ejected.
//! The scripts are the pieces of `actions.sh`; every command comes in two
//! forms, one where nothing may stop for a password and one for a
//! terminal of its own (`ask`), and `wants_password` reads a refusal of
//! the first as a reason to run the second.

use super::super::remotefs::quote;
use super::script::{join, ACTIONS};

/// The script made of the pieces of `actions.sh` tagged `tags`, in the
/// file's order: `sudo` (how a step that needs root runs) before the rest.
fn script(tags: &[&str]) -> String {
    join(ACTIONS, |tag| tags.contains(&tag))
}

const ADDSWAP: &[&str] = &["sudo", "addswap"];
const REMOVESWAP: &[&str] = &["sudo", "removeswap"];
const SIGNAL: &[&str] = &["sudo", "signal"];
const MOUNT: &[&str] = &["sudo", "mount"];
const UNMOUNT: &[&str] = &["sudo", "um", "unmount"];
const EJECT: &[&str] = &["sudo", "um", "eject"];

/// What marks a swap file as one the Console made, and may remove: the
/// readings list those with this in their name under the Swap gauge.
pub const SWAP_MARK: &str = "shellder";

/// The command that adds the swap file `path` of `size` MB (`action`
/// `add`), or switches it off and removes it (`remove`), as the remote
/// login shell gets it. The path is one the dialog or the readings gave,
/// absolute and plain, its name carrying `SWAP_MARK`, so that nothing
/// else on the host is ever written or removed. `ask` as for
/// `disk_command`.
pub fn swap_command(action: &str, path: &str, size: u32, ask: bool) -> Result<String, String> {
    let plain = path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/_.+-".contains(c));
    let name = path.rsplit('/').next().unwrap_or("");
    if !path.starts_with('/') || !plain || path.contains("..") || !name.contains(SWAP_MARK) {
        return Err(format!("not a swap file of the Console: {path}"));
    }
    let ask = if ask { " ask" } else { "" };
    match action {
        "add" => {
            if !(1..=1 << 20).contains(&size) {
                return Err(format!("not a size for a swap file: {size} MB"));
            }
            Ok(format!(
                "sh -c '{}' sh {} {size}{ask}",
                script(ADDSWAP),
                quote(path)
            ))
        }
        "remove" => Ok(format!(
            "sh -c '{}' sh {}{ask}",
            script(REMOVESWAP),
            quote(path)
        )),
        other => Err(format!("cannot {other} a swap file")),
    }
}

/// The signals the process table sends, by name as `kill -s` takes them.
pub const SIGNALS: &[&str] = &["TERM", "KILL", "INT", "HUP", "STOP", "CONT"];

/// The command that sends `signal` (one of `SIGNALS`) to `pid`, as the
/// remote login shell gets it. Pid 0 would be the shell's own process
/// group, so it is refused.
pub fn signal_command(pid: u32, signal: &str, ask: bool) -> Result<String, String> {
    if !SIGNALS.contains(&signal) {
        return Err(format!("not a signal the Console sends: {signal}"));
    }
    if pid == 0 {
        return Err("not a process: 0".into());
    }
    let ask = if ask { " ask" } else { "" };
    Ok(format!("sh -c '{}' sh {pid} {signal}{ask}", script(SIGNAL)))
}

/// The command that mounts or unmounts the volume `device`, or ejects
/// the drive `device` (`action`: `mount`, `unmount`, `eject`), as the
/// remote login shell gets it. The device is a `/dev` path the host
/// itself named; anything else is refused. Without `ask` nothing on the
/// way may stop for a password (`sudo -n`; udisks, which asks polkit,
/// finds no terminal to ask on and refuses); with it, for a terminal
/// tab, `sudo` and polkit's text agent ask on the tab.
pub fn disk_command(action: &str, device: &str, ask: bool) -> Result<String, String> {
    let tags = match action {
        "mount" => MOUNT,
        "unmount" => UNMOUNT,
        "eject" => EJECT,
        other => return Err(format!("cannot {other} a disk")),
    };
    let plain = device
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/_.+-".contains(c));
    if !device.starts_with("/dev/") || !plain || device.contains("..") || device.ends_with('/') {
        return Err(format!("not a device: {device}"));
    }
    let ask = if ask { " ask" } else { "" };
    Ok(format!(
        "sh -c '{}' sh {}{ask}",
        script(tags),
        quote(device)
    ))
}

/// Whether what the host answered means it would do it for a password:
/// udisks saying polkit could be satisfied (`NotAuthorizedCanObtain`),
/// sudo wanting one, or a plain refusal that sudo may get past.
pub fn wants_password(answer: &str) -> bool {
    [
        // udisksctl's two wordings: `GDBus.Error:…NotAuthorizedCanObtain`
        // from a mount, "Not authorized to perform operation
        // (udisks-error-quark, 4)" from a power-off.
        "NotAuthorizedCanObtain",
        "Not authorized",
        // sudo -n without a password: the old wording and the new one
        // (sudo 1.9.14 and later).
        "a password is required",
        "interactive authentication is required",
        "a terminal is required",
        "only root",
        "must be superuser",
        "Operation not permitted",
        "Permission denied",
    ]
    .iter()
    .any(|w| answer.contains(w))
}

/// Every script of this file, for the tests of what a script may carry.
#[cfg(test)]
pub(super) fn every_script() -> Vec<String> {
    [ADDSWAP, REMOVESWAP, SIGNAL, MOUNT, UNMOUNT, EJECT]
        .iter()
        .map(|tags| script(tags))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_script_has_its_pieces_in_order() {
        let pieces = super::super::script::pieces(ACTIONS);
        let tags: Vec<&str> = pieces.iter().map(|(t, _)| *t).collect();
        for list in [ADDSWAP, REMOVESWAP, SIGNAL, MOUNT, UNMOUNT, EJECT] {
            let at: Vec<usize> = list
                .iter()
                .map(|t| tags.iter().position(|x| x == t).expect("a piece"))
                .collect();
            assert!(at.windows(2).all(|w| w[0] < w[1]), "{list:?}");
            assert!(script(list).starts_with("S=\"sudo -n\";"), "{list:?}");
        }
        // The same unmount for both, and as root no sudo at all, so that
        // a host without one still gets to the mount under /mnt.
        assert!(script(UNMOUNT).contains("um(){") && script(EJECT).contains("um(){"));
        for list in [ADDSWAP, SIGNAL, MOUNT, UNMOUNT, EJECT] {
            assert!(script(list).contains("[ \"$(id -u)\" = 0 ] && S=\"\";"));
        }
    }

    #[test]
    fn the_commands_take_only_what_they_should() {
        assert_eq!(
            swap_command("add", "/shellder.swap", 4096, false).unwrap(),
            format!("sh -c '{}' sh '/shellder.swap' 4096", script(ADDSWAP))
        );
        assert_eq!(
            swap_command("remove", "/shellder.swap", 0, true).unwrap(),
            format!("sh -c '{}' sh '/shellder.swap' ask", script(REMOVESWAP))
        );
        for bad in [
            "/swapfile",
            "shellder.swap",
            "/tmp/../shellder.swap",
            "/tmp/shellder swap",
            "/tmp/shellder;rm",
        ] {
            assert!(swap_command("add", bad, 1024, false).is_err(), "{bad}");
        }
        assert!(swap_command("add", "/shellder.swap", 0, false).is_err());
        assert!(swap_command("grow", "/shellder.swap", 1, false).is_err());
        assert_eq!(
            signal_command(4321, "TERM", false).unwrap(),
            format!("sh -c '{}' sh 4321 TERM", script(SIGNAL))
        );
        assert_eq!(
            signal_command(4321, "KILL", true).unwrap(),
            format!("sh -c '{}' sh 4321 KILL ask", script(SIGNAL))
        );
        assert!(signal_command(0, "TERM", false).is_err());
        assert!(signal_command(1, "9", false).is_err());
        assert!(signal_command(1, "SEGV", false).is_err());
        assert_eq!(
            disk_command("mount", "/dev/sdb1", false).unwrap(),
            format!("sh -c '{}' sh '/dev/sdb1'", script(MOUNT))
        );
        assert_eq!(
            disk_command("mount", "/dev/sdb1", true).unwrap(),
            format!("sh -c '{}' sh '/dev/sdb1' ask", script(MOUNT))
        );
        assert!(disk_command("eject", "/dev/disk4", false)
            .unwrap()
            .contains("diskutil eject"));
        // Only a device the host named: nothing that could be a word of its own.
        for bad in [
            "sdb1",
            "/dev/../etc/passwd",
            "/dev/sdb1; rm -rf /",
            "/dev/",
            "/dev/sd b",
        ] {
            assert!(disk_command("unmount", bad, false).is_err(), "{bad}");
        }
        assert!(disk_command("format", "/dev/sdb1", false).is_err());
    }

    /// The sudo the scripts pick: `sudo -n` without `ask`, `sudo` with
    /// it, and none as root. Run here with `sudo` and `id` as functions
    /// of the test's own, so nothing is asked or done.
    #[test]
    fn sudo_asks_only_when_told_to() {
        let pick = |uid: &str, args: &str| {
            let sudo = script(&["sudo"]);
            let probe = format!("id(){{ echo {uid}; }}; {sudo} echo \"[$S]\"");
            let out = std::process::Command::new("/bin/sh")
                .args(["-c", &probe, "sh"])
                .args(args.split_whitespace())
                .output()
                .expect("sh");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(pick("1000", "/dev/sdb1"), "[sudo -n]");
        assert_eq!(pick("1000", "/dev/sdb1 ask"), "[sudo]");
        assert_eq!(pick("1000", "4321 TERM ask"), "[sudo]");
        assert_eq!(pick("0", "/dev/sdb1"), "[]");
        assert_eq!(pick("0", "/dev/sdb1 ask"), "[]");
    }

    #[test]
    fn what_a_password_would_get_past() {
        assert!(wants_password("Error mounting /dev/sda1: GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain: Not authorized to perform operation"));
        assert!(wants_password(
            "Error powering off drive: Not authorized to perform operation (udisks-error-quark, 4)"
        ));
        assert!(wants_password("sudo: a password is required"));
        assert!(wants_password(
            "sudo: interactive authentication is required"
        ));
        assert!(!wants_password("umount: /media/me/STICK: target is busy."));
    }
}
