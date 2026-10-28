//! The Network card (interfaces, their addresses and traffic) and the
//! ports something on the host listens on, for the Ports tab.

use super::fields;
use super::types::{Interface, Listener};

/// `addr:port` as ss, netstat and lsof write it (`0.0.0.0:22`, `[::]:22`,
/// `*:22`, `:::22`), or as the BSDs' netstat does, a point before the
/// port (`*.22`, `127.0.0.1.8080`, `::1.8080`).
fn addr_port(word: &str) -> Option<(String, u16)> {
    let split = |sep: char| {
        let (addr, port) = word.rsplit_once(sep)?;
        Some((addr, port.parse::<u16>().ok()?))
    };
    let (addr, port) = split(':').or_else(|| split('.'))?;
    Some((addr.trim_matches(['[', ']']).to_string(), port))
}

/// The ports listened on, one row per port (the first binding of it), by
/// port: `ss -ltnp` lines (`LISTEN 0 128 0.0.0.0:22 0.0.0.0:*
/// users:(("sshd",pid=1,fd=3))`), `netstat -ltnp` lines (`tcp 0 0
/// 0.0.0.0:22 0.0.0.0:* LISTEN 1/sshd`, the program only where the host
/// says), or, after a first line saying so, `lsof -nP -iTCP
/// -sTCP:LISTEN` (`sshd 1 root 3u IPv4 … TCP *:22 (LISTEN)`). Headers and
/// anything else are passed over.
pub(super) fn listeners(lines: &[&str]) -> Vec<Listener> {
    let lsof = lines.first().is_some_and(|l| l.trim() == "lsof");
    let mut out: Vec<Listener> = Vec::new();
    for l in lines.iter().skip(if lsof { 1 } else { 0 }) {
        let f = fields(l);
        let got = if lsof {
            let Some(name) = f.get(8) else {
                continue;
            };
            addr_port(name).map(|(address, port)| Listener {
                port,
                address,
                process: f[0].to_string(),
                pid: f[1].parse().ok(),
            })
        } else if f.first() == Some(&"LISTEN") {
            let Some(local) = f.get(3) else {
                continue;
            };
            let rest = f.get(5..).unwrap_or_default().join(" ");
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
            addr_port(local).map(|(address, port)| Listener {
                port,
                address,
                process,
                pid,
            })
        } else if f.first().is_some_and(|p| p.starts_with("tcp")) && f.get(5) == Some(&"LISTEN") {
            let (pid, process) = match f.get(6).and_then(|w| w.split_once('/')) {
                Some((pid, name)) => (pid.parse().ok(), name.to_string()),
                None => (None, String::new()),
            };
            addr_port(f[3]).map(|(address, port)| Listener {
                port,
                address,
                process,
                pid,
            })
        } else {
            continue;
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

/// The traffic counters: `/proc/net/dev` (`name: rx-bytes … tx-bytes …`,
/// the transmit bytes ninth), or `netstat -ibn` (BSD, macOS), where the
/// `<Link#n>` row of an interface carries them and the address column
/// may be empty, so the bytes are counted from the end of the row:
/// `… Ibytes Opkts Oerrs Obytes Coll`.
fn counters(lines: &[&str]) -> Vec<(String, u64, u64)> {
    let mut out = Vec::new();
    for l in lines {
        // `/proc/net/dev`: the name, a colon, the counters. A netstat
        // row has colons too, in a MAC address after the name.
        if let Some((name, rest)) = l.split_once(':') {
            let name = name.trim();
            let f = fields(rest);
            if !name.contains(char::is_whitespace) && f.len() >= 9 {
                if let (Ok(rx), Ok(tx)) = (f[0].parse(), f[8].parse()) {
                    out.push((name.to_string(), rx, tx));
                }
                continue;
            }
        }
        let f = fields(l);
        if f.len() < 9 || !f.iter().any(|w| w.starts_with("<Link#")) {
            continue;
        }
        if let (Ok(rx), Ok(tx)) = (f[f.len() - 5].parse(), f[f.len() - 2].parse()) {
            out.push((f[0].to_string(), rx, tx));
        }
    }
    out
}

/// The prefix length of a netmask as ifconfig writes it: `0xffffff00`
/// or `255.255.255.0`.
fn mask_bits(mask: &str) -> Option<u32> {
    if let Some(hex) = mask.strip_prefix("0x") {
        return Some(u32::from_str_radix(hex, 16).ok()?.count_ones());
    }
    let mut bits = 0;
    for part in mask.split('.') {
        bits += part.parse::<u8>().ok()?.count_ones();
    }
    Some(bits)
}

/// An address worth showing: not loopback, not link-local.
fn routable(address: &str) -> bool {
    let a = address.to_ascii_lowercase();
    !(a.starts_with("127.") || a.starts_with("fe80:") || a == "::1" || a.starts_with("::1/"))
}

/// The addresses by interface: `ip -o addr` (`2: eth0    inet
/// 10.0.0.5/24 brd … scope global …`), or `ifconfig` (an unindented
/// `name: flags=…` or `name      Link encap:…` line, then `inet
/// 10.0.0.5 netmask 0xffffff00 …` / `inet addr:10.0.0.5  Mask:…` /
/// `inet6 fe80::1%en0 prefixlen 64 …` under it). In the order the host
/// listed them.
fn addresses(lines: &[&str]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |name: &str, address: String| {
        if !routable(&address) {
            return;
        }
        match out.iter_mut().find(|(n, _)| n == name) {
            Some((_, list)) => list.push(address),
            None => out.push((name.to_string(), vec![address])),
        }
    };
    let mut current = String::new();
    for l in lines {
        let f = fields(l);
        if f.len() >= 4 && f[0].ends_with(':') && f[0][..f[0].len() - 1].parse::<u32>().is_ok() {
            // ip -o addr
            if f[2] == "inet" || f[2] == "inet6" {
                let scope = f.windows(2).find(|w| w[0] == "scope").map(|w| w[1]);
                if !matches!(scope, Some("host") | Some("link")) {
                    add(f[1], f[3].to_string());
                }
            }
            continue;
        }
        if !l.starts_with([' ', '\t']) {
            // ifconfig: the interface's header.
            current = f
                .first()
                .map(|w| w.trim_end_matches(':').to_string())
                .unwrap_or_default();
            continue;
        }
        if current.is_empty() || f.len() < 2 || (f[0] != "inet" && f[0] != "inet6") {
            continue;
        }
        // `inet addr:10.1.2.3` and `inet6 addr: fe80::1/64` (old net-tools).
        let address = match f[1].strip_prefix("addr:") {
            Some("") => f.get(2).copied().unwrap_or(""),
            Some(a) => a,
            None => f[1],
        };
        let address = address.split('%').next().unwrap_or(address);
        if address.is_empty() {
            continue;
        }
        if address.contains('/') {
            add(&current, address.to_string());
            continue;
        }
        let prefix = f.iter().enumerate().find_map(|(i, w)| match *w {
            "prefixlen" => f.get(i + 1)?.parse::<u32>().ok(),
            "netmask" => mask_bits(f.get(i + 1)?),
            _ => w.strip_prefix("Mask:").and_then(mask_bits),
        });
        add(
            &current,
            match prefix {
                Some(p) => format!("{address}/{p}"),
                None => address.to_string(),
            },
        );
    }
    out
}

/// The interfaces with an address, their counters beside; where the
/// host could name no address (neither `ip` nor `ifconfig`), those with
/// traffic instead of the loopback.
pub(super) fn network(counted: &[&str], addressed: &[&str]) -> Vec<Interface> {
    let counters = counters(counted);
    let addresses = addresses(addressed);
    if addresses.is_empty() {
        return counters
            .into_iter()
            .filter(|(name, rx, tx)| !name.starts_with("lo") && rx + tx > 0)
            .map(|(name, rx, tx)| Interface {
                name,
                addresses: Vec::new(),
                received: Some(rx),
                sent: Some(tx),
            })
            .collect();
    }
    addresses
        .into_iter()
        .map(|(name, addresses)| {
            let c = counters.iter().find(|(n, _, _)| *n == name);
            Interface {
                name,
                addresses,
                received: c.map(|c| c.1),
                sent: c.map(|c| c.2),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interfaces_the_old_way_and_without_addresses() {
        // net-tools' old ifconfig: `inet addr:` and `Mask:` words.
        let net = ["  eth0: 10 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0"];
        let addr = [
            "eth0      Link encap:Ethernet  HWaddr 00:11:22:33:44:55",
            "          inet addr:10.1.2.3  Bcast:10.1.2.255  Mask:255.255.255.0",
            "          inet6 addr: fe80::211:22ff:fe33:4455/64 Scope:Link",
            "lo        Link encap:Local Loopback",
            "          inet addr:127.0.0.1  Mask:255.0.0.0",
        ];
        assert_eq!(
            network(&net, &addr),
            vec![Interface {
                name: "eth0".into(),
                addresses: vec!["10.1.2.3/24".into()],
                received: Some(10),
                sent: Some(20),
            }]
        );
        // Neither ip nor ifconfig: the interfaces with traffic, bare.
        let net = [
            "    lo: 10 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0",
            "  eth0: 10 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0",
            "  eth1: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
        ];
        assert_eq!(
            network(&net, &[]),
            vec![Interface {
                name: "eth0".into(),
                addresses: vec![],
                received: Some(10),
                sent: Some(20),
            }]
        );
        assert_eq!(mask_bits("0xffffff00"), Some(24));
        assert_eq!(mask_bits("255.255.0.0"), Some(16));
        assert_eq!(mask_bits("x"), None);
    }

    /// Lines cut short or otherwise not what ss writes are passed over,
    /// not a panic: a `LISTEN` line of four fields, of one, of three.
    #[test]
    fn malformed_ss_lines() {
        let got = listeners(&[
            "State Recv-Q Send-Q Local Address:Port Peer Address:PortProcess",
            "LISTEN 0 128 0.0.0.0:22",
            "LISTEN",
            "LISTEN 0 128",
            "LISTEN 0 128 nonsense 0.0.0.0:*",
            "LISTEN 0 5 127.0.0.1:631 0.0.0.0:*",
            "LISTEN 0 5 [::1]:631 [::]:* users:((\"cupsd\",pid=9",
        ]);
        let ports: Vec<(u16, &str, Option<u32>)> = got
            .iter()
            .map(|l| (l.port, l.address.as_str(), l.pid))
            .collect();
        assert_eq!(ports, vec![(22, "0.0.0.0", None), (631, "127.0.0.1", None)]);
        assert!(listeners(&["lsof", "sshd 1 root 3u IPv4"]).is_empty());
        assert!(listeners(&["lsof"]).is_empty());
    }

    #[test]
    fn netstat_where_there_is_no_ss() {
        // BusyBox and net-tools, with the program where the host says.
        let got = listeners(&[
            "Active Internet connections (only servers)",
            "Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name",
            "tcp        0      0 0.0.0.0:22              0.0.0.0:*               LISTEN      1/sshd",
            "tcp        0      0 127.0.0.1:8888          0.0.0.0:*               LISTEN      -",
            "tcp6       0      0 :::22                   :::*                    LISTEN      1/sshd",
            "tcp6       0      0 :::9000                 :::*                    LISTEN",
            "tcp        0      0 10.0.0.5:22             10.0.0.2:51000          ESTABLISHED -",
        ]);
        let ports: Vec<(u16, &str, &str, Option<u32>)> = got
            .iter()
            .map(|l| (l.port, l.address.as_str(), l.process.as_str(), l.pid))
            .collect();
        assert_eq!(
            ports,
            vec![
                (22, "0.0.0.0", "sshd", Some(1)),
                (8888, "127.0.0.1", "", None),
                (9000, "::", "", None)
            ]
        );
        assert_eq!(addr_port("*.22"), Some(("*".into(), 22)));
        assert_eq!(
            addr_port("127.0.0.1.8080"),
            Some(("127.0.0.1".into(), 8080))
        );
        assert_eq!(addr_port("::1.8080"), Some(("::1".into(), 8080)));
        assert_eq!(addr_port("[::]:22"), Some(("::".into(), 22)));
        assert_eq!(addr_port("nonsense"), None);
    }
}
