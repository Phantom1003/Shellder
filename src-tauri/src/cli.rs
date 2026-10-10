//! Terminal subcommands (the same binary, run with arguments).

use std::io::{IsTerminal, Write};

use crate::core::catalog;
use crate::core::keychain::{self, SecretKind};
use crate::core::log;
use crate::core::prefs;
use crate::core::ssh;
use crate::core::totp::Totp;
use crate::core::{autostart, config};

const USAGE: &str = "usage: shellder <command> [args]

  hosts                       Host entries from ~/.ssh/config and their ControlPath
  status                      master state for every locked host
  lock HOST | unlock HOST     lock a host (reconnect after drops, connect at launch) or unlock it
  add-secret HOST KIND        store password | passphrase | totp in the keychain
  del-secret HOST KIND        remove one stored secret
  install                     register as a login item and start
  uninstall                   stop and remove the login item
  help

Run with no arguments to start the app (--background: no window for this
launch; --login: a login-item launch, which honours \"start silently\").";

pub fn run(args: &[String]) -> i32 {
    log::set_also_stderr(std::io::stderr().is_terminal());
    let Some(cmd) = args.first() else {
        println!("{USAGE}");
        return 2;
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "status" => status(),
        "hosts" => hosts(),
        "lock" => set_locked(rest, true),
        "unlock" => set_locked(rest, false),
        "add-secret" => add_secret(rest),
        "del-secret" => del_secret(rest),
        "install" => install(),
        "uninstall" => uninstall(),
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            0
        }
        _ => {
            eprintln!("unknown command: {cmd}\n{USAGE}");
            2
        }
    }
}

fn status() -> i32 {
    let locked = prefs::locked_hosts();
    if locked.is_empty() {
        println!("no hosts are locked (lock one in the app or with `shellder lock HOST`)");
        return 0;
    }
    let known: Vec<String> = catalog::load()
        .entries
        .into_iter()
        .map(|e| e.alias)
        .collect();
    // Jump hosts from the config are kept up for the hosts going through
    // them, lock or no lock: list those too.
    let mut rows: Vec<(String, String)> = locked.into_iter().map(|h| (h, String::new())).collect();
    let mut i = 0;
    while i < rows.len() {
        let h = rows[i].0.clone();
        i += 1;
        let Some(j) = ssh::resolve(&h).ok().and_then(|r| r.first_jump_host()) else {
            continue;
        };
        if known.contains(&j) && j != h && !rows.iter().any(|r| r.0 == j) {
            rows.push((j, format!("  (jump host for {h})")));
        }
    }
    for (h, note) in rows {
        match ssh::control_path(&h) {
            Ok(None) => println!("{h:<24} no ControlPath configured"),
            Ok(Some(cp)) => println!(
                "{h:<24} {:<8} {cp}{note}",
                if ssh::master_alive(&h) { "UP" } else { "down" }
            ),
            Err(e) => println!("{h:<24} ERROR {e}"),
        }
    }
    0
}

fn hosts() -> i32 {
    let cat = catalog::load();
    let locked = prefs::locked_hosts();
    for e in &cat.entries {
        let mark = if locked.contains(&e.alias) { "*" } else { " " };
        match ssh::control_path(&e.alias) {
            Ok(cp) => println!(
                "{mark} {:<24} {}",
                e.alias,
                cp.unwrap_or_else(|| "(no ControlPath)".into())
            ),
            Err(err) => println!("{mark} {:<24} ERROR {err}", e.alias),
        }
    }
    println!(
        "(* = locked, from {})",
        cat.files
            .iter()
            .map(|f| config::abbreviate_home(f))
            .collect::<Vec<_>>()
            .join(", ")
    );
    0
}

fn set_locked(a: &[String], on: bool) -> i32 {
    let Some(host) = a.first() else {
        eprintln!("usage: {} HOST", if on { "lock" } else { "unlock" });
        return 2;
    };
    let known: Vec<String> = catalog::load()
        .entries
        .into_iter()
        .map(|e| e.alias)
        .collect();
    if !known.contains(host) {
        eprintln!("warning: {host} is not a Host entry in ~/.ssh/config");
    }
    prefs::set_locked(host, on);
    println!(
        "{host}: {} (the running app picks this up within a few seconds)",
        if on { "locked" } else { "unlocked" }
    );
    // Same rules as the lock buttons in the app: locking takes the jump
    // hosts along, unlocking the hosts that jump through this one.
    let chain = |host: &str| -> Vec<String> {
        let mut out = vec![];
        let mut seen = std::collections::HashSet::from([host.to_string()]);
        let mut h = host.to_string();
        while let Some(j) = ssh::resolve(&h).ok().and_then(|r| r.first_jump_host()) {
            if !known.contains(&j) || !seen.insert(j.clone()) {
                break;
            }
            out.push(j.clone());
            h = j;
        }
        out
    };
    if on {
        let mut via = host.clone();
        for j in chain(host) {
            prefs::set_locked(&j, true);
            println!("{j}: locked (jump host for {via})");
            via = j;
        }
    } else {
        for d in prefs::locked_hosts() {
            if d != *host && chain(&d).contains(host) {
                prefs::set_locked(&d, false);
                println!("{d}: unlocked (jumps through {host})");
            }
        }
    }
    0
}

fn parse_host_kind(a: &[String]) -> Option<(String, SecretKind)> {
    if a.len() != 2 {
        eprintln!("usage: HOST password|passphrase|totp");
        return None;
    }
    let Some(k) = SecretKind::parse(&a[1]) else {
        eprintln!("usage: HOST password|passphrase|totp");
        return None;
    };
    Some((a[0].clone(), k))
}

/// Interactive: hidden prompt on the tty. Piped stdin: first line, so
/// scripts can do `printf '%s' "$PW" | shellder add-secret HOST password`.
fn read_secret(prompt: &str) -> Option<String> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let mut line = String::new();
        if stdin.read_line(&mut line).ok()? == 0 {
            return None;
        }
        return Some(line.trim_end_matches(['\n', '\r']).to_string());
    }
    // A hidden prompt on the terminal: echo off while the line is typed.
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    let had = unsafe { libc::tcgetattr(0, &mut term) } == 0;
    if had {
        let mut quiet = term;
        quiet.c_lflag &= !libc::ECHO;
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &quiet) };
    }
    let mut line = String::new();
    let read = stdin.read_line(&mut line);
    if had {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &term) };
    }
    println!();
    read.ok()?;
    Some(line.trim_end_matches(['\n', '\r']).to_string())
}

fn add_secret(a: &[String]) -> i32 {
    let Some((host, kind)) = parse_host_kind(a) else {
        return 2;
    };
    let prompt = if kind == SecretKind::Totp {
        format!("TOTP secret (base32 or otpauth:// URI) for {host}: ")
    } else {
        format!("{} for {host}: ", kind.raw())
    };
    let Some(value) = read_secret(&prompt).filter(|v| !v.is_empty()) else {
        eprintln!("aborted");
        return 1;
    };
    let result = if kind == SecretKind::Totp {
        Totp::parse(&value).and_then(|t| keychain::set(&host, kind, &value).map(|_| Some(t.code())))
    } else {
        keychain::set(&host, kind, &value).map(|_| None)
    };
    match result {
        Ok(Some(code)) => {
            println!("stored. current code: {code}  (should match your authenticator app)");
            0
        }
        Ok(None) => {
            println!("stored {} for {host}", kind.raw());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn del_secret(a: &[String]) -> i32 {
    let Some((host, kind)) = parse_host_kind(a) else {
        return 2;
    };
    if let Err(e) = keychain::delete(&host, kind) {
        eprintln!("error: {e}");
        return 1;
    }
    println!("deleted {} for {host}", kind.raw());
    0
}

fn install() -> i32 {
    match autostart::install(true) {
        Ok(()) => {
            println!("installed and started {}", config::LABEL);
            println!("log: {}", config::path_str(&config::get().log_file));
            for h in prefs::locked_hosts() {
                if !matches!(ssh::control_path(&h), Ok(Some(_))) {
                    println!("WARNING: {h} has no ControlPath in ~/.ssh/config");
                }
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn uninstall() -> i32 {
    autostart::uninstall(true);
    println!("removed {}", config::LABEL);
    0
}
