//! The Disks card: what `df` has mounted, and which of it, and of what is
//! not mounted, is on a removable drive (`lsblk` on Linux, `diskutil list
//! external` on macOS).

use std::collections::{HashMap, HashSet};

use super::fields;
use super::types::{Disk, Volume};

/// The mounted file systems worth a row, and the volumes on removable
/// drives that are not mounted, out of the `df` and `blk` sections.
pub(super) fn read(df: &[&str], blk: &[&str]) -> (Vec<Disk>, Vec<Volume>) {
    let devices = devices(blk);
    let up = mounted(df);
    let volumes = devices
        .volumes
        .iter()
        .filter(|v| !up.contains(&v.device))
        .cloned()
        .collect();
    (disks(df, &devices), volumes)
}

/// A mount worth a row: real storage, not the kernel's own filesystems
/// and not macOS's sealed system volumes beside the data volume.
fn storage(fs: &str, mount: &str, total: u64) -> bool {
    const PSEUDO: &[&str] = &[
        "tmpfs",
        "devtmpfs",
        "udev",
        "none",
        "overlay",
        "squashfs",
        "efivarfs",
        "devfs",
        "cgroup",
        "shm",
        "map",
        "autofs",
        "fusectl",
        "debugfs",
        "tracefs",
        "securityfs",
        "ramfs",
        "rootfs",
        "proc",
        "sysfs",
        "mqueue",
        "hugetlbfs",
        "pstore",
        "bpf",
    ];
    if total == 0 || PSEUDO.contains(&fs) || fs.starts_with("map ") {
        return false;
    }
    // An AppImage mounted by the program running from it.
    if mount.starts_with("/tmp/.mount_") {
        return false;
    }
    // Where udisks mounts removable drives on some distributions.
    if mount.starts_with("/run/media/") {
        return true;
    }
    const HIDDEN: &[&str] = &[
        "/dev",
        "/sys",
        "/proc",
        "/run",
        "/snap/",
        "/private/var/vm",
        "/Volumes/Recovery",
        "/System/Volumes/Preboot",
        "/System/Volumes/VM",
        "/System/Volumes/Update",
        "/System/Volumes/xarts",
        "/System/Volumes/iSCPreboot",
        "/System/Volumes/Hardware",
    ];
    !HIDDEN.iter().any(|h| {
        let h = h.trim_end_matches('/');
        mount == h || mount.starts_with(&format!("{h}/"))
    })
}

fn disks(lines: &[&str], devices: &Devices) -> Vec<Disk> {
    lines
        .iter()
        .skip(1) // the header
        .filter_map(|l| {
            let f = fields(l);
            if f.len() < 6 {
                return None;
            }
            let total: u64 = f[1].parse().ok()?;
            let used: u64 = f[2].parse().ok()?;
            let mount = f[5..].join(" ");
            let drive = devices.removable.get(f[0]);
            storage(f[0], &mount, total).then(|| Disk {
                filesystem: f[0].to_string(),
                mount,
                total: total * 1024,
                used: used * 1024,
                removable: drive.is_some(),
                drive: drive.cloned().flatten(),
            })
        })
        .collect()
}

/// What `df` has mounted, hidden mounts included.
fn mounted(lines: &[&str]) -> HashSet<String> {
    lines
        .iter()
        .skip(1)
        .filter_map(|l| fields(l).first().map(|f| f.to_string()))
        .collect()
}

/// What the host says about its block devices.
#[derive(Debug, Default, PartialEq)]
struct Devices {
    /// Every device on a removable drive, to the drive it is on.
    removable: HashMap<String, Option<String>>,
    /// The volumes among them that may be mounted (`df` says which
    /// already are).
    volumes: Vec<Volume>,
}

/// Linux `lsblk -P`, or macOS `diskutil list external` after a line
/// saying so.
fn devices(lines: &[&str]) -> Devices {
    match lines.first() {
        None => Devices::default(),
        Some(&"diskutil") => diskutil_devices(&lines[1..]),
        Some(_) => lsblk_devices(lines),
    }
}

/// The `KEY="value"` pairs of an `lsblk -P` line; a byte lsblk could not
/// print is `\xHH`.
fn pairs(line: &str) -> Vec<(&str, String)> {
    let mut out = Vec::new();
    let mut rest = line.trim();
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].trim();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        let raw = &after[..end];
        let bytes = raw.as_bytes();
        let mut value: Vec<u8> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"\\x") {
                let hex = raw
                    .get(i + 2..i + 4)
                    .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    value.push(b);
                    i += 4;
                    continue;
                }
            }
            value.push(bytes[i]);
            i += 1;
        }
        out.push((key, String::from_utf8_lossy(&value).into_owned()));
        rest = after[end + 1..].trim_start();
    }
    out
}

/// The drive a partition is on when lsblk does not say (an old one
/// without PKNAME): `sdb1` → `sdb`, `mmcblk0p1` → `mmcblk0`.
fn linux_drive(part: &str) -> Option<String> {
    let base = part.trim_end_matches(|c: char| c.is_ascii_digit());
    if base.len() == part.len() || base.is_empty() {
        return None;
    }
    let base = match base.strip_suffix('p') {
        Some(b) if b.ends_with(|c: char| c.is_ascii_digit()) => b,
        _ => base,
    };
    Some(base.to_string())
}

/// What `mount` cannot take: swap and the members of something larger.
fn mountable(fstype: &str) -> bool {
    !fstype.is_empty()
        && !matches!(
            fstype,
            "swap" | "LVM2_member" | "crypto_LUKS" | "linux_raid_member" | "zfs_member" | "bcache"
        )
}

fn lsblk_devices(lines: &[&str]) -> Devices {
    struct Rec {
        name: String,
        kind: String,
        fstype: String,
        size: u64,
        label: String,
        mount: String,
        removable: bool,
        drive: Option<String>,
    }
    let recs: Vec<Rec> = lines
        .iter()
        .filter_map(|l| {
            let p = pairs(l);
            let get = |k: &str| {
                p.iter()
                    .find(|(key, _)| *key == k)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("")
            };
            let dev = |n: &str| {
                if n.starts_with('/') {
                    n.to_string()
                } else {
                    format!("/dev/{n}")
                }
            };
            let name = get("NAME");
            if name.is_empty() {
                return None;
            }
            let name = dev(name);
            let kind = get("TYPE").to_string();
            let drive = match get("PKNAME") {
                "" if kind == "disk" || kind == "rom" => Some(name.clone()),
                "" if kind == "part" => linux_drive(&name),
                "" => None,
                p => Some(dev(p)),
            };
            Some(Rec {
                // Not HOTPLUG: an internal SATA disk on a hot-plug capable
                // port says so too, and must not get an eject button.
                removable: get("RM") == "1" || get("TRAN") == "usb" || kind == "rom",
                kind,
                fstype: get("FSTYPE").to_string(),
                size: get("SIZE").parse().unwrap_or(0),
                label: get("LABEL").to_string(),
                mount: get("MOUNTPOINT").to_string(),
                drive,
                name,
            })
        })
        .collect();
    // What a device sits on, up to the drive: a partition's disk, and
    // for a LUKS or LVM volume (`crypt`, `lvm`) the partition under it
    // and then the disk. Removable when anything on the way is.
    fn chain<'a>(by_name: &HashMap<&str, &'a Rec>, r: &'a Rec) -> Vec<&'a Rec> {
        let mut out = vec![r];
        while let Some(p) = out.last().unwrap().drive.as_deref() {
            match by_name.get(p) {
                Some(d) if !out.iter().any(|o| o.name == d.name) => out.push(d),
                _ => break,
            }
        }
        out
    }
    let mut by_name: HashMap<&str, &Rec> = HashMap::new();
    for r in &recs {
        by_name.entry(r.name.as_str()).or_insert(r);
    }
    let mut out = Devices::default();
    for r in &recs {
        let up = chain(&by_name, r);
        if !up.iter().any(|d| d.removable) {
            continue;
        }
        let top = up.last().unwrap();
        let drive = if top.name == r.name {
            r.drive.clone()
        } else {
            top.drive.clone().or_else(|| Some(top.name.clone()))
        };
        out.removable.insert(r.name.clone(), drive.clone());
        if r.kind != "loop" && mountable(&r.fstype) && r.mount.is_empty() {
            out.volumes.push(Volume {
                device: r.name.clone(),
                drive,
                label: r.label.clone(),
                fstype: r.fstype.clone(),
                size: r.size,
            });
        }
    }
    out
}

/// "31.9 GB" as diskutil prints it (decimal units), in bytes.
fn diskutil_size(number: &str, unit: &str) -> u64 {
    let v: f64 = number.trim_start_matches(['*', '+']).parse().unwrap_or(0.0);
    let mult: f64 = match unit {
        "B" => 1.0,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        _ => 0.0,
    };
    (v * mult) as u64
}

/// `/dev/disk4s2` → `/dev/disk4`.
fn macos_drive(dev: &str) -> String {
    match dev.rfind('s') {
        Some(i)
            if i > 0
                && dev[i + 1..].chars().all(|c| c.is_ascii_digit())
                && !dev[i + 1..].is_empty() =>
        {
            dev[..i].to_string()
        }
        _ => dev.to_string(),
    }
}

/// `diskutil list external`: a block per disk, physical or synthesized
/// (an APFS container, whose `Physical Store` line names the disk it is
/// on), with a header giving the columns and a row per slice. Every slice
/// is removable; the rows that hold a file system (not a partition
/// scheme, EFI, a container, APFS's own volumes) are volumes.
fn diskutil_devices(lines: &[&str]) -> Devices {
    let mut out = Devices::default();
    let mut disk = String::new();
    let mut store: Option<String> = None;
    let mut cols: Option<(usize, usize)> = None; // NAME, SIZE, in chars
    let mut rows: Vec<(String, String, String, u64)> = Vec::new(); // type, name, id, size
    let flush = |disk: &str,
                 store: &Option<String>,
                 rows: &mut Vec<(String, String, String, u64)>,
                 out: &mut Devices| {
        let drive = store
            .as_deref()
            .map(macos_drive)
            .unwrap_or_else(|| disk.to_string());
        for (kind, name, id, size) in rows.drain(..) {
            let device = format!("/dev/{id}");
            out.removable.insert(device.clone(), Some(drive.clone()));
            let scheme = kind.contains("Scheme") || kind.contains("scheme");
            let apfs_own = kind == "APFS Volume"
                && matches!(
                    name.as_str(),
                    "Preboot" | "Recovery" | "VM" | "Update" | "xART" | "Hardware" | "iSCPreboot"
                );
            let plumbing = matches!(
                kind.as_str(),
                "EFI"
                    | "Apple_Boot"
                    | "Apple_APFS"
                    | "Apple_APFS_Recovery"
                    | "Apple_APFS_ISC"
                    | "Apple_CoreStorage"
                    | "APFS Snapshot"
                    | "Apple_KernelCoreDump"
                    | "Linux Swap"
                    | "Microsoft Reserved"
                    | "Windows Recovery"
            );
            if scheme || apfs_own || plumbing || kind.is_empty() {
                continue;
            }
            out.volumes.push(Volume {
                device,
                drive: Some(drive.clone()),
                label: if name == "-" { String::new() } else { name },
                fstype: kind,
                size,
            });
        }
    };
    for l in lines {
        let t = l.trim();
        if let Some(rest) = l.strip_prefix("/dev/") {
            flush(&disk, &store, &mut rows, &mut out);
            disk = format!("/dev/{}", rest.split_whitespace().next().unwrap_or(""));
            store = None;
            cols = None;
            continue;
        }
        if t.starts_with("#:") {
            let chars: Vec<char> = l.chars().collect();
            let at = |word: &str| -> Option<usize> {
                let w: Vec<char> = word.chars().collect();
                chars.windows(w.len()).position(|win| win == w.as_slice())
            };
            cols = at("NAME").zip(at("SIZE"));
            continue;
        }
        if let Some(rest) = t.strip_prefix("Physical Store ") {
            store = rest.split_whitespace().next().map(|d| format!("/dev/{d}"));
            continue;
        }
        let Some((name_col, size_col)) = cols else {
            continue;
        };
        let f = fields(t);
        if f.len() < 4 || !t.starts_with(|c: char| c.is_ascii_digit()) || !f[0].ends_with(':') {
            continue;
        }
        let id = f[f.len() - 1].to_string();
        let size = diskutil_size(f[f.len() - 3], f[f.len() - 2]);
        let chars: Vec<char> = l.chars().collect();
        // Columns of a row that does not line up with the header may
        // come out in the wrong order: empty, then.
        let slice = |a: usize, b: usize| -> String {
            let (a, b) = (a.min(chars.len()), b.min(chars.len()));
            if a >= b {
                return String::new();
            }
            chars[a..b].iter().collect::<String>().trim().to_string()
        };
        let colon = l.find(':').map(|i| l[..i].chars().count() + 1).unwrap_or(0);
        let kind = slice(colon, name_col);
        let name = slice(name_col, size_col);
        rows.push((kind, name, id, size));
    }
    flush(&disk, &store, &mut rows, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_storage() {
        assert!(storage("/dev/sda1", "/home", 10));
        assert!(!storage("tmpfs", "/tmp", 10));
        assert!(!storage("/dev/sda1", "/", 0));
        assert!(!storage("/dev/loop1", "/snap/core/123", 10));
        assert!(!storage("/dev/disk3s6", "/System/Volumes/VM", 10));
        assert!(storage("/dev/disk3s5", "/System/Volumes/Data", 10));
        assert!(!storage(
            "Shellder.AppImage",
            "/tmp/.mount_ShelldKaeaHd",
            10
        ));
        assert!(storage("/dev/sdb1", "/run/media/alice/STICK", 10));
        assert!(!storage("tmpfs", "/run/user/1000", 10));
    }

    #[test]
    fn block_devices_without_pkname() {
        // An old lsblk (no PKNAME, HOTPLUG or TRAN): the drive of a
        // partition from its name, removable from RM alone.
        let d = lsblk_devices(&[
            r#"NAME="/dev/sdb" TYPE="disk" FSTYPE="" SIZE="100" LABEL="" MOUNTPOINT="" RM="1""#,
            r#"NAME="/dev/sdb1" TYPE="part" FSTYPE="ext4" SIZE="90" LABEL="" MOUNTPOINT="" RM="1""#,
            r#"NAME="/dev/mmcblk0p1" TYPE="part" FSTYPE="vfat" SIZE="90" LABEL="CARD" MOUNTPOINT="" RM="1""#,
            r#"NAME="/dev/sda1" TYPE="part" FSTYPE="ext4" SIZE="90" LABEL="" MOUNTPOINT="/" RM="0""#,
        ]);
        let devs: Vec<(&str, Option<&str>)> = d
            .volumes
            .iter()
            .map(|v| (v.device.as_str(), v.drive.as_deref()))
            .collect();
        assert_eq!(
            devs,
            vec![
                ("/dev/sdb1", Some("/dev/sdb")),
                ("/dev/mmcblk0p1", Some("/dev/mmcblk0"))
            ]
        );
        assert!(!d.removable.contains_key("/dev/sda1"));
        assert_eq!(
            linux_drive("/dev/sr0"),
            Some("/dev/sr".into()),
            "only asked for partitions"
        );
        assert_eq!(linux_drive("/dev/sda"), None);
        assert_eq!(macos_drive("/dev/disk4s2"), "/dev/disk4");
        assert_eq!(macos_drive("/dev/disk4"), "/dev/disk4");
    }

    #[test]
    fn what_lsblk_escapes() {
        assert_eq!(
            pairs(r#"NAME="/dev/sdb1" LABEL="a\x22b\x20c""#),
            vec![
                ("NAME", "/dev/sdb1".to_string()),
                ("LABEL", "a\"b c".to_string())
            ]
        );
        // A name in UTF-8 a byte at a time, and escapes that are not
        // whole: kept as they are, without cutting a character in two.
        assert_eq!(
            pairs(r#"LABEL="caf\xc3\xa9" A="\x€x" B="\x+1" C="\x4""#),
            vec![
                ("LABEL", "café".to_string()),
                ("A", "\\x€x".to_string()),
                ("B", "\\x+1".to_string()),
                ("C", "\\x4".to_string())
            ]
        );
    }

    #[test]
    fn a_diskutil_row_out_of_line_with_its_header() {
        let d = diskutil_devices(&[
            "/dev/disk4 (external, physical):",
            "   #:                       TYPE NAME                    SIZE       IDENTIFIER",
            "   0:     FDisk_partition_scheme                        *32.0 GB    disk4",
            "                                                    1: DOS_FAT_32 STICK 32.0 GB disk4s1",
        ]);
        assert!(d.removable.contains_key("/dev/disk4s1"));
        assert!(d.volumes.is_empty(), "no type to go by: {:?}", d.volumes);
    }
}
