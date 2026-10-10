//! One copy: what to move, from which side, into which directory on the
//! other one, and how it is going.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::remotefs::{self, fs};
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

    /// Stop the copy. Whatever has already landed stays there.
    pub fn cancel(&self) {
        let pid = {
            let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            s.cancelled = true;
            s.pid
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
                self.watch(job.clone(), t, progress);
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
    /// for less often than a local one.
    fn watch(
        self: &Arc<Self>,
        job: TransferJob,
        total: i64,
        progress: impl Fn(f64) + Send + 'static,
    ) {
        let every = if job.to.is_local() {
            Duration::from_millis(400)
        } else {
            Duration::from_secs(1)
        };
        let me = Arc::downgrade(self);
        thread::spawn(move || loop {
            thread::sleep(every);
            let Some(run) = me.upgrade() else { return };
            if run.is_over() {
                return;
            }
            let landed = fs::bytes(&job.to, &job.landing());
            if run.is_over() {
                return;
            }
            let Some(done) = landed else { return };
            // The last percent is the process exiting, not a measurement.
            progress((done as f64 / total as f64).min(0.99));
        });
    }
}
