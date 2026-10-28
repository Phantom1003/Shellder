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
use std::sync::{Arc, Mutex};

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
/// moment is a question, answered through `handler`, and one it leaves
/// unanswered hangs a terminal's or a session's process up. With `said`,
/// what came down the pty and was not a question or its context is
/// collected there: the output of a process whose standard streams are
/// the pty (`ssh::run_on_terminal`), complete by the time this returns.
pub fn serve(
    host: String,
    run_id: u32,
    origin: Origin,
    master: OwnedFd,
    handler: Arc<dyn PromptHandler>,
    said: Option<Arc<Mutex<String>>>,
) {
    let keep = |s: &str| {
        if let Some(said) = &said {
            said.lock().unwrap_or_else(|e| e.into_inner()).push_str(s);
        }
    };
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
        // A moment of quiet only matters after text that could be a
        // question; anything else waits for the next output, so a pty
        // nobody asks on (a master's, for hours) is not woken for nothing.
        let wait = if prompt::looks_like_question(&text[scanned..]) {
            QUIET_MS
        } else {
            -1
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, wait) };
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
                match answer {
                    Some(a) => {
                        let mut line = a.clone();
                        line.push('\n');
                        if file.write_all(line.as_bytes()).is_err() {
                            break;
                        }
                        last_answer = Some(a);
                    }
                    None if hangs_up_unanswered(origin) => hang_up(&host, run_id),
                    None => {}
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
        // As a terminal would show it: '\r' is nothing (lines end in
        // '\n'), a backspace takes the character before it off the line
        // (sudo's pwfeedback shows a star per character typed and rubs
        // them out again once the password is in).
        for c in decoded.chars() {
            match c {
                '\r' => {}
                '\u{8}' => {
                    if text.len() > scanned && !text.ends_with('\n') {
                        text.pop();
                    }
                }
                c => text.push(c),
            }
        }
        // Complete lines go to the log as they come, unless they are our
        // own answer echoed back (the terminal echoes a yes/no, never a
        // password; a sudo with pwfeedback shows a star for each of its
        // characters, which says how many there are, and that is not
        // logged either). The lines stay in `text`: above a question they
        // are its context (a host key's fingerprint).
        while let Some(off) = text[scanned..].find('\n') {
            let line = text[scanned..scanned + off].trim();
            let feedback =
                last_answer.is_some() && !line.is_empty() && line.chars().all(|c| c == '*');
            if last_answer.as_deref() == Some(line) || feedback {
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
            let cut = forget_up_to(&text, scanned);
            keep(&text[..cut]);
            text.drain(..cut);
            scanned -= cut;
        }
    }
    // What came after the last answer, and was never asked about.
    keep(&text);
    handler.master_gone(&host, run_id);
}

/// How long text that looks like a question must sit there, unchanged,
/// before it is taken for one.
const QUIET_MS: libc::c_int = 200;

/// How much of the text the reader holds can go once more than 8 KB of
/// it are complete lines (`scanned`): everything before the line that
/// holds the byte 4 KB before their end, so that about the last 4 KB stay
/// as context for a question to come. That byte may sit inside a
/// character (output that is not ASCII): the line is looked for before
/// the character's first byte.
fn forget_up_to(text: &str, scanned: usize) -> usize {
    let mut at = scanned.saturating_sub(4096).min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Whether a question nobody answered (the user cancelled it, a stored
/// secret was rejected) must end the process here. A master's attempt is
/// the daemon's to end, and an action's handler ends the action itself
/// (`ActionPrompts`); a terminal, a session or a copy would otherwise
/// wait at the question for ever, since nothing else is going to answer.
fn hangs_up_unanswered(origin: Origin) -> bool {
    match origin {
        Origin::Terminal | Origin::Session => true,
        Origin::Master | Origin::Action => false,
    }
}

/// Hang the process that asked up, as its terminal going away would: a
/// SIGHUP to its process group (it is a session leader, see
/// `Pty::attach`, so the group is its pid), which takes in what it
/// started on the same terminal (a ProxyJump hop, an sftp's ssh), with
/// the process alone if the group is not there.
fn hang_up(host: &str, pid: u32) {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return;
    };
    if pid <= 1 {
        return;
    }
    log::prompt(format!(
        "{host}: nobody answered; hanging up the process that asked (pid {pid})"
    ));
    unsafe {
        if libc::kill(-pid, libc::SIGHUP) != 0 {
            libc::kill(pid, libc::SIGHUP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgetting_cuts_at_a_line_and_never_inside_a_character() {
        // Lines of three-byte characters: 4 KB before the end of the
        // scanned text falls inside one.
        let line = format!("{}\n", "中".repeat(1000));
        let text = line.repeat(3);
        let scanned = text.len();
        assert!(!text.is_char_boundary(scanned - 4096));
        let cut = forget_up_to(&text, scanned);
        assert_eq!(cut, line.len());
        assert!(text.is_char_boundary(cut));
        // A text with unscanned rest, and ASCII: still at a line.
        let mut text = "x".repeat(99) + "\n";
        text = text.repeat(100) + "Password";
        let cut = forget_up_to(&text, 100 * 100);
        assert_eq!(cut % 100, 0);
        assert_eq!(cut, 100 * 100 - 4100);
        // One long line: nothing to cut at.
        let text = "é".repeat(6000) + "\n";
        assert_eq!(forget_up_to(&text, text.len()), 0);
    }

    #[test]
    fn only_terminals_and_sessions_are_hung_up_here() {
        assert!(hangs_up_unanswered(Origin::Terminal));
        assert!(hangs_up_unanswered(Origin::Session));
        assert!(!hangs_up_unanswered(Origin::Master));
        assert!(!hangs_up_unanswered(Origin::Action));
    }
}
