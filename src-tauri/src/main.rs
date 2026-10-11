#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use shellder::core::autostart::LOGIN_FLAG;
use shellder::{app, cli, core};

/// flock-based guard so the login item, a manual launch and a second copy
/// of the app never run two daemons (they would fight over the same
/// ControlPath sockets). The fd is kept open for the life of the process.
/// A lock that is taken means another instance and the launch gives up at
/// once, except after a Tauri restart (`app::restart`: language change,
/// installed update), which starts the new copy while the old one is
/// still on its way out: that launch, marked by `config::RELAUNCH_ENV`,
/// tries again for a few seconds first.
fn single_instance() -> bool {
    use std::os::fd::IntoRawFd;
    core::config::ensure_dirs();
    let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&core::config::get().lock_file)
    else {
        return true; // cannot lock: do not block the user
    };
    let fd = f.into_raw_fd();
    let relaunch = std::env::var_os(core::config::RELAUNCH_ENV).is_some();
    std::env::remove_var(core::config::RELAUNCH_ENV); // not for the processes we start
    let wait = if relaunch { 10 } else { 0 };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
    loop {
        if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn main() {
    // CLI mode: any argument is a subcommand, except our own flags:
    // --background forces a start without a window, --login marks a launch
    // by the login item (the only one that honours "start silently").
    let mut args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("-psn_"))
        .collect();
    let background = args.iter().any(|a| a == "--background");
    let login_item = args.iter().any(|a| a == LOGIN_FLAG);
    args.retain(|a| a != "--background" && a != LOGIN_FLAG);
    if !args.is_empty() {
        std::process::exit(cli::run(&args));
    }

    // GUI app.
    if !single_instance() {
        core::log::warn("another shellder instance is running; exiting");
        eprintln!("shellder is already running");
        std::process::exit(0);
    }
    core::log::start_fresh();
    app::run(background, login_item);
}
