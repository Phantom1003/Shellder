use super::*;
use crate::core::ssh;

const LINUX: &str = include_str!("fixtures/linux.txt");
const MACOS: &str = include_str!("fixtures/macos.txt");
/// What the script printed in an Alpine container: BusyBox's `ps`,
/// `df`, `netstat` and `ash`, a tmux session, a user with a long name.
const ALPINE: &str = include_str!("fixtures/alpine.txt");
/// What it printed in a Debian container: `dash`, procps, iproute2,
/// tmux and screen sessions, the same long name.
const DEBIAN: &str = include_str!("fixtures/debian.txt");

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|p| p.to_string()).collect()
}

/// The Console switched to this machine reads it through the same
/// script, run here: what every host answers (uptime, memory, its
/// processes, this account among the logins or not) comes back.
#[test]
fn this_machine_is_read_like_a_host() {
    let snap = fetch(ssh::LOCAL, &strings(PARTS)).expect("a reading of this machine");
    assert!(snap.uptime.is_some(), "{snap:?}");
    assert!(snap.memory.is_some(), "{snap:?}");
    assert!(!snap.processes.is_empty(), "{snap:?}");
    assert!(snap.read_parts.is_empty(), "no tab's part asked for");
    // The tree too, rooted at the logins (none here: an empty tree
    // is still a reading, not an error).
    tree(ssh::LOCAL, &TreeRoot::Logins).expect("a tree of this machine");
}

/// The tabs' parts are read when asked for, and said to be: on a host
/// with neither tmux nor screen nor docker the fields are the same
/// either way, and `readParts` is what tells the page that None means
/// "not on the host" rather than "not looked at".
#[test]
fn the_tabs_parts_say_they_were_read() {
    let snap = fetch(ssh::LOCAL, &strings(&["ports", "jobs", "cpu"])).expect("a reading");
    assert_eq!(snap.read_parts, ["jobs", "ports"], "in TAB_PARTS' order");
    assert!(snap.cpu_ticks.is_some() || snap.cpu_percent.is_some());
    assert!(snap.memory.is_none(), "not asked for");
    assert!(snap.tmux.is_none() && snap.screen.is_none() && snap.vnc.is_empty());
    let all = fetch(ssh::LOCAL, &strings(TAB_PARTS)).expect("a reading");
    assert_eq!(all.read_parts, TAB_PARTS);
    assert!(
        !all.hostname.is_empty() && all.uptime.is_some(),
        "the head, always"
    );
}

/// A German locale that ssh passed on, as LC_ALL even: the numbers still
/// come with a point (BSD `ps` writes `0,7` under it, which would leave
/// the processes empty).
#[test]
fn a_locale_with_a_decimal_comma() {
    let marker = script::fresh_marker();
    let command = format!(
        "LANG=de_DE.UTF-8 LC_ALL=de_DE.UTF-8 {}",
        script::reading(&strings(&["processes"]), &marker)
    );
    let text = output(remote::shell(ssh::LOCAL, &command), &marker).expect("a reading");
    let snap = read(&Sections::new(&text, &marker));
    assert!(!snap.processes.is_empty(), "{text}");
    assert!(snap.load.is_some(), "{text}");
}

#[test]
fn guest_ticks_are_not_counted_twice() {
    // user 1000 (of which 300 guest), nice 0, system 500, idle 8000,
    // iowait 200, irq 0, softirq 10, steal 0, guest 300, guest_nice 0
    let s = parse("@@shellder-console:stat\ncpu  1000 0 500 8000 200 0 10 0 300 0\n");
    assert_eq!(
        s.cpu_ticks,
        Some(CpuTicks {
            busy: 1510,
            total: 9710
        })
    );
    assert_eq!(cpu_ticks("cpu 1 2"), None, "cut short");
    assert_eq!(cpu_ticks("cpu a b c d e"), None);
}

/// A directory name with a newline and a marker in it (a tmux pane's
/// path here) cannot start a section: the marker of the run has a nonce
/// the host did not know.
#[test]
fn a_directory_name_cannot_forge_a_section() {
    let marker = script::fresh_marker();
    let forged = format!(
        "{marker}hostname\nreal\n{marker}tmux\npresent\nw\t0\t0\t0\tsh\t1\tsh\t1\t/tmp/x\n\
         @@shellder-console:docker\npresent evil\nM\t/x\t/\n@@shellder-console:hostname\nfake\n\
         {marker}docker\n"
    );
    let snap = read(&Sections::new(&forged, &marker));
    assert_eq!(snap.hostname, "real");
    assert_eq!(snap.docker, None, "the real docker section is empty");
    let tmux = snap.tmux.expect("tmux");
    assert_eq!(
        tmux[0].path, "/tmp/x",
        "the rest of the name is not a window"
    );
    assert_eq!(tmux[0].windows.len(), 1);
    // Without the nonce, as before, the forged section would have won.
    assert!(parse(&forged).docker.is_some());
}

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
    assert_eq!(tmux[0].activity, Some(1699999000.0));
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
    assert_eq!(
        s.processes.len(),
        3,
        "the busiest by CPU, then by memory, python3 once"
    );
    assert_eq!(s.processes[0].pid, 4321);
    assert_eq!(s.processes[0].cpu, 250.0);
    assert_eq!(s.processes[0].command, "python3");
    assert_eq!((s.processes[2].pid, s.processes[2].memory), (5555, 30.0));
    assert_eq!(
        s.swap_files,
        Some(vec![
            SwapArea {
                path: "/dev/nvme0n1p3".into(),
                kind: "partition".into(),
                size: 4194300 * 1024,
                used: 388604 * 1024,
                temporary: false,
            },
            SwapArea {
                path: "/shellder.swap".into(),
                kind: "file".into(),
                size: 4194304 * 1024,
                used: 0,
                temporary: true,
            }
        ])
    );
    // The interfaces with an address, their counters beside: not
    // the loopback, not a veth without an address, no link-local
    // address.
    assert_eq!(
        s.network,
        vec![
            Interface {
                name: "eth0".into(),
                addresses: vec!["10.0.0.5/24".into(), "2001:db8::5/64".into()],
                received: Some(123456789),
                sent: Some(987654321),
            },
            Interface {
                name: "docker0".into(),
                addresses: vec!["172.17.0.1/16".into()],
                received: Some(0),
                sent: Some(4242),
            }
        ]
    );
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
        (
            slurm[1].name.as_str(),
            slurm[1].state.as_str(),
            slurm[1].time.as_str()
        ),
        ("eval | nightly", "PENDING", "0:00"),
        "a bar in the name: tabs between the fields"
    );
    assert_eq!(slurm[1].path, "/home/alice/eval");
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
    assert_eq!(s.swap_files, None, "macOS makes its own swap");
    // en0 and the VPN's utun3, with the prefix from the netmask; the
    // counters from the <Link#> rows, whose address column may be
    // empty; not the loopback, not awdl0 with its link-local only.
    assert_eq!(
        s.network,
        vec![
            Interface {
                name: "en0".into(),
                addresses: vec!["192.168.1.5/24".into()],
                received: Some(600000000),
                sent: Some(70000000),
            },
            Interface {
                name: "utun3".into(),
                addresses: vec!["10.8.0.2/24".into()],
                received: Some(300000),
                sent: Some(400000),
            }
        ]
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
fn reads_a_busybox_host() {
    let s = parse(ALPINE);
    assert!(s.os.starts_with("Alpine Linux"), "{}", s.os);
    assert!(s.kernel.starts_with("Linux"));
    assert!(s.uptime.is_some() && s.load.is_some() && s.cpus.is_some());
    assert!(s.cpu_ticks.is_some() && s.memory.is_some());
    assert!(!s.disks.is_empty(), "BusyBox's df -P");
    assert!(s.processes.is_empty(), "BusyBox's ps has no cpu column");
    assert_eq!(s.swap_files.as_ref().map(|v| v.len()), Some(2));
    // tmux without a UTF-8 locale, its tabs kept by `-u`.
    let tmux = s.tmux.expect("tmux present");
    assert_eq!(tmux.len(), 1);
    assert_eq!(
        (tmux[0].name.as_str(), tmux[0].path.as_str()),
        ("work", "/tmp/a dir")
    );
    assert_eq!(tmux[0].windows[0].command, "sh");
    assert_eq!(s.screen, None);
    assert_eq!(s.docker, None);
    assert_eq!(s.slurm, None);
    assert_eq!(s.pbs, None);
    assert_eq!(
        s.listeners,
        vec![Listener {
            port: 8022,
            address: "::".into(),
            process: String::new(),
            pid: None,
        }],
        "BusyBox's netstat, another user's program not named"
    );
}

#[test]
fn reads_a_dash_host() {
    let s = parse(DEBIAN);
    assert!(s.os.starts_with("Debian"), "{}", s.os);
    assert!(s.uptime.is_some() && s.load.is_some() && s.cpus.is_some());
    assert!(s.cpu_ticks.is_some() && s.memory.is_some());
    assert!(!s.disks.is_empty());
    assert!(s.processes.iter().any(|p| p.command == "tmux: server"));
    assert!(
        s.processes
            .iter()
            .all(|p| p.user == "root" || p.user == "averyve+"),
        "a long name cut by procps"
    );
    // A UTF-8 session name without a UTF-8 locale, sessions by name.
    let tmux = s.tmux.expect("tmux present");
    let names: Vec<(&str, usize, &str)> = tmux
        .iter()
        .map(|t| (t.name.as_str(), t.windows.len(), t.path.as_str()))
        .collect();
    assert_eq!(names, vec![("café", 1, "/tmp/b"), ("main", 2, "/tmp")]);
    assert!(tmux[1].windows[1].active && !tmux[1].windows[0].active);
    let screen = s.screen.expect("screen present");
    assert_eq!(screen.len(), 1);
    assert_eq!(
        (
            screen[0].id.as_str(),
            screen[0].path.as_str(),
            screen[0].attached
        ),
        ("553.notes", "/tmp", false)
    );
    assert_eq!(
        s.listeners,
        vec![Listener {
            port: 8022,
            address: "0.0.0.0".into(),
            process: String::new(),
            pid: None,
        }],
        "ss, without -H: the header passed over"
    );
    assert_eq!(s.users, Some(0), "who read, nobody logged in");
}

/// The script itself under BusyBox's `ash` (Alpine) and `dash` (Debian)
/// as the images come, in Docker: it runs to its last section, and the
/// head and the gauges are read. Ignored, since it needs Docker and
/// pulls the images: `cargo test --lib monitor -- --ignored`.
#[test]
#[ignore]
fn the_script_runs_under_busybox_and_dash() {
    let every = strings(&[PARTS, TAB_PARTS].concat());
    for image in ["alpine:latest", "debian:stable-slim"] {
        let marker = script::fresh_marker();
        let out = std::process::Command::new("docker")
            .args(["run", "--rm", image, "sh", "-c"])
            .arg(script::reading_script(&every, &marker))
            .output()
            .expect("docker");
        let text = String::from_utf8_lossy(&out.stdout);
        let s = Sections::new(&text, &marker);
        for name in [
            "hostname", "uname", "os", "now", "uptime", "load", "ncpu", "stat", "topcpu", "mem",
            "swap", "swaps", "df", "blk", "net", "addr", "gpu", "ps", "who", "tmux", "screen",
            "vnc", "slurm", "pbs", "docker", "ports",
        ] {
            assert!(s.has(name), "{image}: no {name} in {text}");
        }
        let snap = read(&s);
        assert!(
            !snap.os.is_empty() && !snap.hostname.is_empty(),
            "{image}: {snap:?}"
        );
        assert!(
            snap.uptime.is_some() && snap.load.is_some(),
            "{image}: {snap:?}"
        );
        assert!(
            snap.memory.is_some() && snap.cpu_ticks.is_some(),
            "{image}: {snap:?}"
        );
        assert!(!snap.disks.is_empty(), "{image}: {snap:?}");
    }
}

#[test]
fn an_os_without_a_name_and_parts_not_read() {
    let s = parse("@@shellder-console:uname\nFreeBSD 14.1\n");
    assert_eq!(s.os, "FreeBSD 14.1");
    assert_eq!(s.uptime, None);
    assert_eq!(s.memory, None);
    assert!(s.disks.is_empty());
    assert_eq!(s.users, None, "the logins were not read");
    assert_eq!(s.swap_files, None);
    assert_eq!(s.tmux, None);
    assert!(s.read_parts.is_empty());
}

#[test]
fn sizes_and_numbers() {
    assert_eq!(sized("2048.00M"), Some(2048 << 20));
    assert_eq!(sized("1.5G"), Some(3 << 29));
    assert_eq!(sized("x"), None);
    assert_eq!(floats("{ 1.2 3.4 5.6 }", 3), Some(vec![1.2, 3.4, 5.6]));
    assert_eq!(floats("1.2", 3), None);
}
