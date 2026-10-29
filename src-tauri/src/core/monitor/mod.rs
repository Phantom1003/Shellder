//! What a Console window shows about a host: one shell script, run
//! through the master when one is up and through the host's session
//! otherwise, whose output is cut into sections and read here. Every
//! section is best effort: a host without `/proc` (macOS, the BSDs), without
//! tmux, screen or `nvidia-smi` leaves its part empty, and the window shows
//! what there is.
//!
//! The scripts are shell files beside this one, joined into one line at
//! load time (`script`): the reading, the process tree (`tree`) and what
//! the Status tab does on the host (`actions`). The reading is cut up by
//! subject: `disks`, `network`, `sessions` (tmux, screen, VNC, containers,
//! batch jobs), and here the head, CPU, memory, GPUs, processes and
//! logins; `types` is what the page gets.

mod actions;
mod disks;
mod network;
mod script;
mod sessions;
mod tree;
mod types;

pub use actions::{disk_command, signal_command, swap_command, wants_password, SIGNALS, SWAP_MARK};
pub use script::{command, PARTS, TAB_PARTS};
pub use tree::{parse_tree, tree, tree_command, Tree, TreeRoot, TreeRow};
pub use types::*;

use super::config::READING_DEADLINE;
use super::remotefs::remote;
use super::ssh;

/// Run the script on `host`, reading the `parts` the page asked for
/// (`PARTS`, `TAB_PARTS`), and read what it said. A reading that has not
/// finished within `READING_DEADLINE` (a `df` stuck on a mount whose
/// server is gone) is given up, so the column can try again.
pub fn fetch(host: &str, parts: &[String]) -> Result<Snapshot, String> {
    let marker = script::fresh_marker();
    let out = remote::shell_within(host, &script::reading(parts, &marker), READING_DEADLINE);
    let text = output(out, &marker)?;
    let mut snapshot = read(&Sections::new(&text, &marker));
    snapshot.read_parts = TAB_PARTS
        .iter()
        .filter(|t| parts.iter().any(|p| p == *t))
        .map(|t| t.to_string())
        .collect();
    Ok(snapshot)
}

/// What a script printed, or why it printed nothing of its own: without
/// a single section it never ran (no `sh`, a login refused), and what
/// the host said instead is the error.
fn output(out: ssh::Output, marker: &str) -> Result<String, String> {
    if out.stdout.contains(marker) {
        return Ok(out.stdout);
    }
    let why = out.text();
    Err(if why.is_empty() {
        format!("exited with status {}", out.status)
    } else {
        why
    })
}

/// A script's output cut at its markers, section name to the lines under
/// it, blank lines left out. A line starts a section only when it starts
/// with the run's marker: what a host prints of its own (a directory
/// name with a newline in it) cannot start one.
struct Sections<'a>(Vec<(&'a str, Vec<&'a str>)>);

impl<'a> Sections<'a> {
    fn new(text: &'a str, marker: &str) -> Sections<'a> {
        let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
        for line in text.lines() {
            if let Some(name) = line.strip_prefix(marker) {
                out.push((name.trim(), Vec::new()));
            } else if let Some(last) = out.last_mut() {
                if !line.trim().is_empty() {
                    last.1.push(line);
                }
            }
        }
        Sections(out)
    }

    /// The lines of the first section of that name, none without one.
    fn get(&self, name: &str) -> &[&'a str] {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, l)| l.as_slice())
            .unwrap_or(&[])
    }

    /// Whether the script got to that section at all.
    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|(n, _)| *n == name)
    }

    /// The first line of a section, trimmed; empty without one.
    fn first(&self, name: &str) -> String {
        self.get(name)
            .first()
            .map(|l| l.trim().to_string())
            .unwrap_or_default()
    }
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

/// `/proc/swaps`: a header, then `path type size used priority`, the
/// sizes in kB. None without the header: not a Linux host.
fn swap_areas(lines: &[&str]) -> Option<Vec<SwapArea>> {
    let mut it = lines.iter();
    if !it.next()?.starts_with("Filename") {
        return None;
    }
    Some(
        it.filter_map(|l| {
            let f = fields(l);
            if f.len() < 4 {
                return None;
            }
            let path = f[0].to_string();
            Some(SwapArea {
                temporary: path.rsplit('/').next().unwrap_or("").contains(SWAP_MARK),
                path,
                kind: f[1].to_string(),
                size: f[2].parse::<u64>().ok()? * 1024,
                used: f[3].parse::<u64>().ok()? * 1024,
            })
        })
        .collect(),
    )
}

/// `/proc/stat`'s first line, `cpu` and the ticks by kind.
fn cpu_ticks(line: &str) -> Option<CpuTicks> {
    let f = fields(line);
    if f.first() != Some(&"cpu") {
        return None;
    }
    let v: Vec<u64> = f[1..].iter().map_while(|w| w.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some(CpuTicks {
        busy: total.saturating_sub(idle),
        total,
    })
}

/// "CPU usage: 3.12% user, 5.2% sys, 91.6% idle" (macOS's `top`).
fn cpu_percent(line: &str) -> Option<f64> {
    let i = line.find("idle")?;
    let before = line[..i].trim_end();
    let idle: f64 = before
        .rsplit(' ')
        .next()?
        .trim_end_matches('%')
        .parse()
        .ok()?;
    Some((100.0 - idle).clamp(0.0, 100.0))
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

/// `loginctl show-session`: a block of `Key=Value` lines a session, from
/// its `Id`. Only a person's sessions (class `user`, root's
/// `user-early`, their `-light` kinds), not a greeter's or a user's
/// service manager's. When as `who` writes it, `2026-10-29 10:47`, out
/// of `Thu 2026-10-29 10:47:10 +08`; the oldest first.
fn logind_sessions(lines: &[&str]) -> Vec<Login> {
    let mut blocks: Vec<Vec<(&str, &str)>> = Vec::new();
    for l in lines {
        let Some((key, value)) = l.split_once('=') else {
            continue;
        };
        if key == "Id" || blocks.is_empty() {
            blocks.push(Vec::new());
        }
        if let Some(b) = blocks.last_mut() {
            b.push((key, value.trim()));
        }
    }
    let mut out: Vec<Login> = blocks
        .iter()
        .filter_map(|b| {
            let get = |k: &str| b.iter().find(|(key, _)| *key == k).map(|(_, v)| *v);
            let class = get("Class")?;
            if !class.starts_with("user") || class.contains("incomplete") {
                return None;
            }
            let when = get("Timestamp").unwrap_or("");
            let f = fields(when);
            let since = match f.iter().position(|w| w.as_bytes().get(4) == Some(&b'-')) {
                Some(i) if i + 1 < f.len() => {
                    format!("{} {}", f[i], f[i + 1].get(..5).unwrap_or(f[i + 1]))
                }
                _ => when.to_string(),
            };
            Some(Login {
                user: get("Name")?.to_string(),
                tty: get("TTY").unwrap_or("").to_string(),
                since,
                from: get("RemoteHost")
                    .filter(|h| !h.is_empty())
                    .map(str::to_string),
            })
        })
        .collect();
    out.sort_by(|a, b| a.since.cmp(&b.since));
    out
}

/// The busiest by CPU, then the busiest by memory; a process in both
/// lists once.
fn processes(lines: &[&str]) -> Vec<Process> {
    let mut out: Vec<Process> = Vec::new();
    for l in lines {
        let f = fields(l);
        if f.len() < 5 {
            continue;
        }
        let Ok(pid) = f[0].parse::<u32>() else {
            continue;
        };
        if out.iter().any(|p| p.pid == pid) {
            continue;
        }
        let (Ok(cpu), Ok(memory)) = (f[2].parse(), f[3].parse()) else {
            continue;
        };
        out.push(Process {
            pid,
            user: f[1].to_string(),
            cpu,
            memory,
            command: f[4..].join(" "),
        });
    }
    out
}

/// Read the script's output, as `command` marks it.
pub fn parse(text: &str) -> Snapshot {
    read(&Sections::new(text, &script::marker("")))
}

fn read(s: &Sections) -> Snapshot {
    let kernel = s.first("uname");
    let os = {
        let o = s.first("os");
        if o.is_empty() {
            kernel.clone()
        } else {
            o
        }
    };
    let now: Option<f64> = s.get("now").first().and_then(|l| num(l));
    let uptime = s.get("uptime").first().and_then(|l| {
        if let Some(v) = floats(l, 1).filter(|_| !l.contains("sec")) {
            return Some(v[0]);
        }
        // `{ sec = 1700000000, usec = 0 } Tue Nov 14 …`
        let i = l.find("sec =")?;
        let boot: f64 = num(fields(&l[i + 5..]).first()?.trim_end_matches(','))?;
        Some((now? - boot).max(0.0))
    });
    let load = s
        .get("load")
        .first()
        .and_then(|l| floats(l, 3))
        .map(|v| [v[0], v[1], v[2]]);
    let login_sessions = s.has("logind");
    let logins = if login_sessions {
        logind_sessions(s.get("logind"))
    } else {
        logins(s.get("who"))
    };
    // Only when the logins were read at all: with them left out of the
    // readings the head says nothing about them.
    let users = (s.has("who") || login_sessions).then_some(logins.len() as u32);
    let (disks, volumes) = disks::read(s.get("df"), s.get("blk"));
    let (memory, swap_linux) = memory(s.get("mem"));
    let swap = swap_linux.or_else(|| s.get("swap").first().and_then(|l| swap_usage(l)));
    Snapshot {
        hostname: s.first("hostname"),
        os,
        kernel,
        uptime,
        load,
        cpus: s.get("ncpu").first().and_then(|l| num(l)),
        cpu_ticks: s.get("stat").first().and_then(|l| cpu_ticks(l)),
        cpu_percent: s.get("topcpu").last().and_then(|l| cpu_percent(l)),
        memory,
        swap,
        swap_files: swap_areas(s.get("swaps")),
        disks,
        volumes,
        network: network::network(s.get("net"), s.get("addr")),
        gpus: gpus(s.get("gpu")),
        tmux: sessions::tmux(s.get("tmux")),
        screen: sessions::screen(s.get("screen"), s.get("screencwd")),
        docker: sessions::containers(s.get("docker")),
        slurm: sessions::slurm(s.get("slurm")),
        pbs: sessions::pbs(s.get("pbs")),
        vnc: sessions::vnc(s.get("vnc")),
        processes: processes(s.get("ps")),
        users,
        login_sessions,
        logins,
        listeners: network::listeners(s.get("ports")),
        read_parts: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
