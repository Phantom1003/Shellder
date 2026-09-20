import Darwin
import Foundation

/// Who is on the other end. The askpass helper is an ordinary executable that
/// any process running as this user could start, so the app does not trust
/// what a request says about itself: it asks the kernel who connected and
/// walks that process's ancestry.
enum Proc {
    /// Executable path of a running process (symlinks resolved by the kernel
    /// at exec, so the helper reports the binary, not the shellder-askpass
    /// link it was started through).
    static func path(_ pid: pid_t) -> String? {
        var buf = [CChar](repeating: 0, count: 4096)
        return proc_pidpath(pid, &buf, UInt32(buf.count)) > 0 ? String(cString: buf) : nil
    }

    static func parent(_ pid: pid_t) -> pid_t? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        return pid_t(info.pbi_ppid)
    }

    /// The process at the other end of an accepted unix socket connection.
    /// The kernel answers this, the peer has no say in it.
    static func peer(of fd: Int32) -> pid_t? {
        var pid: pid_t = 0
        var len = socklen_t(MemoryLayout<pid_t>.size)
        guard getsockopt(fd, SOL_LOCAL, LOCAL_PEERPID, &pid, &len) == 0, pid > 0 else { return nil }
        return pid
    }

    /// Is this process's stdout an anonymous pipe? ssh gives the helper one
    /// end of a `pipe()` and reads the answer from the other, so a helper
    /// whose stdout is a file, a FIFO or a socket is writing the answer
    /// somewhere ssh will never look.
    static func stdoutIsPipe(_ pid: pid_t) -> Bool {
        let want = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, nil, 0)
        guard want > 0 else { return false }
        let count = Int(want) / MemoryLayout<proc_fdinfo>.size
        var buf = [proc_fdinfo](repeating: proc_fdinfo(), count: count)
        let got = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, &buf, want)
        guard got > 0 else { return false }
        for f in buf.prefix(Int(got) / MemoryLayout<proc_fdinfo>.size) where f.proc_fd == 1 {
            return f.proc_fdtype == UInt32(PROX_FDTYPE_PIPE)
        }
        return false
    }

    private static func same(_ a: String, _ b: String) -> Bool {
        a == b || URL(fileURLWithPath: a).resolvingSymlinksInPath().path
            == URL(fileURLWithPath: b).resolvingSymlinksInPath().path
    }

    /// The ssh binary, under SIP: nothing running as the user can replace it
    /// or make itself show up under this path.
    static let sshPath = "/usr/bin/ssh"
    /// ssh starts a ProxyJump/ProxyCommand connection through this shell.
    private static let shPath = "/bin/sh"
    private static let maxHops = 12

    enum Origin {
        /// The helper was forked by `ssh`, and that ssh descends from us.
        case ours(ssh: pid_t, chain: String)
        case rejected(String)
    }

    /// Decide whether an askpass helper really is one of ours.
    ///
    /// There is exactly one legitimate way for the helper to exist: ssh forked
    /// it and holds the pipe its answer goes into, and that ssh is a process
    /// this app started (directly for a master, or through `/bin/sh` and a
    /// second ssh for a jump host). So three things must hold: the parent is
    /// ssh itself, the answer goes into a pipe (not into a file a
    /// `ProxyCommand` redirected it to), and the chain above leads back to
    /// this very process.
    static func verifyHelper(_ helper: pid_t) -> Origin {
        guard let hp = path(helper) else { return .rejected("helper \(helper) is gone") }
        guard same(hp, Config.selfPath) else {
            return .rejected("helper \(helper) is \(hp), not \(Config.selfPath)")
        }
        guard let ssh = parent(helper), let sp = path(ssh) else {
            return .rejected("helper \(helper): no parent")
        }
        guard same(sp, sshPath) else {
            return .rejected("helper \(helper) was started by \(sp) (pid \(ssh)), not \(sshPath)")
        }
        guard stdoutIsPipe(helper) else {
            return .rejected("helper \(helper) does not write its answer into an ssh pipe")
        }
        var chain = "helper \(helper) ← ssh \(ssh)"
        var pid = ssh
        for _ in 0..<maxHops {
            guard let up = parent(pid), up > 1, let p = path(up) else { break }
            if up == getpid() { return .ours(ssh: ssh, chain: chain + " ← shellder \(up)") }
            guard same(p, sshPath) || same(p, shPath) else {
                return .rejected("\(chain) ← \(p) (pid \(up)), which is not one of ours")
            }
            chain += " ← \((p as NSString).lastPathComponent) \(up)"
            pid = up
        }
        return .rejected("\(chain): does not lead back to this app (pid \(getpid()))")
    }
}
