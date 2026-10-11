//! A pseudo-terminal that is only the master's controlling terminal. ssh
//! never reads a password from stdin: `read_passphrase()` opens /dev/tty,
//! and so does every ProxyJump hop it starts. Give the process a pty as its
//! controlling terminal, keep stdin/stdout/stderr on ordinary pipes, and
//! the only thing that ever comes down the pty is a question — which is
//! answered by writing the line back. No helper, no socket, no need to ask
//! the kernel who is on the other end: whoever reads the slave was started
//! by this process.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::Arc;

use super::log;
use super::prompt::{self, Origin, PromptHandler, PromptRequest};

pub struct Pty {
    pub master: OwnedFd,
    /// Kept open on this side for as long as the process lives: the child
    /// holds no fd to it (it opens /dev/tty only to ask), and a pty whose
    /// slave nobody holds reads as hung up on macOS, which would end the
    /// reader and destroy the terminal under the process. Closing it is
    /// what tells the reader the process is gone.
    pub slave: OwnedFd,
}

fn errno_string() -> String {
    std::io::Error::last_os_error().to_string()
}

impl Pty {
    pub fn open() -> Result<Pty, String> {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        let rc = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            return Err(format!("openpty(): {}", errno_string()));
        }
        for fd in [master, slave] {
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }
        Ok(Pty {
            master: unsafe { OwnedFd::from_raw_fd(master) },
            slave: unsafe { OwnedFd::from_raw_fd(slave) },
        })
    }

    /// Make the pty the controlling terminal of the process `cmd` starts,
    /// and nothing else: its standard streams stay whatever `cmd` says.
    pub fn attach(&self, cmd: &mut Command) {
        let slave: RawFd = self.slave.as_raw_fd();
        unsafe {
            cmd.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(slave, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // The slave fd itself is not needed: /dev/tty reaches the
                // controlling terminal from now on, and it is close-on-exec.
                Ok(())
            });
        }
    }
}

/// Reads the master side until the process on the other end is gone. A
/// line is logged as it arrives; text left waiting without a newline for a
/// moment is a question, answered through `handler`.
pub fn serve(
    host: String,
    run_id: u32,
    origin: Origin,
    master: OwnedFd,
    handler: Arc<dyn PromptHandler>,
) {
    let fd = master.as_raw_fd();
    let mut file = std::fs::File::from(master);
    let mut bytes: Vec<u8> = Vec::new();
    // What the terminal received since the last answer, '\r' dropped.
    let mut text = String::new();
    // How much of `text` has been looked at for complete lines.
    let mut scanned = 0usize;
    let mut last_answer: Option<String> = None;
    let mut chunk = [0u8; 4096];
    loop {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, 200) };
        if rc < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if rc == 0 {
            // Quiet. Whatever sits there without a newline is a question.
            let tail = &text[scanned..];
            if prompt::looks_like_question(tail) {
                let question = text.trim().to_string();
                let kind = prompt::classify(&question);
                let req = PromptRequest {
                    host: host.clone(),
                    prompt: question,
                    kind,
                    run_id,
                    origin,
                };
                let answer = handler.ask(&req);
                text.clear();
                scanned = 0;
                last_answer = None;
                if let Some(a) = answer {
                    let mut line = a.clone();
                    line.push('\n');
                    if file.write_all(line.as_bytes()).is_err() {
                        break;
                    }
                    last_answer = Some(a);
                }
            }
            continue;
        }
        if pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
            && pfd.revents & libc::POLLIN == 0
        {
            // Nobody holds the slave any more: the process is gone and
            // its MasterProc, which kept our side open, was dropped.
            break;
        }
        let n = match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.raw_os_error() == Some(libc::EINTR) => continue,
            Err(_) => break, // EIO: the slave side is closed, the process is gone
        };
        bytes.extend_from_slice(&chunk[..n]);
        let good = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(e) => e.valid_up_to(),
        };
        let decoded = String::from_utf8_lossy(&bytes[..good]).into_owned();
        bytes.drain(..good);
        text.extend(decoded.chars().filter(|&c| c != '\r'));
        // Complete lines go to the log as they come, unless they are our
        // own answer echoed back (the terminal echoes a yes/no, never a
        // password). The lines stay in `text`: above a question they are
        // its context (a host key's fingerprint).
        while let Some(off) = text[scanned..].find('\n') {
            let line = text[scanned..scanned + off].trim();
            if last_answer.as_deref() == Some(line) {
                // Our own answer echoed back: not context for what follows.
                text.drain(scanned..scanned + off + 1);
                continue;
            }
            if !line.is_empty() {
                log::ssh(&host, line);
            }
            scanned += off + 1;
        }
        if scanned > 8192 {
            // Nothing is asking: forget the oldest lines.
            let keep = text[..scanned].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let cut = keep.min(scanned - 4096);
            let cut = text[..cut].rfind('\n').map(|i| i + 1).unwrap_or(0);
            text.drain(..cut);
            scanned -= cut;
        }
    }
    handler.master_gone(&host, run_id);
}
