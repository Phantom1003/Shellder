#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use shellder::core::autostart::LOGIN_FLAG;
use shellder::{app, cli, core};

/// flock-based guard so the login item and a manual launch never run two
/// daemons. The fd is kept open for the life of the process.
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
    unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) == 0 }
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
