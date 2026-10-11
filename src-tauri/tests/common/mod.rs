//! A throwaway sshd for the integration tests: started as this user on a
//! high port with a host key and an authorized key of its own, both made
//! here, so only key authentication is possible (a password needs root).
//! The key file has a passphrase and the host key is new to the client,
//! so a login asks twice on the controlling pty.
#![allow(dead_code)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Sshd {
    pub dir: String,
    pub port: u16,
    pub user: String,
    child: std::process::Child,
}

pub const PASSPHRASE: &str = "s3cret";

fn run(cmd: &str, args: &[&str]) {
    let st = Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    assert!(st.success(), "{cmd} {args:?}");
}

impl Sshd {
    /// None when this machine has no sshd to start (CI's Linux runners
    /// have one; a Mac has /usr/sbin/sshd whether Remote Login is on or
    /// not). `name` keeps the scratch directories of two tests apart.
    pub fn start(name: &str) -> Option<Sshd> {
        if !std::path::Path::new("/usr/sbin/sshd").exists() {
            eprintln!("no sshd here, skipping");
            return None;
        }
        let dir = std::env::temp_dir().join(format!("shjt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap().to_string();
        run(
            "ssh-keygen",
            &[
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                &format!("{d}/hostkey"),
            ],
        );
        run(
            "ssh-keygen",
            &[
                "-q",
                "-t",
                "ed25519",
                "-N",
                PASSPHRASE,
                "-f",
                &format!("{d}/id"),
            ],
        );
        std::fs::copy(format!("{d}/id.pub"), format!("{d}/authorized_keys")).unwrap();
        let port = 2200 + (std::process::id() % 1000) as u16;
        let user = std::env::var("USER").unwrap();
        std::fs::write(
            format!("{d}/sshd_config"),
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/hostkey\nAuthorizedKeysFile {d}/authorized_keys\n\
                 PasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nPidFile none\n\
                 LogLevel ERROR\nAllowUsers {user}\nSubsystem sftp internal-sftp\n"
            ),
        )
        .unwrap();
        let child = Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f", &format!("{d}/sshd_config")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let up = Instant::now();
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                up.elapsed() < Duration::from_secs(10),
                "sshd did not come up"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        Some(Sshd {
            dir: d,
            port,
            user,
            child,
        })
    }

    /// An ssh config with one Host entry for this daemon, `extra` lines
    /// (a ControlPath, say) appended, written as the config shellder uses
    /// (SHELLDER_SSH_CONFIG). The state directory goes to the same place.
    pub fn configure(&self, alias: &str, extra: &str) {
        let d = &self.dir;
        std::fs::write(
            format!("{d}/config"),
            format!(
                "Host {alias}\n    HostName 127.0.0.1\n    Port {}\n    User {}\n    IdentityFile {d}/id\n    IdentitiesOnly yes\n\
                 StrictHostKeyChecking ask\n    UserKnownHostsFile {d}/known_hosts\n    LogLevel ERROR\n{extra}",
                self.port, self.user
            ),
        )
        .unwrap();
        std::env::set_var("SHELLDER_SSH_CONFIG", format!("{d}/config"));
        std::env::set_var("SHELLDER_STATE_DIR", d);
    }
}

/// Killed when the test ends, however it ends: a daemon left behind holds
/// the harness's pipes open.
impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Bytes that are not all alike, so a copy that landed in the wrong place
/// or order would show.
pub fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}
