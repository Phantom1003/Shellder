//! What the Console's column lists beside the Status tab: tmux and screen
//! sessions and VNC servers (`sessions`), the account's batch jobs
//! (`jobs`), and the containers (`containers`).

use std::collections::HashMap;

use super::types::{
    Container, Containers, PbsJob, ScreenSession, SlurmJob, TmuxSession, TmuxWindow, VncSession,
};
use super::{fields, num};

/// `tmux list-windows -a` after a line saying tmux is there: a line per
/// window, tabs between its session's name, attached clients and last
/// activity, then the window's index, name, whether it is the active one,
/// what runs in its active pane, its panes, and the active pane's
/// directory (the rest of the line, a tab in it included). Every session
/// has a window, so the sessions come from the same lines, in their
/// order.
pub(super) fn tmux(lines: &[&str]) -> Option<Vec<TmuxSession>> {
    let mut it = lines.iter();
    if it.next().map(|l| l.trim()) != Some("present") {
        return None;
    }
    let mut out: Vec<TmuxSession> = Vec::new();
    for l in it {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        let path = f.get(8..).unwrap_or_default().join("\t");
        let active = f[5] == "1";
        let i = match out.iter().position(|s| s.name == f[0]) {
            Some(i) => i,
            None => {
                out.push(TmuxSession {
                    name: f[0].to_string(),
                    activity: num(f[2]),
                    attached: num(f[1]).unwrap_or(0),
                    windows: Vec::new(),
                    path: String::new(),
                });
                out.len() - 1
            }
        };
        if active {
            out[i].path = path.clone();
        }
        out[i].windows.push(TmuxWindow {
            index: num(f[3]).unwrap_or(0),
            name: f[4].to_string(),
            active,
            command: f[6].to_string(),
            path,
            panes: num(f[7]).unwrap_or(1),
        });
    }
    Some(out)
}

/// `screen -ls`: "There are screens on:", then one tab-indented line per
/// session, `pid.name  (date)  (Attached|Detached|Multi|Dead ???)`.
pub(super) fn screen(lines: &[&str], cwds: &[&str]) -> Option<Vec<ScreenSession>> {
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
pub(super) fn containers(lines: &[&str]) -> Option<Containers> {
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

/// `squeue -h -o "%i\t%j\t%T\t%M\t%D\t%R\t%P\t%Z"`: tabs between, since
/// a job's name may hold a `|` or a space; the directory is the rest.
pub(super) fn slurm(lines: &[&str]) -> Option<Vec<SlurmJob>> {
    let mut it = lines.iter();
    if it.next().map(|l| l.trim()) != Some("present") {
        return None;
    }
    Some(
        it.filter_map(|l| {
            let f: Vec<&str> = l.split('\t').map(str::trim).collect();
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
                path: f.get(7..).unwrap_or_default().join("\t"),
            })
        })
        .collect(),
    )
}

/// `qstat -f`: `Job Id: N` then `    key = value` lines, a long value
/// going on in lines that start with a tab.
pub(super) fn pbs(lines: &[&str]) -> Option<Vec<PbsJob>> {
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
pub(super) fn vnc(lines: &[&str]) -> Vec<VncSession> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_without_anything_running() {
        assert_eq!(tmux(&["present"]), Some(vec![]));
        assert_eq!(tmux(&[]), None);
        assert_eq!(
            screen(&["present", "No Sockets found in /run/screen/S-me."], &[]),
            Some(vec![])
        );
        let podman = containers(&["present podman"]).unwrap();
        assert_eq!((podman.tool.as_str(), podman.list.len()), ("podman", 0));
        assert_eq!(containers(&[]), None);
        assert!(vnc(&[]).is_empty(), "Screen Sharing off, or not a Mac");
    }

    #[test]
    fn a_tab_or_a_bar_in_a_name() {
        let jobs = slurm(&[
            "present",
            "42\ta|b c\tRUNNING\t1:00\t1\tnode01\tgpu\t/home/me/x\ty",
            "43\tshort\tPENDING\t0:00\t1\t(Priority)\tgpu",
            "44\tcut",
        ])
        .unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(
            (jobs[0].name.as_str(), jobs[0].path.as_str()),
            ("a|b c", "/home/me/x\ty")
        );
        assert_eq!(
            (jobs[1].reason.as_str(), jobs[1].path.as_str()),
            ("(Priority)", "")
        );
        let t = tmux(&[
            "present",
            "work\t0\t1700000000\t0\tsh\t1\tsh\t1\t/tmp/a\tb",
            "work\t0\t1700000000\t1\tvim\t0\tvim\t2",
        ])
        .unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].path, "/tmp/a\tb");
        assert_eq!(t[0].windows.len(), 2);
        assert_eq!(
            (t[0].windows[1].panes, t[0].windows[1].path.as_str()),
            (2, "")
        );
    }
}
