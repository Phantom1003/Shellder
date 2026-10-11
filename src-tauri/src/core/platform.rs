//! What this platform's OpenSSH can do. Windows' OpenSSH has no
//! ControlMaster (there are no Unix sockets for the multiplexer to listen
//! on), so there is nothing for the daemon to keep alive there: the switch
//! and the lock disappear, and every shell or Files window logs in on its
//! own, with shellder answering the prompts.

use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// `ssh -M` works here: the daemon can keep masters alive, the switch
    /// and the lock make sense.
    pub control_master: bool,
}

/// `SHELLDER_NO_CONTROLMASTER=1` is the test hook that shows the app the
/// way a platform without ControlMaster sees it, on one that has it.
pub fn capabilities() -> Capabilities {
    let forced_off = env::var_os("SHELLDER_NO_CONTROLMASTER")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    Capabilities {
        control_master: !cfg!(windows) && !forced_off,
    }
}

pub fn control_master() -> bool {
    capabilities().control_master
}
