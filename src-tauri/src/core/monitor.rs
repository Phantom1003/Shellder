//! What a Console window shows about a host: one shell script, run
//! through the master when one is up and through the host's session
//! otherwise, whose output is cut into sections and read here. Every
//! section is best effort: a host without `/proc` (macOS, the BSDs), without
//! tmux, screen or `nvidia-smi` leaves its part empty, and the window shows
//! what there is.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::remotefs::{quote, remote};

/// Where a section starts in the script's output.
const MARK: &str = "@@shellder-console:";

/// The script, one line of POSIX sh so that it means the same under any
/// login shell: it is wrapped in `sh -c '…'`, which is why it carries no
/// single quote. `S name` starts a section; what a command cannot say is
/// left empty. Linux reads `/proc`, macOS its `sysctl`s, `vm_stat` and
/// `top` (which costs a second, the sample it takes); `df -P` and `ps -eo`
/// are what everyone has (BusyBox's `ps` lacks the cpu column, so that
/// section is empty there). `lsblk` (macOS: `diskutil list external`) says
/// which disks are removable and which of their volumes are not mounted.
const SCRIPT: &str = concat!(
    "S(){ printf \"\\n@@shellder-console:%s\\n\" \"$1\"; }; ",
    "S hostname; hostname 2>/dev/null; ",
    "S uname; uname -sr 2>/dev/null; ",
    "S os; if [ -r /etc/os-release ]; then . /etc/os-release 2>/dev/null; printf \"%s\\n\" \"$PRETTY_NAME\"; ",
    "elif command -v sw_vers >/dev/null 2>&1; then printf \"macOS %s\\n\" \"$(sw_vers -productVersion 2>/dev/null)\"; fi; ",
    "S now; date +%s 2>/dev/null; ",
    "S uptime; cat /proc/uptime 2>/dev/null || sysctl -n kern.boottime 2>/dev/null; ",
    "S load; cat /proc/loadavg 2>/dev/null || sysctl -n vm.loadavg 2>/dev/null; ",
    "S ncpu; nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null; ",
    "S stat; head -n 1 /proc/stat 2>/dev/null; ",
    "S topcpu; [ -r /proc/stat ] || top -l 2 -n 0 -s 1 2>/dev/null | grep \"CPU usage\" | tail -n 1; ",
    "S mem; cat /proc/meminfo 2>/dev/null || { sysctl -n hw.memsize hw.pagesize 2>/dev/null; vm_stat 2>/dev/null; }; ",
    "S swap; [ -r /proc/meminfo ] || sysctl -n vm.swapusage 2>/dev/null; ",
    "S df; df -P -k 2>/dev/null; ",
    "S blk; if command -v lsblk >/dev/null 2>&1; then lsblk -b -p -P -o NAME,TYPE,FSTYPE,SIZE,LABEL,MOUNTPOINT,RM,HOTPLUG,TRAN,PKNAME 2>/dev/null || lsblk -b -p -P -o NAME,TYPE,FSTYPE,SIZE,LABEL,MOUNTPOINT,RM 2>/dev/null; elif command -v diskutil >/dev/null 2>&1; then echo diskutil; diskutil list external 2>/dev/null; fi; ",
    "S gpu; command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null; ",
    "S tmux; command -v tmux >/dev/null 2>&1 && { echo present; tmux list-sessions -F \"#{session_name}\t#{session_windows}\t#{session_created}\t#{session_attached}\t#{session_activity}\" 2>/dev/null; }; ",
    "S windows; tmux list-windows -a -F \"#{session_name}\t#{window_index}\t#{window_name}\t#{window_active}\t#{pane_current_command}\t#{window_panes}\t#{pane_current_path}\" 2>/dev/null; ",
    "S screen; command -v screen >/dev/null 2>&1 && { echo present; screen -ls 2>/dev/null; }; ",
    "S screencwd; for p in $(screen -ls 2>/dev/null | sed -n \"s/^[ \t]*\\([0-9][0-9]*\\)\\..*/\\1/p\"); do c=$(pgrep -P \"$p\" 2>/dev/null | head -n 1); [ -n \"$c\" ] && printf \"%s\t%s\\n\" \"$p\" \"$(readlink /proc/$c/cwd 2>/dev/null)\"; done; ",
    "S docker; D=docker; command -v docker >/dev/null 2>&1 || D=podman; command -v $D >/dev/null 2>&1 && { echo present $D; $D ps --format \"{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}\" 2>/dev/null; ids=$($D ps -q 2>/dev/null); [ -n \"$ids\" ] && $D inspect --format \"M\t{{.Name}}{{range .Mounts}}{{if eq .Type \\\"bind\\\"}}\t{{.Source}}{{end}}{{end}}\" $ids 2>/dev/null; }; ",
    "S slurm; command -v squeue >/dev/null 2>&1 && { echo present; squeue -h -u \"$(id -un)\" -o \"%i|%j|%T|%M|%D|%R|%P|%Z\" 2>/dev/null; }; ",
    "S pbs; command -v qstat >/dev/null 2>&1 && command -v qselect >/dev/null 2>&1 && { echo present; ids=$(qselect -u \"$(id -un)\" 2>/dev/null); [ -n \"$ids\" ] && qstat -f $ids 2>/dev/null; }; ",
    "S vnc; ps -eo user,pid,args 2>/dev/null | grep -E \"(^| |/)(Xvnc|Xvnc4|Xtigervnc|Xtightvnc|Xrealvnc|x11vnc|vncserver-x11|vncserver-x11-core) \"; ",
    "launchctl print system/com.apple.screensharing >/dev/null 2>&1 && echo screensharing; ",
    "S ps; ps -eo pid,user,pcpu,pmem,comm 2>/dev/null | tail -n +2 | sort -k3 -rn 2>/dev/null | head -n 15; ",
    "S who; who 2>/dev/null; ",
    "S ports; if command -v ss >/dev/null 2>&1; then ss -ltnpH 2>/dev/null; elif command -v lsof >/dev/null 2>&1; then echo lsof; lsof -nP -iTCP -sTCP:LISTEN 2>/dev/null | tail -n +2; fi; ",
    "true",
);

/// The script as the remote login shell gets it.
pub fn command() -> String {
    format!("sh -c '{SCRIPT}'")
}

/// Mount the volume `$1`: `diskutil` on macOS; on Linux udisks (which
/// picks the mount point, `/media/$USER/LABEL` or `/run/media/…`), an
/// fstab entry, and last `sudo -n mount` under `/mnt`.
const MOUNT: &str = concat!(
    "S=\"sudo -n\"; [ \"$2\" = ask ] && S=sudo; d=\"$1\"; if command -v diskutil >/dev/null 2>&1; then exec diskutil mount \"$d\"; fi; e=\"\"; ",
    "if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl mount -b \"$d\" 2>&1) && { printf \"%s\\n\" \"$o\"; exit 0; }; e=\"$o\"; fi; ",
    "o=$(mount \"$d\" 2>&1) && exit 0; [ -n \"$e\" ] || e=\"$o\"; ",
    "if $S true 2>/dev/null; then m=/mnt/$(basename \"$d\"); o=$($S mkdir -p \"$m\" 2>&1 && $S mount \"$d\" \"$m\" 2>&1) && { echo \"Mounted $d at $m\"; exit 0; }; fi; ",
    "printf \"%s\\n\" \"$e\" >&2; exit 1",
);

/// Unmount the volume `$1`.
const UNMOUNT: &str = concat!(
    "S=\"sudo -n\"; [ \"$2\" = ask ] && S=sudo; d=\"$1\"; if command -v diskutil >/dev/null 2>&1; then exec diskutil unmount \"$d\"; fi; ",
    "um(){ e=\"\"; if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl unmount -b \"$1\" 2>&1) && { printf \"%s\\n\" \"$o\"; return 0; }; e=\"$o\"; fi; ",
    "o=$(umount \"$1\" 2>&1) && return 0; [ -n \"$e\" ] || e=\"$o\"; $S umount \"$1\" 2>/dev/null && return 0; printf \"%s\\n\" \"$e\" >&2; return 1; }; ",
    "um \"$d\"",
);

/// Eject the drive `$1`: every volume on it unmounted (`lsblk -l`: a
/// list, not the tree whose `├─` would be taken for part of the name),
/// then the drive powered off (`udisksctl power-off`, or `eject`, which also opens an
/// optical drive), so that it can be pulled. macOS: `diskutil eject`.
const EJECT: &str = concat!(
    "S=\"sudo -n\"; [ \"$2\" = ask ] && S=sudo; d=\"$1\"; if command -v diskutil >/dev/null 2>&1; then exec diskutil eject \"$d\"; fi; ",
    "um(){ e=\"\"; if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl unmount -b \"$1\" 2>&1) && { printf \"%s\\n\" \"$o\"; return 0; }; e=\"$o\"; fi; ",
    "o=$(umount \"$1\" 2>&1) && return 0; [ -n \"$e\" ] || e=\"$o\"; $S umount \"$1\" 2>/dev/null && return 0; printf \"%s\\n\" \"$e\" >&2; return 1; }; ",
    "lsblk -n -l -p -o NAME,MOUNTPOINT \"$d\" 2>/dev/null | while read -r n m; do [ -n \"$m\" ] || continue; um \"$n\" || exit 1; done || exit 1; e=\"\"; ",
    "case \"$d\" in /dev/sr*|/dev/cdrom*) o=$(eject \"$d\" 2>&1) && exit 0;; esac; ",
    "if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl power-off -b \"$d\" 2>&1) && exit 0; e=\"$o\"; fi; ",
    "o=$(eject \"$d\" 2>&1) && exit 0; [ -n \"$e\" ] || e=\"$o\"; $S eject \"$d\" 2>/dev/null && exit 0; ",
    "printf \"%s\\n\" \"$e\" >&2; exit 1",
);

/// The command that mounts or unmounts the volume `device`, or ejects
/// the drive `device` (`action`: `mount`, `unmount`, `eject`), as the
/// remote login shell gets it. The device is a `/dev` path the host
/// itself named; anything else is refused. Without `ask` nothing on the
/// way may stop for a password (`sudo -n`; udisks, which asks polkit,
/// finds no terminal to ask on and refuses); with it, for a terminal
/// tab, `sudo` and polkit's text agent ask on the tab.
pub fn disk_command(action: &str, device: &str, ask: bool) -> Result<String, String> {
    let script = match action {
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
    Ok(format!("sh -c '{script}' sh {}{ask}", quote(device)))
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
        "a password is required",
        "a terminal is required",
        "only root",
        "must be superuser",
        "Operation not permitted",
        "Permission denied",
    ]
    .iter()
    .any(|w| answer.contains(w))
}

/// Ticks of `/proc/stat`'s first line: the window takes the difference
/// between two snapshots for a percentage, since a single reading only
/// says what happened since boot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CpuTicks {
    pub busy: u64,
    pub total: u64,
}

/// Something that fills up: bytes used of bytes there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Gauge {
    pub total: u64,
    pub used: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub filesystem: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    /// On a removable drive (USB, a card, a disc): it can be unmounted
    /// and the drive ejected from the Console.
    pub removable: bool,
    /// The drive it is on (`/dev/sdb`, `/dev/disk4`), what an eject takes.
    pub drive: Option<String>,
}

/// A volume on a removable drive that is not mounted: a row with a
/// mount button.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub device: String,
    pub drive: Option<String>,
    pub label: String,
    /// What is on it as the host names it (`vfat`, `APFS Volume`).
    pub fstype: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub index: u32,
    pub name: String,
    /// Percent busy, as nvidia-smi reports it.
    pub utilization: Option<f64>,
    pub memory: Option<Gauge>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxWindow {
    pub index: u32,
    pub name: String,
    pub active: bool,
    /// What runs in the active pane, and where.
    pub command: String,
    pub path: String,
    pub panes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxSession {
    pub name: String,
    /// Unix time.
    pub created: Option<f64>,
    pub activity: Option<f64>,
    /// How many clients are attached to it (one of them may be this
    /// window's).
    pub attached: u32,
    pub windows: Vec<TmuxWindow>,
    /// The active window's current directory.
    pub path: String,
}

/// A GNU screen session, as `screen -ls` lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSession {
    /// `pid.name`: what `screen -x` is given, unambiguous.
    pub id: String,
    pub name: String,
    pub pid: u32,
    pub attached: bool,
    /// The current directory of the shell in it (Linux, through /proc).
    pub path: String,
}

/// A VNC server running on the host: an Xvnc, Xtigervnc, x11vnc… process,
/// or macOS's Screen Sharing (Remote Management serves the same), which
/// launchd listens for and starts when a viewer connects.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VncSession {
    /// Empty for Screen Sharing: it is the system's, not a user's.
    pub user: String,
    /// None for Screen Sharing: no process until a viewer connects.
    pub pid: Option<u32>,
    /// The program, without its path; "Screen Sharing" on macOS.
    pub program: String,
    /// The X display it serves (`:1`), when it says.
    pub display: Option<u32>,
    /// Where a viewer connects: `-rfbport`, or 5900 plus the display.
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    /// The host side of its bind mounts: where its files are on the host.
    pub mounts: Vec<String>,
}

/// The containers, and which tool lists them (`docker` or `podman`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Containers {
    pub tool: String,
    pub list: Vec<Container>,
}

/// A Slurm job of the user, from `squeue`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlurmJob {
    pub id: String,
    pub name: String,
    /// RUNNING, PENDING, …
    pub state: String,
    /// Time used so far, as squeue writes it.
    pub time: String,
    pub nodes: u32,
    /// The node list of a running job, the reason of a waiting one.
    pub reason: String,
    pub partition: String,
    /// The job's working directory (`%Z`).
    pub path: String,
}

/// A PBS (OpenPBS, PBS Pro, Torque) job of the user, from `qstat -f`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PbsJob {
    pub id: String,
    pub name: String,
    /// R, Q, H, E, …
    pub state: String,
    /// resources_used.walltime, empty before it runs.
    pub time: String,
    pub nodes: u32,
    /// The first node of exec_host, where a shell for the job goes.
    pub node: String,
    pub queue: String,
    /// Where it was submitted from: `init_work_dir` (Torque), or
    /// `PBS_O_WORKDIR` in its `Variable_List`.
    pub path: String,
}

/// A TCP port something on the host listens on: what a port forward
/// reaches.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub port: u16,
    /// What it is bound to: `0.0.0.0`, `127.0.0.1`, `*`, `::`…
    pub address: String,
    /// The program, when the host says (ss only names the user's own).
    pub process: String,
    pub pid: Option<u32>,
}

/// A line of `who`: someone logged in.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub user: String,
    pub tty: String,
    /// When, as `who` wrote it.
    pub since: String,
    /// Where from, when `who` says.
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: u32,
    pub user: String,
    pub cpu: f64,
    pub memory: f64,
    pub command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub hostname: String,
    /// "Ubuntu 24.04.1 LTS", "macOS 15.1", or what `uname` says.
    pub os: String,
    pub kernel: String,
    /// Seconds since boot.
    pub uptime: Option<f64>,
    pub load: Option<[f64; 3]>,
    pub cpus: Option<u32>,
    pub cpu_ticks: Option<CpuTicks>,
    /// A percentage the host gave outright (macOS's `top`), when there are
    /// no ticks to difference.
    pub cpu_percent: Option<f64>,
    pub memory: Option<Gauge>,
    pub swap: Option<Gauge>,
    pub disks: Vec<Disk>,
    /// Volumes on removable drives that are not mounted.
    pub volumes: Vec<Volume>,
    pub gpus: Vec<Gpu>,
    /// None: no tmux on the host. Some(empty): tmux, but no session.
    pub tmux: Option<Vec<TmuxSession>>,
    /// The same for GNU screen.
    pub screen: Option<Vec<ScreenSession>>,
    /// None: neither docker nor podman on the host.
    pub docker: Option<Containers>,
    /// None: no Slurm client on the host.
    pub slurm: Option<Vec<SlurmJob>>,
    /// None: no PBS client on the host.
    pub pbs: Option<Vec<PbsJob>>,
    pub vnc: Vec<VncSession>,
    pub processes: Vec<Process>,
    pub logins: Vec<Login>,
    pub users: Option<u32>,
    pub listeners: Vec<Listener>,
}

/// Run the script on `host` and read what it said.
pub fn fetch(host: &str) -> Result<Snapshot, String> {
    let out = remote::shell(host, &command());
    if !out.stdout.contains(MARK) {
        let why = out.text();
        return Err(if why.is_empty() {
            format!("ssh exited with status {}", out.status)
        } else {
            why
        });
    }
    Ok(parse(&out.stdout))
}

/// The output cut at its markers: section name to the lines under it.
fn sections(text: &str) -> Vec<(&str, Vec<&str>)> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix(MARK) {
            out.push((name.trim(), Vec::new()));
        } else if let Some(last) = out.last_mut() {
            if !line.trim().is_empty() {
                last.1.push(line);
            }
        }
    }
    out
}

fn num<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim().trim_end_matches('.').parse().ok()
}

fn fields(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

/// The first `n` numbers in a line, whatever is around them (`{ 1.2 3.4 }`).
fn floats(line: &str, n: usize) -> Option<Vec<f64>> {
    let v: Vec<f64> = line
        .split(|c: char| c.is_whitespace() || c == '{' || c == '}' || c == ',')
        .filter_map(|w| w.parse().ok())
        .take(n)
        .collect();
    (v.len() == n).then_some(v)
}

/// "2048.00M" (vm.swapusage) in bytes.
fn sized(word: &str) -> Option<u64> {
    let w = word.trim();
    let (digits, unit) = w.split_at(w.trim_end_matches(|c: char| c.is_ascii_alphabetic()).len());
    let v: f64 = digits.parse().ok()?;
    let mult = match unit {
        "K" | "k" => 1u64 << 10,
        "M" | "m" => 1 << 20,
        "G" | "g" => 1 << 30,
        "T" | "t" => 1 << 40,
        "" => 1,
        _ => return None,
    };
    Some((v * mult as f64) as u64)
}

/// A `key: value` from `/proc/meminfo` (in kB) or `vm_stat` (in pages).
fn keyed(lines: &[&str], key: &str) -> Option<u64> {
    lines.iter().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == key).then(|| num::<u64>(fields(v).first()?))?
    })
}

fn memory(lines: &[&str]) -> (Option<Gauge>, Option<Gauge>) {
    if let Some(total) = keyed(lines, "MemTotal") {
        // Linux: kB. Available is what the kernel says can be had without
        // swapping; a kernel too old to say gets free + buffers + cache.
        let available = keyed(lines, "MemAvailable").or_else(|| {
            Some(keyed(lines, "MemFree")? + keyed(lines, "Buffers")? + keyed(lines, "Cached")?)
        });
        let memory = available.map(|a| Gauge {
            total: total * 1024,
            used: total.saturating_sub(a) * 1024,
        });
        let swap = match (keyed(lines, "SwapTotal"), keyed(lines, "SwapFree")) {
            (Some(t), Some(f)) if t > 0 => Some(Gauge {
                total: t * 1024,
                used: t.saturating_sub(f) * 1024,
            }),
            _ => None,
        };
        return (memory, swap);
    }
    // macOS: hw.memsize, hw.pagesize, then vm_stat's page counts.
    let mut nums = lines.iter().filter_map(|l| num::<u64>(l));
    let (Some(total), Some(page)) = (nums.next(), nums.next()) else {
        return (None, None);
    };
    let pages = |k: &str| keyed(lines, k).unwrap_or(0);
    let available =
        (pages("Pages free") + pages("Pages inactive") + pages("Pages speculative")) * page;
    (
        Some(Gauge {
            total,
            used: total.saturating_sub(available),
        }),
        None,
    )
}

/// `vm.swapusage`: "total = 2048.00M  used = 1024.00M  free = 1024.00M".
fn swap_usage(line: &str) -> Option<Gauge> {
    let after = |key: &str| {
        let i = line.find(key)?;
        let rest = line[i + key.len()..].trim_start_matches([' ', '=']);
        sized(fields(rest).first()?)
    };
    let total = after("total")?;
    (total > 0).then(|| Gauge {
        total,
        used: after("used").unwrap_or(0),
    })
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
        let mut value = String::new();
        let mut i = 0;
        while i < raw.len() {
            if raw[i..].starts_with("\\x") && i + 4 <= raw.len() {
                if let Ok(b) = u8::from_str_radix(&raw[i + 2..i + 4], 16) {
                    value.push(b as char);
                    i += 4;
                    continue;
                }
            }
            let c = raw[i..].chars().next().unwrap();
            value.push(c);
            i += c.len_utf8();
        }
        out.push((key, value));
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
    fn chain<'a>(recs: &'a [Rec], r: &'a Rec) -> Vec<&'a Rec> {
        let mut out = vec![r];
        while let Some(p) = out.last().unwrap().drive.as_ref() {
            match recs.iter().find(|d| &d.name == p) {
                Some(d) if !out.iter().any(|o| o.name == d.name) => out.push(d),
                _ => break,
            }
        }
        out
    }
    let mut out = Devices::default();
    for r in &recs {
        let up = chain(&recs, r);
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
        let slice = |a: usize, b: usize| -> String {
            chars[a.min(chars.len())..b.min(chars.len())]
                .iter()
                .collect::<String>()
                .trim()
                .to_string()
        };
        let colon = l.find(':').map(|i| l[..i].chars().count() + 1).unwrap_or(0);
        let kind = slice(colon, name_col);
        let name = slice(name_col, size_col);
        rows.push((kind, name, id, size));
    }
    flush(&disk, &store, &mut rows, &mut out);
    out
}

fn gpus(lines: &[&str]) -> Vec<Gpu> {
    lines
        .iter()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            if f.len() < 5 {
                return None;
            }
            let mib = |s: &str| num::<f64>(s).map(|v| (v * 1024.0 * 1024.0) as u64);
            Some(Gpu {
                index: f[0].parse().ok()?,
                name: f[1].to_string(),
                utilization: num(f[2]),
                memory: match (mib(f[3]), mib(f[4])) {
                    (Some(used), Some(total)) if total > 0 => Some(Gauge { total, used }),
                    _ => None,
                },
            })
        })
        .collect()
}

fn tmux(sessions: &[&str], windows: &[&str]) -> Option<Vec<TmuxSession>> {
    let mut it = sessions.iter();
    if it.next().map(|l| l.trim()) != Some("present") {
        return None;
    }
    let mut out: Vec<TmuxSession> = it
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 5 {
                return None;
            }
            Some(TmuxSession {
                name: f[0].to_string(),
                created: num(f[2]),
                activity: num(f[4]),
                attached: num(f[3]).unwrap_or(0),
                windows: Vec::new(),
                path: String::new(),
            })
        })
        .collect();
    for l in windows {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 6 {
            continue;
        }
        if let Some(s) = out.iter_mut().find(|s| s.name == f[0]) {
            let path = f.get(6).unwrap_or(&"").to_string();
            if f[3] == "1" {
                s.path = path.clone();
            }
            s.windows.push(TmuxWindow {
                index: num(f[1]).unwrap_or(0),
                name: f[2].to_string(),
                active: f[3] == "1",
                command: f[4].to_string(),
                path,
                panes: num(f[5]).unwrap_or(1),
            });
        }
    }
    Some(out)
}

/// `screen -ls`: "There are screens on:", then one tab-indented line per
/// session, `pid.name  (date)  (Attached|Detached|Multi|Dead ???)`.
fn screen(lines: &[&str], cwds: &[&str]) -> Option<Vec<ScreenSession>> {
    let mut it = lines.iter();
    if it.next().map(|l| l.trim()) != Some("present") {
        return None;
    }
    let cwd = |pid: u32| {
        cwds.iter()
            .find_map(|l| {
                let (p, path) = l.split_once('\t')?;
                (p.trim().parse::<u32>().ok()? == pid).then(|| path.trim().to_string())
            })
            .unwrap_or_default()
    };
    Some(
        it.filter_map(|l| {
            if !l.starts_with([' ', '\t']) {
                return None;
            }
            let f = fields(l);
            let (pid, name) = f.first()?.split_once('.')?;
            let pid: u32 = pid.parse().ok()?;
            let state = f[1..].join(" ");
            if state.contains("Dead") {
                return None;
            }
            Some(ScreenSession {
                id: f[0].to_string(),
                name: name.to_string(),
                pid,
                attached: state.contains("Attached") || state.contains("Multi"),
                path: cwd(pid),
            })
        })
        .collect(),
    )
}

/// `docker ps --format` with tabs between id, name, image, status, ports;
/// the first line says which tool answered. After them, one `M` line per
/// container from `docker inspect`: its name (docker writes it with a
/// leading slash) and the host side of each bind mount, tabs between.
fn containers(lines: &[&str]) -> Option<Containers> {
    let mut it = lines.iter();
    let first = it.next()?.trim();
    let tool = first.strip_prefix("present")?.trim();
    let tool = if tool.is_empty() { "docker" } else { tool };
    let mut mounts: HashMap<&str, Vec<String>> = HashMap::new();
    for l in lines.iter().filter_map(|l| l.strip_prefix("M\t")) {
        let mut f = l.split('\t');
        let name = f.next().unwrap_or("").trim_start_matches('/');
        mounts.insert(
            name,
            f.filter(|m| !m.is_empty()).map(str::to_string).collect(),
        );
    }
    Some(Containers {
        tool: tool.to_string(),
        list: it
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() < 4 || f[0] == "M" {
                    return None;
                }
                Some(Container {
                    id: f[0].to_string(),
                    name: f[1].to_string(),
                    image: f[2].to_string(),
                    status: f[3].to_string(),
                    ports: f.get(4).unwrap_or(&"").to_string(),
                    mounts: mounts.get(f[1]).cloned().unwrap_or_default(),
                })
            })
            .collect(),
    })
}

/// `squeue -h -o "%i|%j|%T|%M|%D|%R|%P|%Z"`.
fn slurm(lines: &[&str]) -> Option<Vec<SlurmJob>> {
    let mut it = lines.iter();
    if it.next().map(|l| l.trim()) != Some("present") {
        return None;
    }
    Some(
        it.filter_map(|l| {
            let f: Vec<&str> = l.trim().split('|').collect();
            if f.len() < 7 {
                return None;
            }
            Some(SlurmJob {
                id: f[0].to_string(),
                name: f[1].to_string(),
                state: f[2].to_string(),
                time: f[3].to_string(),
                nodes: f[4].parse().unwrap_or(0),
                reason: f[5].to_string(),
                partition: f[6].to_string(),
                path: f.get(7).unwrap_or(&"").to_string(),
            })
        })
        .collect(),
    )
}

/// `qstat -f`: `Job Id: N` then `    key = value` lines, a long value
/// going on in lines that start with a tab.
fn pbs(lines: &[&str]) -> Option<Vec<PbsJob>> {
    let mut it = lines.iter();
    if it.next().map(|l| l.trim()) != Some("present") {
        return None;
    }
    let mut jobs: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for l in it {
        if let Some(id) = l.strip_prefix("Job Id:") {
            jobs.push((id.trim().to_string(), Vec::new()));
        } else if let Some(cur) = jobs.last_mut() {
            if l.starts_with('\t') {
                if let Some(last) = cur.1.last_mut() {
                    last.1.push_str(l.trim());
                }
            } else if let Some((k, v)) = l.split_once('=') {
                cur.1.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    Some(
        jobs.into_iter()
            .map(|(id, attrs)| {
                let get = |k: &str| {
                    attrs
                        .iter()
                        .find(|(kk, _)| kk == k)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                };
                let exec = get("exec_host");
                let mut path = get("init_work_dir");
                if path.is_empty() {
                    path = get("Variable_List")
                        .split(',')
                        .find_map(|v| v.strip_prefix("PBS_O_WORKDIR=").map(str::to_string))
                        .unwrap_or_default();
                }
                PbsJob {
                    id,
                    name: get("Job_Name"),
                    state: get("job_state"),
                    time: get("resources_used.walltime"),
                    nodes: get("Resource_List.nodect").parse().unwrap_or(0),
                    node: exec.split(['/', '+']).next().unwrap_or("").to_string(),
                    queue: get("queue"),
                    path,
                }
            })
            .collect(),
    )
}

/// `ps -eo user,pid,args` lines of VNC servers, and `screensharing` on a
/// Mac whose Screen Sharing or Remote Management is on: launchd holds
/// its port, 5900 (`vnc-server` in `/etc/services`), as root, so a
/// user's `lsof` never lists it and `ps` only shows `screensharingd`
/// while a viewer is connected; `launchctl print` of the service, which
/// any user may ask for, says whether it is loaded.
fn vnc(lines: &[&str]) -> Vec<VncSession> {
    lines
        .iter()
        .filter_map(|l| {
            if l.trim() == "screensharing" {
                return Some(VncSession {
                    user: String::new(),
                    pid: None,
                    program: "Screen Sharing".into(),
                    display: None,
                    port: 5900,
                });
            }
            let f = fields(l);
            if f.len() < 3 {
                return None;
            }
            let program = f[2].rsplit('/').next()?.to_string();
            let args = &f[3..];
            let display = args.iter().find_map(|a| {
                a.strip_prefix(':')
                    .and_then(|d| d.split('.').next()?.parse().ok())
            });
            let rfbport = args
                .windows(2)
                .find(|w| w[0] == "-rfbport")
                .and_then(|w| w[1].parse::<u16>().ok());
            Some(VncSession {
                user: f[0].to_string(),
                pid: Some(f[1].parse().ok()?),
                program,
                display,
                port: rfbport.unwrap_or(5900 + display.unwrap_or(0) as u16),
            })
        })
        .collect()
}

/// `addr:port` as ss and lsof write it (`0.0.0.0:22`, `[::]:22`, `*:22`).
fn addr_port(word: &str) -> Option<(String, u16)> {
    let (addr, port) = word.rsplit_once(':')?;
    let port = port.parse().ok()?;
    Some((addr.trim_matches(['[', ']']).to_string(), port))
}

/// `ss -ltnpH` lines (`LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:(("sshd",pid=1,fd=3))`),
/// or, after a first line saying so, `lsof -nP -iTCP -sTCP:LISTEN`
/// (`sshd 1 root 3u IPv4 … TCP *:22 (LISTEN)`). One row per port, the
/// first binding of it.
fn listeners(lines: &[&str]) -> Vec<Listener> {
    let lsof = lines.first().map(|l| l.trim() == "lsof").unwrap_or(false);
    let mut out: Vec<Listener> = Vec::new();
    for l in lines.iter().skip(if lsof { 1 } else { 0 }) {
        let f = fields(l);
        let got = if lsof {
            if f.len() < 9 {
                continue;
            }
            addr_port(f[8]).map(|(address, port)| Listener {
                port,
                address,
                process: f[0].to_string(),
                pid: f[1].parse().ok(),
            })
        } else {
            if f.len() < 4 || f[0] != "LISTEN" {
                continue;
            }
            let rest = f[5..].join(" ");
            let process = rest
                .split("((\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("")
                .to_string();
            let pid = rest
                .split("pid=")
                .nth(1)
                .and_then(|s| s.split([',', ')']).next()?.parse().ok());
            addr_port(f[3]).map(|(address, port)| Listener {
                port,
                address,
                process,
                pid,
            })
        };
        if let Some(x) = got {
            if !out.iter().any(|o| o.port == x.port) {
                out.push(x);
            }
        }
    }
    out.sort_by_key(|x| x.port);
    out
}

/// `who`: `user tty when (from)`, the date in the local form of the host.
fn logins(lines: &[&str]) -> Vec<Login> {
    lines
        .iter()
        .filter_map(|l| {
            let f = fields(l);
            if f.len() < 3 {
                return None;
            }
            let (from, rest) = match f.last() {
                Some(w) if w.starts_with('(') && w.ends_with(')') => {
                    (Some(w[1..w.len() - 1].to_string()), &f[2..f.len() - 1])
                }
                _ => (None, &f[2..]),
            };
            Some(Login {
                user: f[0].to_string(),
                tty: f[1].to_string(),
                since: rest.join(" "),
                from,
            })
        })
        .collect()
}

fn processes(lines: &[&str]) -> Vec<Process> {
    lines
        .iter()
        .filter_map(|l| {
            let f = fields(l);
            if f.len() < 5 {
                return None;
            }
            Some(Process {
                pid: f[0].parse().ok()?,
                user: f[1].to_string(),
                cpu: f[2].parse().ok()?,
                memory: f[3].parse().ok()?,
                command: f[4..].join(" "),
            })
        })
        .collect()
}

/// Read the script's output.
pub fn parse(text: &str) -> Snapshot {
    let secs = sections(text);
    let get = |name: &str| -> &[&str] {
        secs.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, l)| l.as_slice())
            .unwrap_or(&[])
    };
    let first = |name: &str| {
        get(name)
            .first()
            .map(|l| l.trim().to_string())
            .unwrap_or_default()
    };
    let kernel = first("uname");
    let os = {
        let o = first("os");
        if o.is_empty() {
            kernel.clone()
        } else {
            o
        }
    };
    let now: Option<f64> = get("now").first().and_then(|l| num(l));
    let uptime = get("uptime").first().and_then(|l| {
        if let Some(v) = floats(l, 1).filter(|_| !l.contains("sec")) {
            return Some(v[0]);
        }
        // `{ sec = 1700000000, usec = 0 } Tue Nov 14 …`
        let i = l.find("sec =")?;
        let boot: f64 = num(fields(&l[i + 5..]).first()?.trim_end_matches(','))?;
        Some((now? - boot).max(0.0))
    });
    let load = get("load")
        .first()
        .and_then(|l| floats(l, 3))
        .map(|v| [v[0], v[1], v[2]]);
    let cpus = get("ncpu").first().and_then(|l| num(l));
    let cpu_ticks = get("stat").first().and_then(|l| {
        let f = fields(l);
        if f.first() != Some(&"cpu") || f.len() < 5 {
            return None;
        }
        let v: Vec<u64> = f[1..].iter().filter_map(|w| w.parse().ok()).collect();
        let total: u64 = v.iter().sum();
        let idle = v[3] + v.get(4).copied().unwrap_or(0);
        Some(CpuTicks {
            busy: total.saturating_sub(idle),
            total,
        })
    });
    // "CPU usage: 3.12% user, 5.2% sys, 91.6% idle"
    let cpu_percent = get("topcpu").last().and_then(|l| {
        let i = l.find("idle")?;
        let before = l[..i].trim_end();
        let idle: f64 = before
            .rsplit(' ')
            .next()?
            .trim_end_matches('%')
            .parse()
            .ok()?;
        Some((100.0 - idle).clamp(0.0, 100.0))
    });
    let logins = logins(get("who"));
    let devices = devices(get("blk"));
    let up = mounted(get("df"));
    let disks = disks(get("df"), &devices);
    let volumes = devices
        .volumes
        .iter()
        .filter(|v| !up.contains(&v.device))
        .cloned()
        .collect();
    let (memory, swap_linux) = memory(get("mem"));
    let swap = swap_linux.or_else(|| get("swap").first().and_then(|l| swap_usage(l)));
    Snapshot {
        hostname: first("hostname"),
        os,
        kernel,
        uptime,
        load,
        cpus,
        cpu_ticks,
        cpu_percent,
        memory,
        swap,
        disks,
        volumes,
        gpus: gpus(get("gpu")),
        tmux: tmux(get("tmux"), get("windows")),
        screen: screen(get("screen"), get("screencwd")),
        docker: containers(get("docker")),
        slurm: slurm(get("slurm")),
        pbs: pbs(get("pbs")),
        vnc: vnc(get("vnc")),
        processes: processes(get("ps")),
        users: Some(logins.len() as u32),
        logins,
        listeners: listeners(get("ports")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scripts_fit_in_single_quotes() {
        for script in [SCRIPT, MOUNT, UNMOUNT, EJECT] {
            assert!(
                !script.contains('\''),
                "a single quote would end the sh -c argument"
            );
            assert!(
                !script.contains('\n'),
                "a csh login shell would choke on a newline"
            );
            assert!(!script.contains('!'), "csh would expand it as history");
        }
        assert!(command().starts_with("sh -c '"));
        assert_eq!(
            disk_command("mount", "/dev/sdb1", false).unwrap(),
            format!("sh -c '{MOUNT}' sh '/dev/sdb1'")
        );
        assert_eq!(
            disk_command("mount", "/dev/sdb1", true).unwrap(),
            format!("sh -c '{MOUNT}' sh '/dev/sdb1' ask")
        );
        assert!(disk_command("eject", "/dev/disk4", false)
            .unwrap()
            .contains("diskutil eject"));
        assert!(wants_password("Error mounting /dev/sda1: GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain: Not authorized to perform operation"));
        assert!(wants_password(
            "Error powering off drive: Not authorized to perform operation (udisks-error-quark, 4)"
        ));
        assert!(wants_password("sudo: a password is required"));
        assert!(!wants_password("umount: /media/me/STICK: target is busy."));
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

    const LINUX: &str = r#"
@@shellder-console:hostname
gpu-box
@@shellder-console:uname
Linux 6.8.0-45-generic
@@shellder-console:os
Ubuntu 24.04.1 LTS
@@shellder-console:now
1700000000
@@shellder-console:uptime
360000.12 2800000.00
@@shellder-console:load
0.52 0.61 0.70 2/1234 56789
@@shellder-console:ncpu
32
@@shellder-console:stat
cpu  1000 50 500 8000 200 0 10 0 0 0
@@shellder-console:topcpu
@@shellder-console:mem
MemTotal:       65536000 kB
MemFree:         1000000 kB
MemAvailable:   40000000 kB
Buffers:          500000 kB
Cached:         20000000 kB
SwapTotal:       8388604 kB
SwapFree:        8000000 kB
@@shellder-console:swap
@@shellder-console:df
Filesystem     1024-blocks      Used  Available Capacity Mounted on
tmpfs              6553600      2000    6551600       1% /run
/dev/nvme0n1p2   981876212 500000000  431876212      54% /
/dev/sda1       7812345678 100000000 7712345678       2% /mnt/data disk
efivarfs               256        30        221      12% /sys/firmware/efi/efivars
overlay          981876212 500000000  431876212      54% /var/lib/docker/overlay2/abc/merged
Shellder_2.1.2_amd64.AppImage 94208 94208     0     100% /tmp/.mount_ShelldKaeaHd
/dev/sdd1         61035520  10000000   51035520      17% /run/media/alice/BACKUP
@@shellder-console:blk
NAME="/dev/loop0" TYPE="loop" FSTYPE="squashfs" SIZE="4096" LABEL="" MOUNTPOINT="/snap/core/123" RM="0" HOTPLUG="0" TRAN="" PKNAME=""
NAME="/dev/sda" TYPE="disk" FSTYPE="" SIZE="8000000000000" LABEL="" MOUNTPOINT="" RM="0" HOTPLUG="1" TRAN="usb" PKNAME=""
NAME="/dev/sda1" TYPE="part" FSTYPE="ext4" SIZE="7999842951168" LABEL="data disk" MOUNTPOINT="/mnt/data disk" RM="0" HOTPLUG="1" TRAN="" PKNAME="/dev/sda"
NAME="/dev/sdb" TYPE="disk" FSTYPE="" SIZE="32000000000" LABEL="" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="usb" PKNAME=""
NAME="/dev/sdb1" TYPE="part" FSTYPE="vfat" SIZE="31998951424" LABEL="STICK\x20A" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="" PKNAME="/dev/sdb"
NAME="/dev/sdb2" TYPE="part" FSTYPE="crypto_LUKS" SIZE="1048576" LABEL="" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="" PKNAME="/dev/sdb"
NAME="/dev/mapper/luks-1" TYPE="crypt" FSTYPE="ext4" SIZE="1000000" LABEL="SAFE" MOUNTPOINT="" RM="0" HOTPLUG="0" TRAN="" PKNAME="/dev/sdb2"
NAME="/dev/sdb3" TYPE="part" FSTYPE="swap" SIZE="1048576" LABEL="" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="" PKNAME="/dev/sdb"
NAME="/dev/sdc" TYPE="disk" FSTYPE="" SIZE="0" LABEL="" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="usb" PKNAME=""
NAME="/dev/sdd" TYPE="disk" FSTYPE="" SIZE="64000000000" LABEL="" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="usb" PKNAME=""
NAME="/dev/sdd1" TYPE="part" FSTYPE="exfat" SIZE="62500372480" LABEL="BACKUP" MOUNTPOINT="/run/media/alice/BACKUP" RM="1" HOTPLUG="1" TRAN="" PKNAME="/dev/sdd"
NAME="/dev/sde" TYPE="disk" FSTYPE="" SIZE="4000000000000" LABEL="" MOUNTPOINT="" RM="0" HOTPLUG="1" TRAN="sata" PKNAME=""
NAME="/dev/sde1" TYPE="part" FSTYPE="xfs" SIZE="4000000000000" LABEL="" MOUNTPOINT="" RM="0" HOTPLUG="1" TRAN="" PKNAME="/dev/sde"
NAME="/dev/sr0" TYPE="rom" FSTYPE="" SIZE="1073741312" LABEL="" MOUNTPOINT="" RM="1" HOTPLUG="1" TRAN="sata" PKNAME=""
NAME="/dev/nvme0n1" TYPE="disk" FSTYPE="" SIZE="1024209543168" LABEL="" MOUNTPOINT="" RM="0" HOTPLUG="0" TRAN="nvme" PKNAME=""
NAME="/dev/nvme0n1p1" TYPE="part" FSTYPE="vfat" SIZE="1127219200" LABEL="" MOUNTPOINT="/boot/efi" RM="0" HOTPLUG="0" TRAN="" PKNAME="/dev/nvme0n1"
NAME="/dev/nvme0n1p2" TYPE="part" FSTYPE="ext4" SIZE="1005441241088" LABEL="" MOUNTPOINT="/" RM="0" HOTPLUG="0" TRAN="" PKNAME="/dev/nvme0n1"
@@shellder-console:gpu
0, NVIDIA A100-SXM4-40GB, 87, 30000, 40960
1, NVIDIA A100-SXM4-40GB, 0, 0, 40960
@@shellder-console:tmux
present
main	3	1699990000	1	1699999000
train	1	1699995000	0	1699999500
@@shellder-console:windows
main	0	zsh	0	zsh	1	/home/alice
main	1	vim	1	vim	2	/home/alice/src
main	2	htop	0	htop	1	/home/alice
train	0	python	1	python3	1	/data/train
@@shellder-console:screen
present
There are screens on:
	12345.main	(10/12/2026 12:00:00 AM)	(Attached)
	23456.pts-0.gpu-box	(10/11/2026 11:00:00 PM)	(Detached)
	34567.old	(Dead ???)
2 Sockets in /run/screen/S-alice.
@@shellder-console:screencwd
12345	/home/alice/notes
@@shellder-console:docker
present docker
0123abcd	web	nginx:1.27	Up 3 days	0.0.0.0:8080->80/tcp
4567efgh	db	postgres:16	Up 3 days (healthy)	
M	/web	/srv/web/html	/srv/web/conf
M	/db
@@shellder-console:slurm
present
12345|train-llm|RUNNING|1-02:03:04|2|gpu[01-02]|gpu|/home/alice/llm
12346|eval|PENDING|0:00|1|(Priority)|gpu|/home/alice/eval
@@shellder-console:pbs
present
Job Id: 777.pbs01
    Job_Name = train
    Job_Owner = alice@login01
    resources_used.walltime = 01:02:03
    job_state = R
    queue = gpu
    exec_host = node01/0*4+node02/0*4
    Resource_List.nodect = 2
    Variable_List = PBS_O_HOME=/home/alice,PBS_O_LANG=en_US.UTF-8,
	PBS_O_WORKDIR=/home/alice/run,PBS_O_PATH=/usr/bin
Job Id: 778.pbs01
    Job_Name = eval
    job_state = Q
    queue = gpu
    Resource_List.nodect = 1
@@shellder-console:vnc
alice     2222 /usr/bin/Xtigervnc :1 -desktop gpu-box:1 -rfbport 5901 -auth /home/alice/.Xauthority
bob       3333 x11vnc -display :0 -rfbport 5910 -forever
carol     4444 Xvnc :3 -geometry 1920x1080
@@shellder-console:ps
 4321 alice    250.0  12.3 python3
    1 root       0.0   0.1 systemd
@@shellder-console:ports
LISTEN 0      128          0.0.0.0:22        0.0.0.0:*    users:(("sshd",pid=1,fd=3))
LISTEN 0      4096       127.0.0.1:8888      0.0.0.0:*    users:(("jupyter-lab",pid=7777,fd=6))
LISTEN 0      128             [::]:22           [::]:*
LISTEN 0      5                  *:5901            *:*
@@shellder-console:who
alice    pts/0        2026-10-12 00:10 (10.0.0.2)
bob      tty1         2026-10-11 09:00
carol    pts/3        2026-10-12 00:30 (tmux(1234).%0)
"#;

    #[test]
    fn reads_a_linux_host() {
        let s = parse(LINUX);
        assert_eq!(s.hostname, "gpu-box");
        assert_eq!(s.os, "Ubuntu 24.04.1 LTS");
        assert_eq!(s.kernel, "Linux 6.8.0-45-generic");
        assert_eq!(s.uptime, Some(360000.12));
        assert_eq!(s.load, Some([0.52, 0.61, 0.70]));
        assert_eq!(s.cpus, Some(32));
        assert_eq!(
            s.cpu_ticks,
            Some(CpuTicks {
                busy: 1560,
                total: 9760
            })
        );
        assert_eq!(s.cpu_percent, None);
        assert_eq!(
            s.memory,
            Some(Gauge {
                total: 65536000 * 1024,
                used: 25536000 * 1024
            })
        );
        assert_eq!(
            s.swap,
            Some(Gauge {
                total: 8388604 * 1024,
                used: 388604 * 1024
            })
        );
        let mounts: Vec<&str> = s.disks.iter().map(|d| d.mount.as_str()).collect();
        assert_eq!(
            mounts,
            vec!["/", "/mnt/data disk", "/run/media/alice/BACKUP"],
            "no AppImage mount"
        );
        assert_eq!(s.disks[0].used, 500000000 * 1024);
        assert!(!s.disks[0].removable && s.disks[0].drive.is_none());
        assert!(
            s.disks[1].removable,
            "a USB drive: TRAN says so, RM does not"
        );
        assert_eq!(s.disks[1].drive.as_deref(), Some("/dev/sda"));
        assert_eq!(s.disks[2].drive.as_deref(), Some("/dev/sdd"));
        // Not mounted: the stick's vfat partition and the LUKS volume
        // opened on its second one (the drive found through the chain),
        // but not the LUKS container itself or the swap, not the empty
        // drive, not the drive without a disc, not an internal SATA disk
        // on a hot-plug port (sde), and nothing else internal.
        assert_eq!(
            s.volumes,
            vec![
                Volume {
                    device: "/dev/sdb1".into(),
                    drive: Some("/dev/sdb".into()),
                    label: "STICK A".into(),
                    fstype: "vfat".into(),
                    size: 31998951424,
                },
                Volume {
                    device: "/dev/mapper/luks-1".into(),
                    drive: Some("/dev/sdb".into()),
                    label: "SAFE".into(),
                    fstype: "ext4".into(),
                    size: 1000000,
                }
            ]
        );
        assert_eq!(s.gpus.len(), 2);
        assert_eq!(s.gpus[0].utilization, Some(87.0));
        assert_eq!(
            s.gpus[0].memory,
            Some(Gauge {
                total: 40960 << 20,
                used: 30000 << 20
            })
        );
        let tmux = s.tmux.expect("tmux present");
        assert_eq!(tmux.len(), 2);
        assert_eq!(tmux[0].name, "main");
        assert_eq!(tmux[0].attached, 1);
        assert_eq!(tmux[0].created, Some(1699990000.0));
        assert_eq!(tmux[0].windows.len(), 3);
        assert!(tmux[0].windows[1].active);
        assert_eq!(tmux[0].windows[1].panes, 2);
        assert_eq!(tmux[1].windows[0].command, "python3");
        assert_eq!(
            tmux[0].path, "/home/alice/src",
            "the active window's directory"
        );
        assert_eq!(tmux[0].windows[0].path, "/home/alice");
        assert_eq!(tmux[1].path, "/data/train");
        assert_eq!(s.processes.len(), 2);
        assert_eq!(s.processes[0].pid, 4321);
        assert_eq!(s.processes[0].cpu, 250.0);
        assert_eq!(s.processes[0].command, "python3");
        let screen = s.screen.expect("screen present");
        assert_eq!(screen.len(), 2, "the dead one is left out");
        assert_eq!(screen[0].id, "12345.main");
        assert_eq!(
            (screen[0].name.as_str(), screen[0].pid, screen[0].attached),
            ("main", 12345, true)
        );
        assert_eq!(
            (screen[1].name.as_str(), screen[1].attached),
            ("pts-0.gpu-box", false)
        );
        assert_eq!(screen[0].path, "/home/alice/notes");
        assert_eq!(screen[1].path, "");
        let docker = s.docker.expect("docker present");
        assert_eq!(docker.tool, "docker");
        assert_eq!(docker.list.len(), 2);
        assert_eq!(docker.list[0].mounts, ["/srv/web/html", "/srv/web/conf"]);
        assert!(docker.list[1].mounts.is_empty());
        assert_eq!(
            (docker.list[0].name.as_str(), docker.list[0].ports.as_str()),
            ("web", "0.0.0.0:8080->80/tcp")
        );
        assert_eq!(
            (
                docker.list[1].status.as_str(),
                docker.list[1].ports.as_str()
            ),
            ("Up 3 days (healthy)", "")
        );
        let slurm = s.slurm.expect("slurm present");
        assert_eq!(slurm.len(), 2);
        assert_eq!(
            (
                slurm[0].id.as_str(),
                slurm[0].state.as_str(),
                slurm[0].nodes,
                slurm[0].reason.as_str()
            ),
            ("12345", "RUNNING", 2, "gpu[01-02]")
        );
        assert_eq!(slurm[0].path, "/home/alice/llm");
        assert_eq!(
            (slurm[1].state.as_str(), slurm[1].time.as_str()),
            ("PENDING", "0:00")
        );
        let pbs = s.pbs.expect("pbs present");
        assert_eq!(pbs.len(), 2);
        assert_eq!(
            (
                pbs[0].id.as_str(),
                pbs[0].name.as_str(),
                pbs[0].state.as_str(),
                pbs[0].time.as_str()
            ),
            ("777.pbs01", "train", "R", "01:02:03")
        );
        assert_eq!(
            (pbs[0].nodes, pbs[0].node.as_str(), pbs[0].queue.as_str()),
            (2, "node01", "gpu")
        );
        assert_eq!(
            pbs[0].path, "/home/alice/run",
            "PBS_O_WORKDIR of Variable_List"
        );
        assert_eq!(pbs[1].path, "");
        assert_eq!(
            (
                pbs[1].state.as_str(),
                pbs[1].node.as_str(),
                pbs[1].time.as_str()
            ),
            ("Q", "", "")
        );
        assert_eq!(s.vnc.len(), 3);
        assert_eq!(
            (s.vnc[0].program.as_str(), s.vnc[0].display, s.vnc[0].port),
            ("Xtigervnc", Some(1), 5901)
        );
        assert_eq!(
            (s.vnc[1].user.as_str(), s.vnc[1].display, s.vnc[1].port),
            ("bob", Some(0), 5910)
        );
        assert_eq!(
            (s.vnc[2].pid, s.vnc[2].display, s.vnc[2].port),
            (Some(4444), Some(3), 5903)
        );
        assert_eq!(s.logins.len(), 3);
        assert_eq!(s.logins[0].from.as_deref(), Some("10.0.0.2"));
        assert_eq!(s.logins[0].since, "2026-10-12 00:10");
        assert_eq!(s.logins[1].from, None);
        assert_eq!(s.logins[1].tty, "tty1");
        assert_eq!(s.users, Some(3));
        let ports: Vec<(u16, &str, &str, Option<u32>)> = s
            .listeners
            .iter()
            .map(|l| (l.port, l.address.as_str(), l.process.as_str(), l.pid))
            .collect();
        assert_eq!(
            ports,
            vec![
                (22, "0.0.0.0", "sshd", Some(1)),
                (5901, "*", "", None),
                (8888, "127.0.0.1", "jupyter-lab", Some(7777))
            ]
        );
    }

    const MACOS: &str = "
@@shellder-console:hostname
laptop.local
@@shellder-console:uname
Darwin 24.1.0
@@shellder-console:os
macOS 15.1
@@shellder-console:now
1700003600
@@shellder-console:uptime
{ sec = 1700000000, usec = 123 } Tue Nov 14 22:13:20 2023
@@shellder-console:load
{ 1.52 1.61 1.70 }
@@shellder-console:ncpu
10
@@shellder-console:stat
@@shellder-console:topcpu
CPU usage: 3.12% user, 5.2% sys, 91.68% idle
CPU usage: 10.0% user, 5.0% sys, 85.0% idle
@@shellder-console:mem
17179869184
16384
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                               10000.
Pages active:                            300000.
Pages inactive:                          200000.
Pages speculative:                        40000.
@@shellder-console:swap
total = 2048.00M  used = 512.00M  free = 1536.00M  (encrypted)
@@shellder-console:df
Filesystem    1024-blocks      Used Available Capacity  Mounted on
/dev/disk3s1s1  971350180  10000000 400000000     3%    /
devfs                 200       200         0   100%    /dev
/dev/disk3s6    971350180   2000000 400000000     1%    /System/Volumes/VM
/dev/disk3s2    971350180   6000000 400000000     2%    /System/Volumes/Preboot
/dev/disk3s5    971350180 550000000 400000000    58%    /System/Volumes/Data
map auto_home           0         0         0   100%    /System/Volumes/Data/home
/dev/disk4s1     31227904  10000000  21227904    33%    /Volumes/STICK
@@shellder-console:blk
diskutil
/dev/disk4 (external, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:     FDisk_partition_scheme                        *32.0 GB    disk4
   1:                 DOS_FAT_32 STICK                   32.0 GB    disk4s1

/dev/disk6 (external, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      GUID_partition_scheme                        *2.0 TB     disk6
   1:                        EFI EFI                     209.7 MB   disk6s1
   2:                 Apple_APFS Container disk7         2.0 TB     disk6s2

/dev/disk7 (synthesized):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      APFS Container Scheme -                      +2.0 TB     disk7
                                 Physical Store disk6s2
   1:                APFS Volume Time Machine            1.2 TB     disk7s1
   2:                APFS Volume Preboot                 20.0 MB    disk7s2
@@shellder-console:gpu
@@shellder-console:tmux
@@shellder-console:windows
@@shellder-console:vnc
screensharing
@@shellder-console:ps
  500 me        12.5   3.0 WindowServer
@@shellder-console:ports
lsof
launchd       1           root   10u  IPv6 0x1234      0t0  TCP *:22 (LISTEN)
Xvnc       2222             me    5u  IPv4 0x5678      0t0  TCP 127.0.0.1:5901 (LISTEN)
@@shellder-console:who
me               console  Oct 12 00:10
me               ttys000  Oct 12 00:12 (10.0.0.9)
";

    #[test]
    fn reads_a_macos_host() {
        let s = parse(MACOS);
        assert_eq!(s.os, "macOS 15.1");
        assert_eq!(s.uptime, Some(3600.0));
        assert_eq!(s.load, Some([1.52, 1.61, 1.70]));
        assert_eq!(s.cpu_ticks, None);
        assert_eq!(s.cpu_percent, Some(15.0), "the second sample of top");
        assert_eq!(
            s.memory,
            Some(Gauge {
                total: 17179869184,
                used: 17179869184 - 250000 * 16384
            })
        );
        assert_eq!(
            s.swap,
            Some(Gauge {
                total: 2048 << 20,
                used: 512 << 20
            })
        );
        let mounts: Vec<&str> = s.disks.iter().map(|d| d.mount.as_str()).collect();
        assert_eq!(mounts, vec!["/", "/System/Volumes/Data", "/Volumes/STICK"]);
        assert!(!s.disks[0].removable);
        assert!(s.disks[2].removable);
        assert_eq!(s.disks[2].drive.as_deref(), Some("/dev/disk4"));
        // The APFS volume on the other external disk, on the physical
        // disk under its container; not the EFI slice, the container,
        // or APFS's Preboot.
        assert_eq!(
            s.volumes,
            vec![Volume {
                device: "/dev/disk7s1".into(),
                drive: Some("/dev/disk6".into()),
                label: "Time Machine".into(),
                fstype: "APFS Volume".into(),
                size: 1_200_000_000_000,
            }]
        );
        assert!(s.gpus.is_empty());
        assert_eq!(s.tmux, None, "no tmux on the host");
        assert_eq!(s.screen, None);
        assert_eq!(s.docker, None);
        assert_eq!(s.slurm, None);
        assert_eq!(s.pbs, None);
        // Screen Sharing is on: launchd's port, which lsof did not list.
        assert_eq!(
            s.vnc,
            vec![VncSession {
                user: String::new(),
                pid: None,
                program: "Screen Sharing".into(),
                display: None,
                port: 5900,
            }]
        );
        assert_eq!(s.processes.len(), 1);
        assert_eq!(s.logins[0].since, "Oct 12 00:10");
        assert_eq!(s.logins[1].from.as_deref(), Some("10.0.0.9"));
        assert_eq!(s.users, Some(2));
        assert_eq!(s.listeners.len(), 2);
        assert_eq!(
            (
                s.listeners[1].port,
                s.listeners[1].process.as_str(),
                s.listeners[1].pid,
                s.listeners[1].address.as_str()
            ),
            (5901, "Xvnc", Some(2222), "127.0.0.1")
        );
    }

    #[test]
    fn tmux_without_sessions_and_an_os_without_a_name() {
        let s = parse("@@shellder-console:uname\nFreeBSD 14.1\n@@shellder-console:tmux\npresent\n");
        assert_eq!(s.tmux, Some(vec![]));
        assert_eq!(
            parse("@@shellder-console:screen\npresent\nNo Sockets found in /run/screen/S-me.\n")
                .screen,
            Some(vec![])
        );
        assert_eq!(s.os, "FreeBSD 14.1");
        assert_eq!(s.uptime, None);
        assert_eq!(s.memory, None);
        assert!(s.disks.is_empty());
        let podman = parse("@@shellder-console:docker\npresent podman\n")
            .docker
            .unwrap();
        assert_eq!((podman.tool.as_str(), podman.list.len()), ("podman", 0));
        assert_eq!(parse("@@shellder-console:docker\n").docker, None);
        assert!(
            parse("@@shellder-console:vnc\n").vnc.is_empty(),
            "Screen Sharing off, or not a Mac"
        );
    }

    #[test]
    fn sizes_and_numbers() {
        assert_eq!(sized("2048.00M"), Some(2048 << 20));
        assert_eq!(sized("1.5G"), Some(3 << 29));
        assert_eq!(sized("x"), None);
        assert_eq!(floats("{ 1.2 3.4 5.6 }", 3), Some(vec![1.2, 3.4, 5.6]));
        assert_eq!(floats("1.2", 3), None);
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
        assert_eq!(
            pairs(r#"NAME="/dev/sdb1" LABEL="a\x22b\x20c""#),
            vec![
                ("NAME", "/dev/sdb1".to_string()),
                ("LABEL", "a\"b c".to_string())
            ]
        );
    }
}
