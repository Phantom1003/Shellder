//! Append-only file logger shared by every process mode (app, CLI).
//!
//! The file is opened once and kept open, in append mode: the app and a
//! CLI run write to it at the same time, each line landing whole at the
//! end. Now and then the name is looked up again, so that a log moved
//! away underneath (a GUI that started fresh while a CLI run was going,
//! a state directory deleted by hand) is not written into unseen.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::config;

/// The open log, and when its name was last looked up.
struct Sink {
    file: Option<File>,
    checked: Option<Instant>,
}

static SINK: Mutex<Sink> = Mutex::new(Sink {
    file: None,
    checked: None,
});
static ALSO_STDERR: AtomicBool = AtomicBool::new(false);

/// How often the log's name is looked up again (a stat, no more).
const RECHECK: Duration = Duration::from_secs(10);

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
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    sink.refresh();
    let failed = match sink.file.as_mut() {
        Some(f) => f.write_all(line.as_bytes()).is_err(),
        None => false,
    };
    if failed {
        // Opened again with the next line.
        sink.file = None;
        sink.checked = None;
    }
    if ALSO_STDERR.load(Ordering::Relaxed) {
        let _ = std::io::stderr().write_all(line.as_bytes());
    }
}

impl Sink {
    /// Every `RECHECK` (and at the first line): make sure the file held
    /// open is still the one under the log's name, and open it (the state
    /// directory made first) when it is not, or none is open. A log that
    /// cannot be opened is tried again after the same while, not at every
    /// line.
    fn refresh(&mut self) {
        let now = Instant::now();
        if self
            .checked
            .is_some_and(|t| now.duration_since(t) < RECHECK)
        {
            return;
        }
        self.checked = Some(now);
        if self.file.as_ref().is_some_and(still_named) {
            return;
        }
        config::ensure_dirs();
        self.file = create(&config::get().log_file).ok();
    }
}

/// Whether `f` is still the file at the log's path.
fn still_named(f: &File) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (f.metadata(), std::fs::metadata(&config::get().log_file)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = f;
        config::get().log_file.exists()
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
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    config::ensure_dirs();
    let log = &config::get().log_file;
    let old = log.with_extension("log.old");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(log, &old);
    sink.file = create(log).ok();
    sink.checked = Some(Instant::now());
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
