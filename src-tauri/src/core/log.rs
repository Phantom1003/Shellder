//! Append-only file logger shared by every process mode (app, CLI).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::config;

static LOCK: Mutex<()> = Mutex::new(());
static ALSO_STDERR: AtomicBool = AtomicBool::new(false);

pub fn set_also_stderr(on: bool) {
    ALSO_STDERR.store(on, Ordering::Relaxed);
}

pub fn write(level: &str, msg: &str) {
    let line = format!(
        "{} {} [{}] {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        level,
        std::process::id(),
        msg
    );
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut f) = append_handle() {
        let _ = f.write_all(line.as_bytes());
    }
    if ALSO_STDERR.load(Ordering::Relaxed) {
        let _ = std::io::stderr().write_all(line.as_bytes());
    }
}

pub fn info(m: impl AsRef<str>) {
    write("INFO", m.as_ref())
}
/// Output of an ssh process we spawned (its stderr/stdout), one line each.
pub fn ssh(host: &str, line: &str) {
    write("SSH", &format!("{host}: {line}"))
}
/// A prompt ssh raised and how it was answered.
pub fn prompt(m: impl AsRef<str>) {
    write("PROMPT", m.as_ref())
}
pub fn warn(m: impl AsRef<str>) {
    write("WARNING", m.as_ref())
}
pub fn error(m: impl AsRef<str>) {
    write("ERROR", m.as_ref())
}

/// Start a fresh log for this run: the previous one is kept next to it as
/// shellder.log.old so a crash the login item restarted us from is not lost.
pub fn start_fresh() {
    config::ensure_dirs();
    let log = &config::get().log_file;
    let old = log.with_extension("log.old");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(log, &old);
    let _ = create(log);
}

fn create(path: &std::path::Path) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// A handle positioned at the end of the log file.
pub fn append_handle() -> Option<File> {
    config::ensure_dirs();
    create(&config::get().log_file).ok()
}
