//! One copy: what to move, from which side, into which directory on the
//! other one, and how it is going. sftp (scp between two hosts), through
//! the master when one is up and on a login of its own otherwise.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::remotefs::{self, fs, Size};
use super::ssh::{self, FileSource};

/// One drag: what to copy, from which side, and into which directory on
/// the other one. Either side can be this machine or a host, so a copy
/// between two hosts is a job like any other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub from: FileSource,
    pub to: FileSource,
    /// The item to copy, on the side it comes from.
    pub source: String,
    /// The directory it goes into, on the side it lands on.
    pub destination: String,
    /// Pick up where an earlier copy of the same thing stopped: what the
    /// destination already has of it is kept and the rest appended.
    #[serde(default)]
    pub resume: bool,
}

impl TransferJob {
    pub fn name(&self) -> String {
        remotefs::last_component(&self.source)
    }
    /// Where the copy ends up: the destination directory plus the name.
    pub fn landing(&self) -> String {
        remotefs::join(&self.destination, &self.name())
    }
    /// Into a host is up, onto this machine is down: which arrow it draws.
    pub fn is_upload(&self) -> bool {
        !self.to.is_local()
    }
    /// Whether a second run can go on from what the first left behind:
    /// sftp can, the scp between two hosts cannot.
    pub fn can_resume(&self) -> bool {
        self.from.is_local() || self.to.is_local()
    }

    /// A side without a master up: the copy logs in on its own.
    pub fn logs_in(&self) -> bool {
        ssh::copy_logs_in(&self.from, &self.to)
    }
}

/// One job's sftp (or scp), with its progress followed by measuring what
/// has arrived on the destination side. The meter they draw is for a
/// terminal, and watching the destination works the same way in both
/// directions; a resumed copy starts out at what was already there.
#[derive(Default)]
pub struct TransferRun {
    inner: Arc<Mutex<RunState>>,
}

#[derive(Default)]
struct RunState {
    pid: Option<u32>,
    cancelled: bool,
    finished: bool,
}

impl TransferRun {
    pub fn new() -> Arc<TransferRun> {
        Arc::new(TransferRun::default())
    }

    /// Stop the copy. Whatever has already landed stays there. A copy
    /// that has finished is not signalled: its pid may be another
    /// process's by now.
    pub fn cancel(&self) {
        let pid = {
            let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            s.cancelled = true;
            s.pid.filter(|_| !s.finished)
        };
        if let Some(pid) = pid {
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancelled
    }

    fn is_over(&self) -> bool {
        let s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        s.finished || s.cancelled
    }

    /// Bytes to move, measured on the source side. None when the side could
    /// not answer, which leaves the progress bar indeterminate.
    pub fn total(job: &TransferJob) -> Option<i64> {
        fs::bytes(&job.from, &job.source)
    }

    /// Runs the copy to completion. `progress` is called from another thread
    /// with a fraction of `total` while the copy runs, never after it ends.
    pub fn run(
        self: &Arc<Self>,
        job: &TransferJob,
        total: Option<i64>,
        progress: impl Fn(f64) + Send + 'static,
    ) -> ssh::Output {
        if let Some(t) = total {
            if t > 0 {
                // What is at the landing before the copy begins: a resumed
                // copy goes on from it, so it needs no look.
                let before = if job.resume {
                    None
                } else {
                    fs::measure(&job.to, &job.landing())
                };
                self.watch(job.clone(), t, before, progress);
            }
        }
        let inner = self.inner.clone();
        let mut on_start = move |pid: u32| {
            let stop = {
                let mut s = inner.lock().unwrap_or_else(|e| e.into_inner());
                s.pid = Some(pid);
                s.cancelled
            };
            if stop {
                unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            }
        };
        let out = ssh::copy(
            std::slice::from_ref(&job.source),
            &job.from,
            &job.destination,
            &job.to,
            job.resume,
            Some(&mut on_start),
        );
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finished = true;
        out
    }

    /// Asks the destination side how much of the copy is there yet, until
    /// the copy ends. A remote answer costs an ssh session, so it is asked
    /// for less often than a local one, and a tree that takes long to
    /// measure is measured less often again: the wait after a look is a
    /// few times what the look took.
    fn watch(
        self: &Arc<Self>,
        job: TransferJob,
        total: i64,
        before: Option<Size>,
        progress: impl Fn(f64) + Send + 'static,
    ) {
        let every = if job.to.is_local() {
            Duration::from_millis(400)
        } else {
            Duration::from_secs(1)
        };
        let me = Arc::downgrade(self);
        thread::spawn(move || {
            let mut wait = every;
            loop {
                thread::sleep(wait);
                let Some(run) = me.upgrade() else { return };
                if run.is_over() {
                    return;
                }
                let started = Instant::now();
                let landed = fs::bytes(&job.to, &job.landing());
                wait = (started.elapsed() * 4).clamp(every, Duration::from_secs(30));
                if run.is_over() {
                    return;
                }
                let Some(landed) = landed else { return };
                progress(fraction(landed, before, total));
            }
        });
    }
}

/// How far a copy is, from the bytes at its landing now and what was there
/// before it began (None for a resumed copy, which goes on from what it
/// finds, so that counts). A directory it goes into keeps what it had,
/// which is not the copy's and is taken off; a file it writes over is
/// started anew, so until the file's size has moved the copy has not
/// begun. The last percent is the process exiting, not a measurement.
fn fraction(landed: i64, before: Option<Size>, total: i64) -> f64 {
    let done = match before {
        Some(Size {
            bytes,
            directory: true,
        }) => landed - bytes,
        Some(Size {
            bytes,
            directory: false,
        }) if landed == bytes => 0,
        _ => landed,
    };
    (done.max(0) as f64 / total as f64).min(0.99)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_leaves_out_what_was_there() {
        let dir = |bytes| {
            Some(Size {
                bytes,
                directory: true,
            })
        };
        let file = |bytes| {
            Some(Size {
                bytes,
                directory: false,
            })
        };
        // Into a new place, or going on from what is there: all of it.
        assert_eq!(fraction(50, Some(Size::default()), 100), 0.5);
        assert_eq!(fraction(50, None, 100), 0.5);
        // Into a folder that had 400 bytes of other files.
        assert_eq!(fraction(400, dir(400), 100), 0.0);
        assert_eq!(fraction(425, dir(400), 100), 0.25);
        // Over a file of 80 bytes: nothing until it starts over.
        assert_eq!(fraction(80, file(80), 100), 0.0);
        assert_eq!(fraction(10, file(80), 100), 0.1);
        assert_eq!(fraction(90, file(80), 100), 0.9);
        // Never the end, never below nothing.
        assert_eq!(fraction(100, None, 100), 0.99);
        assert_eq!(fraction(300, dir(400), 100), 0.0);
    }
}
