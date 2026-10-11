//! Port forwards on a host's master, made and taken back at the user's
//! request from the Console's Ports tab: `ssh -O forward` and `ssh -O
//! cancel`, so they need a master with a socket. A forward lives as long
//! as the master; the list kept here says which are still up by
//! remembering the master's pid they were made on.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::log;
use super::ssh;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// `-L`: a port here reaches a port on the host.
    Local,
    /// `-R`: a port on the host reaches a port here.
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
    pub direction: Direction,
    /// The port on this machine.
    pub local: u16,
    /// The port on the host.
    pub remote: u16,
}

impl Forward {
    /// What ssh is given after `-L` or `-R`: both ends on loopback.
    fn spec(&self) -> String {
        match self.direction {
            Direction::Local => format!("127.0.0.1:{}:127.0.0.1:{}", self.local, self.remote),
            Direction::Remote => format!("127.0.0.1:{}:127.0.0.1:{}", self.remote, self.local),
        }
    }

    fn flag(&self) -> &'static str {
        match self.direction {
            Direction::Local => "-L",
            Direction::Remote => "-R",
        }
    }
}

/// The forwards made: the host, the pid of the master they were made on,
/// and the forward.
static ACTIVE: Mutex<Vec<(String, u32, Forward)>> = Mutex::new(Vec::new());

fn free_local_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("no free port here: {e}"))
}

fn failure(what: &str, r: &ssh::Output) -> String {
    let why = r.text();
    if why.is_empty() {
        format!("{what} exited with status {}", r.status)
    } else {
        why
    }
}

/// Make a forward on the host's master. A port left out is picked: here,
/// a free one; on the host, one sshd allocates (`-R 0:…`, which it reports
/// as "Allocated port N").
pub fn establish(
    host: &str,
    direction: Direction,
    local: Option<u16>,
    remote: Option<u16>,
) -> Result<Forward, String> {
    if !ssh::master_reusable(host) {
        return Err(
            "A forward goes through the master, so the host has to be connected with a socket."
                .into(),
        );
    }
    let pid = ssh::master_pid(host).ok_or("the master is gone")?;
    let mut f = match direction {
        Direction::Local => Forward {
            direction,
            local: match local {
                Some(p) => p,
                None => free_local_port()?,
            },
            remote: remote.ok_or("A local forward needs the port on the host.")?,
        },
        Direction::Remote => Forward {
            direction,
            local: local.ok_or("A remote forward needs the port on this machine.")?,
            remote: remote.unwrap_or(0),
        },
    };
    let r = ssh::run(&["-O", "forward", f.flag(), &f.spec(), host]);
    if r.status != 0 {
        return Err(failure("ssh -O forward", &r));
    }
    if f.remote == 0 {
        // "Allocated port 45678 for remote forward to 127.0.0.1:8080"
        let text = format!("{}\n{}", r.stdout, r.stderr);
        f.remote = text
            .split("Allocated port ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse().ok())
            .ok_or("sshd did not say which port it allocated")?;
    }
    log::info(format!(
        "{host}: forward {} {} on master {pid}",
        f.flag(),
        f.spec()
    ));
    let mut a = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    a.retain(|(h, _, x)| !(h == host && *x == f));
    a.push((host.to_string(), pid, f));
    Ok(f)
}

/// Take a forward back (`ssh -O cancel`). One the master no longer has
/// (it was restarted) is just forgotten.
pub fn cancel(host: &str, f: Forward) -> Result<(), String> {
    let live = active(host).contains(&f);
    {
        let mut a = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        a.retain(|(h, _, x)| !(h == host && *x == f));
    }
    if !live {
        return Ok(());
    }
    let r = ssh::run(&["-O", "cancel", f.flag(), &f.spec(), host]);
    if r.status != 0 {
        return Err(failure("ssh -O cancel", &r));
    }
    log::info(format!(
        "{host}: forward {} {} cancelled",
        f.flag(),
        f.spec()
    ));
    Ok(())
}

/// The forwards still up on the host's master: those made on the master
/// that is running now. The others are dropped from the list.
pub fn active(host: &str) -> Vec<Forward> {
    let pid = ssh::master_pid(host);
    let mut a = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    a.retain(|(h, p, _)| h != host || Some(*p) == pid);
    a.iter()
        .filter(|(h, _, _)| h == host)
        .map(|(_, _, f)| *f)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        let l = Forward {
            direction: Direction::Local,
            local: 6006,
            remote: 6007,
        };
        assert_eq!(
            (l.flag(), l.spec().as_str()),
            ("-L", "127.0.0.1:6006:127.0.0.1:6007")
        );
        let r = Forward {
            direction: Direction::Remote,
            local: 8080,
            remote: 9090,
        };
        assert_eq!(
            (r.flag(), r.spec().as_str()),
            ("-R", "127.0.0.1:9090:127.0.0.1:8080")
        );
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"direction":"remote","local":8080,"remote":9090}"#
        );
    }
}
