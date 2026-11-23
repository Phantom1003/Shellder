//! How long the app takes over what it reads: a question off the pty, a
//! listing of ten thousand files, a host's readings, the ssh config, the
//! prefs. Nothing here starts a process; `pipes.rs` has what does.
//!
//! `cargo bench --bench parsing`, or `-- listing` for the measurements
//! with that word in their name.

mod harness;

use std::time::Duration;

use shellder::core::prompt;
use shellder::core::totp::Totp;
use shellder::core::{catalog, monitor, prefs, remotefs, ssh};

const LINUX: &str = include_str!("../src/core/monitor/fixtures/linux.txt");
const MACOS: &str = include_str!("../src/core/monitor/fixtures/macos.txt");
const TREE: &str = include_str!("../src/core/monitor/fixtures/tree-debian.txt");

const fn us(n: u64) -> Duration {
    Duration::from_micros(n)
}
const fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// `ls -nL` of a directory of `files`, as GNU's writes it, every tenth
/// entry a directory and some names with blanks.
fn listing(files: usize) -> String {
    let mut text = String::from("total 123456\n");
    for i in 0..files {
        let (mode, name) = match i % 10 {
            0 => ("drwxr-xr-x", format!("folder {i:05}")),
            1 => ("lrwxrwxrwx", format!("link-{i:05} -> target")),
            _ => ("-rw-r--r--", format!("measurement-{i:05}.dat")),
        };
        text.push_str(&format!(
            "{mode} 1 1000 1000 {} 2026-09-{:02} 08:48:{:02}.250000000 +0000 {name}\n",
            i * 4096,
            1 + i % 28,
            i % 60
        ));
    }
    text
}

/// `ps` as `ssh::orphans_in` reads it, one process in twenty an ssh of
/// the app's left without a parent.
fn processes(count: usize) -> String {
    (0..count)
        .map(|i| {
            if i % 20 == 0 {
                format!(
                    "{:5}     1 /usr/bin/ssh -o ControlPersist=no -o Tag=shellder -T host{i} sh\n",
                    1000 + i
                )
            } else {
                format!(
                    "{:5} {:5} /usr/libexec/some-daemon --flag --config /etc/thing{i}.conf\n",
                    1000 + i,
                    500 + i % 7
                )
            }
        })
        .collect()
}

/// An ssh config of `hosts` Host blocks, in files of a hundred that the
/// main one includes by a glob.
fn config_files(dir: &str, hosts: usize) {
    std::fs::create_dir_all(format!("{dir}/conf.d")).unwrap();
    for file in 0..hosts / 100 {
        let text: String = (0..100)
            .map(|i| {
                let n = file * 100 + i;
                format!(
                    "# machine {n}\nHost node{n:04} n{n}\n    HostName 10.0.{}.{}\n    User alice\n    \
                     ProxyJump gate\n    ControlPath ~/.ssh/cm-%C\n\n",
                    n / 250,
                    n % 250
                )
            })
            .collect();
        std::fs::write(format!("{dir}/conf.d/{file:02}.conf"), text).unwrap();
    }
    std::fs::write(
        format!("{dir}/config"),
        format!("Host gate\n    HostName gate.example\nHost *\n    ServerAliveInterval 30\nInclude {dir}/conf.d/*.conf\n"),
    )
    .unwrap();
}

fn main() {
    let dir = harness::scratch("parsing");
    let mut b = harness::Bench::from_args("parsing (no process started)");

    let questions = [
        "alice@gate.example's password: ",
        "(alice@otp-gw) Verification code: ",
        "Enter passphrase for key '/home/alice/.ssh/id_ed25519': ",
        "The authenticity of host 'h (10.0.0.1)' can't be established.\n\
         ED25519 key fingerprint is SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG.\n\
         Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
        "[sudo] password for alice: ",
        "Last login: Tue Sep 24 08:48:49 2026 from 10.0.0.2",
    ];
    // Compiled once, at the first question: not what is measured.
    prompt::classify(questions[0]);
    let mut at = 0;
    b.each("a question told apart (prompt::classify)", us(5), || {
        at = (at + 1) % questions.len();
        (
            prompt::classify(questions[at]),
            prompt::user_host(questions[at]),
        )
    });

    let totp = Totp::parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=6&period=30").unwrap();
    let mut second = 1_700_000_000u64;
    b.each("a TOTP code", us(10), || {
        second += 30;
        totp.code_at(second)
    });

    let text = listing(10_000);
    let now = 1_790_000_000.0;
    assert_eq!(remotefs::parse(&text, "/data", now).len(), 10_000);
    b.each(
        "a listing of 10,000 files read (remotefs::parse)",
        ms(30),
        || remotefs::parse(&text, "/data", now),
    );
    // Twice the files in about twice the time: a listing that took four
    // times as long would be reading every line against every other.
    let double = listing(20_000);
    b.each("a listing of 20,000 files read", ms(60), || {
        remotefs::parse(&double, "/data", now)
    });

    b.each(
        "a Linux host's readings read (monitor::parse)",
        us(500),
        || monitor::parse(LINUX),
    );
    b.each("a Mac's readings read", us(500), || monitor::parse(MACOS));
    b.each("a process tree read (monitor::parse_tree)", us(500), || {
        monitor::parse_tree(TREE)
    });

    let ps = processes(5_000);
    assert_eq!(ssh::orphans_in(&ps).len(), 250);
    b.each("5,000 processes looked through for orphans", ms(2), || {
        ssh::orphans_in(&ps)
    });

    config_files(&dir, 2_000);
    assert_eq!(catalog::load().entries.len(), 2_001);
    b.each(
        "an ssh config of 2,000 hosts in 20 files (catalog::load)",
        ms(20),
        catalog::load,
    );

    // What a host's file comes back as: base64 in lines, 8 MB of it.
    let bytes: Vec<u8> = (0..8 * 1_048_576u32)
        .map(|i| ((i * 31) >> 3) as u8)
        .collect();
    let encoded = {
        use base64::Engine;
        let one = base64::engine::general_purpose::STANDARD.encode(&bytes);
        one.as_bytes()
            .chunks(76)
            .map(|l| std::str::from_utf8(l).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        remotefs::decode_base64(&encoded).as_deref(),
        Some(&bytes[..])
    );
    b.rate("a file's base64 decoded (8 MB)", 5, 70.0, || {
        let text = encoded.clone().into_bytes();
        let t = std::time::Instant::now();
        let decoded = remotefs::decode_base64_owned(text);
        (decoded.map(|d| d.len()).unwrap_or(0), t.elapsed())
    });

    // The prefs are read afresh at every access and written whole.
    prefs::update(|p| {
        p.locked_hosts = (0..50).map(|i| format!("node{i:04}")).collect();
        for i in 0..50 {
            p.idle_modes.insert(format!("node{i:04}"), "shell".into());
        }
    })
    .unwrap();
    assert_eq!(prefs::load().locked_hosts.len(), 50);
    b.each("the prefs read (prefs::load)", us(300), prefs::load);
    let mut flip = false;
    b.each(
        "the prefs changed and written (prefs::update)",
        ms(40),
        || {
            flip = !flip;
            prefs::update(|p| p.keep_in_background = flip)
        },
    );

    let _ = std::fs::remove_dir_all(&dir);
    b.finish()
}
