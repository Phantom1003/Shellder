//! What a Console window shows about a host: one shell script, run
//! through the master when one is up and through the host's session
//! otherwise, whose output is cut into sections and read here. Every
//! section is best effort: a host without `/proc` (macOS, the BSDs), without
//! tmux or without `nvidia-smi` leaves its part empty, and the window shows
//! what there is.

use serde::Serialize;

use super::remotefs::remote;

/// Where a section starts in the script's output.
const MARK: &str = "@@shellder-console:";

/// The script, one line of POSIX sh so that it means the same under any
/// login shell: it is wrapped in `sh -c '…'`, which is why it carries no
/// single quote. `S name` starts a section; what a command cannot say is
/// left empty. Linux reads `/proc`, macOS its `sysctl`s, `vm_stat` and
/// `top` (which costs a second, the sample it takes); `df -P` and `ps -eo`
/// are what everyone has (BusyBox's `ps` lacks the cpu column, so that
/// section is empty there).
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
    "S gpu; command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null; ",
    "S tmux; command -v tmux >/dev/null 2>&1 && { echo present; tmux list-sessions -F \"#{session_name}\t#{session_windows}\t#{session_created}\t#{session_attached}\t#{session_activity}\" 2>/dev/null; }; ",
    "S windows; tmux list-windows -a -F \"#{session_name}\t#{window_index}\t#{window_name}\t#{window_active}\t#{pane_current_command}\t#{window_panes}\" 2>/dev/null; ",
    "S ps; ps -eo pid,user,pcpu,pmem,comm 2>/dev/null | tail -n +2 | sort -k3 -rn 2>/dev/null | head -n 15; ",
    "S users; who 2>/dev/null | wc -l; ",
    "true",
);

/// The script as the remote login shell gets it.
pub fn command() -> String {
    format!("sh -c '{SCRIPT}'")
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
    /// What runs in the active pane.
    pub command: String,
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
    pub gpus: Vec<Gpu>,
    /// None: no tmux on the host. Some(empty): tmux, but no session.
    pub tmux: Option<Vec<TmuxSession>>,
    pub processes: Vec<Process>,
    pub users: Option<u32>,
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

fn disks(lines: &[&str]) -> Vec<Disk> {
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
            storage(f[0], &mount, total).then(|| Disk {
                filesystem: f[0].to_string(),
                mount,
                total: total * 1024,
                used: used * 1024,
            })
        })
        .collect()
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
            })
        })
        .collect();
    for l in windows {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 6 {
            continue;
        }
        if let Some(s) = out.iter_mut().find(|s| s.name == f[0]) {
            s.windows.push(TmuxWindow {
                index: num(f[1]).unwrap_or(0),
                name: f[2].to_string(),
                active: f[3] == "1",
                command: f[4].to_string(),
                panes: num(f[5]).unwrap_or(1),
            });
        }
    }
    Some(out)
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
        disks: disks(get("df")),
        gpus: gpus(get("gpu")),
        tmux: tmux(get("tmux"), get("windows")),
        processes: processes(get("ps")),
        users: get("users").first().and_then(|l| num(l)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_fits_in_single_quotes() {
        assert!(
            !SCRIPT.contains('\''),
            "a single quote would end the sh -c argument"
        );
        assert!(
            !SCRIPT.contains('\n'),
            "a csh login shell would choke on a newline"
        );
        assert!(command().starts_with("sh -c '"));
    }

    const LINUX: &str = "
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
@@shellder-console:gpu
0, NVIDIA A100-SXM4-40GB, 87, 30000, 40960
1, NVIDIA A100-SXM4-40GB, 0, 0, 40960
@@shellder-console:tmux
present
main\t3\t1699990000\t1\t1699999000
train\t1\t1699995000\t0\t1699999500
@@shellder-console:windows
main\t0\tzsh\t0\tzsh\t1
main\t1\tvim\t1\tvim\t2
main\t2\thtop\t0\thtop\t1
train\t0\tpython\t1\tpython3\t1
@@shellder-console:ps
 4321 alice    250.0  12.3 python3
    1 root       0.0   0.1 systemd
@@shellder-console:users
3
";

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
        assert_eq!(mounts, vec!["/", "/mnt/data disk"]);
        assert_eq!(s.disks[0].used, 500000000 * 1024);
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
        assert_eq!(s.processes.len(), 2);
        assert_eq!(s.processes[0].pid, 4321);
        assert_eq!(s.processes[0].cpu, 250.0);
        assert_eq!(s.processes[0].command, "python3");
        assert_eq!(s.users, Some(3));
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
@@shellder-console:gpu
@@shellder-console:tmux
@@shellder-console:windows
@@shellder-console:ps
  500 me        12.5   3.0 WindowServer
@@shellder-console:users
1
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
        assert_eq!(mounts, vec!["/", "/System/Volumes/Data"]);
        assert!(s.gpus.is_empty());
        assert_eq!(s.tmux, None, "no tmux on the host");
        assert_eq!(s.processes.len(), 1);
    }

    #[test]
    fn tmux_without_sessions_and_an_os_without_a_name() {
        let s = parse("@@shellder-console:uname\nFreeBSD 14.1\n@@shellder-console:tmux\npresent\n");
        assert_eq!(s.tmux, Some(vec![]));
        assert_eq!(s.os, "FreeBSD 14.1");
        assert_eq!(s.uptime, None);
        assert_eq!(s.memory, None);
        assert!(s.disks.is_empty());
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
    }
}
