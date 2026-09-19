import Foundation

struct SSHError: Error, CustomStringConvertible {
    let description: String
    init(_ d: String) { description = d }
}

enum SSH {
    static let binary = "/usr/bin/ssh"

    /// `-F file` only when a test config is in use; normally ssh reads
    /// ~/.ssh/config on its own.
    static var baseArgs: [String] { Config.configOverridden ? ["-F", Config.sshConfigFile] : [] }

    /// Environment that makes ssh call this binary (by its shellder-askpass
    /// name) for every prompt (SSH_ASKPASS_REQUIRE=force, OpenSSH >= 8.4). The
    /// helper talks to the running app over SHELLDER_SOCK; without the app it
    /// falls back to the keychain.
    static func askpassEnv(host: String) -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        env["SSH_ASKPASS"] = Config.askpassPath
        env["SSH_ASKPASS_REQUIRE"] = "force"
        env["SHELLDER_HOST"] = host
        env["SHELLDER_SOCK"] = Config.socketFile
        if env["DISPLAY"] == nil { env["DISPLAY"] = "shellder:0" }
        return env
    }

    struct Result {
        let status: Int32
        let stdout: String
        let stderr: String
    }

    /// Run ssh (or another OpenSSH client, scp) to completion, capturing output.
    @discardableResult
    static func run(_ args: [String], env: [String: String]? = nil, binary: String = binary) -> Result {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: binary)
        p.arguments = baseArgs + args
        if let env = env { p.environment = env }
        p.standardInput = FileHandle.nullDevice
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        do {
            try p.run()
        } catch {
            return Result(status: -1, stdout: "", stderr: "\(error)")
        }
        let o = out.fileHandleForReading.readDataToEndOfFile()
        let e = err.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        return Result(status: p.terminationStatus,
                      stdout: String(decoding: o, as: UTF8.self),
                      stderr: String(decoding: e, as: UTF8.self))
    }

    /// Resolved client configuration for `host` (`ssh -G`): lower-cased keys,
    /// every value in order of appearance (identityfile repeats).
    static func config(_ host: String) throws -> [String: [String]] {
        let r = run(["-G", host])
        guard r.status == 0 else {
            let msg = r.stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            throw SSHError(msg.isEmpty ? "ssh -G \(host) failed (rc \(r.status))" : msg)
        }
        var cfg: [String: [String]] = [:]
        for line in r.stdout.split(separator: "\n") {
            let parts = line.split(separator: " ", maxSplits: 1)
            guard let k = parts.first else { continue }
            cfg[k.lowercased(), default: []].append(parts.count > 1 ? String(parts[1]) : "")
        }
        return cfg
    }

    static func resolve(_ host: String) throws -> ResolvedHost {
        ResolvedHost(try config(host))
    }

    /// ControlPath from the user's ssh config, nil when unset.
    static func controlPath(_ host: String) throws -> String? {
        try resolve(host).controlPath
    }

    static func masterAlive(_ host: String) -> Bool {
        run(["-O", "check", host]).status == 0
    }

    /// pid of the master owning the host's socket ("Master running (pid=N)").
    static func masterPid(_ host: String) -> Int32? {
        let r = run(["-O", "check", host])
        guard r.status == 0,
              let re = try? NSRegularExpression(pattern: "pid=(\\d+)"),
              let m = re.firstMatch(in: r.stderr, range: NSRange(r.stderr.startIndex..., in: r.stderr)),
              let range = Range(m.range(at: 1), in: r.stderr) else { return nil }
        return Int32(r.stderr[range])
    }

    /// Marker put on every master we spawn so a later shellder can recognise
    /// one left behind by a crashed instance (`ssh -o Tag=shellder`, visible in ps).
    static let tag = "Tag=shellder"

    /// Kill masters left behind by an earlier shellder that did not shut down
    /// cleanly: tagged ssh processes whose parent is now launchd (ppid 1).
    /// Ones that already own a socket are replaced by the daemon; this sweep
    /// catches the rest (e.g. still stuck in authentication).
    static func killOrphanedMasters() {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/ps")
        p.arguments = ["-axo", "pid=,ppid=,command="]
        let out = Pipe()
        p.standardOutput = out
        p.standardError = FileHandle.nullDevice
        guard (try? p.run()) != nil else { return }
        let text = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        p.waitUntilExit()
        for line in text.split(separator: "\n") where line.contains(tag) {
            let f = line.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: true)
            guard f.count == 3, let pid = Int32(f[0]), let ppid = Int32(f[1]), ppid == 1 else { continue }
            Log.warn("orphaned shellder master pid \(pid) from an earlier run, killing: \(f[2].prefix(80))")
            kill(pid, SIGTERM)
        }
    }

    /// True when `pid` is an ssh master started by shellder (this or an earlier run).
    static func isOurMaster(pid: Int32) -> Bool {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/ps")
        p.arguments = ["-o", "command=", "-p", "\(pid)"]
        let out = Pipe()
        p.standardOutput = out
        p.standardError = FileHandle.nullDevice
        guard (try? p.run()) != nil else { return false }
        let text = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        p.waitUntilExit()
        return text.contains(tag)
    }

    /// Arguments that only concern the master process itself. Everything else
    /// (HostName, User, ProxyJump, ControlPath, …) comes from ~/.ssh/config.
    /// What the master does to stay connected.
    enum IdleMode: String, CaseIterable {
        /// `ssh -N`: no session at all. Cleanest, but some servers close it.
        case none
        /// An idle login shell on a pty: looks exactly like a person who left
        /// a terminal open (`w` shows an idle login). sshd sends it SIGHUP
        /// when the connection drops, so nothing is left behind.
        case shell
        /// `cat` blocked on a stdin pipe held open locally: no pty, no login
        /// scripts, exits on EOF when the connection drops.
        case cat

        var title: String {
            switch self {
            case .none: return "no session (-N)"
            case .shell: return "idle shell (pty)"
            case .cat: return "cat on stdin"
            }
        }

        /// For narrow controls.
        var short: String {
            switch self {
            case .none: return "-N"
            case .shell: return "idle shell"
            case .cat: return "cat"
            }
        }
    }

    static func masterArgs(_ host: String, resolved: ResolvedHost, mode: IdleMode) -> [String] {
        var args = [
            "-M",
            "-o", tag,                           // identifies our masters in ps
            "-o", "ControlPersist=no",          // we own the process; no daemonising
            "-o", "NumberOfPasswordPrompts=1",   // a wrong secret fails fast instead of looping
            "-o", "LogLevel=ERROR",
        ]
        if resolved.serverAliveInterval == 0 {
            // Detect a dead link so the master can be restarted; only applied
            // when the user's config does not set its own value.
            args += ["-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3"]
        }
        switch mode {
        case .none:
            args += ["-N", host]
        case .shell:
            // Force a pty even though we have no terminal; the login shell then
            // sits idle reading from it. X11 is left to the multiplexed
            // sessions, which request their own forwarding.
            args += ["-tt", "-o", "ForwardX11=no", host]
        case .cat:
            args += ["-T", "-o", "ForwardX11=no", host, "cat"]
        }
        return args
    }

    /// Last few KB of a master's stderr: copied to the log as it arrives and
    /// kept so the daemon can tell *why* ssh exited.
    final class OutputTail {
        private let lock = NSLock()
        private var buf = ""
        func append(_ s: String) {
            lock.lock(); defer { lock.unlock() }
            buf += s
            if buf.count > 4096 { buf = String(buf.suffix(4096)) }
        }
        var text: String { lock.lock(); defer { lock.unlock() }; return buf }
        /// Last non-empty line, trimmed.
        var lastLine: String? {
            text.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
                .last { !$0.isEmpty && !$0.hasPrefix("Warning: untrusted X11") }
        }
    }

    /// A pipe whose reader logs every complete line as "host: line".
    private static func linePipe(_ host: String, tail: OutputTail?) -> Pipe {
        let pipe = Pipe()
        var pending = ""
        pipe.fileHandleForReading.readabilityHandler = { h in
            let data = h.availableData
            if data.isEmpty {
                h.readabilityHandler = nil
                let rest = pending.trimmingCharacters(in: .whitespacesAndNewlines)
                if !rest.isEmpty { Log.ssh(host, rest) }
                return
            }
            let text = String(decoding: data, as: UTF8.self)
            tail?.append(text)
            pending += text
            while let nl = pending.firstIndex(where: { $0 == "\n" || $0 == "\r" }) {
                let line = pending[..<nl].trimmingCharacters(in: .whitespaces)
                pending = String(pending[pending.index(after: nl)...])
                if !line.isEmpty { Log.ssh(host, line) }
            }
        }
        return pipe
    }

    /// Spawn `ssh -M host` (with -N, an idle shell or cat) with this binary as SSH_ASKPASS.
    static func spawnMaster(_ host: String, resolved: ResolvedHost, mode: IdleMode, log: FileHandle) throws -> (Process, OutputTail) {
        guard let cp = resolved.controlPath else {
            throw SSHError("no ControlPath configured for \(host) in ~/.ssh/config")
        }
        let fm = FileManager.default
        try? fm.createDirectory(atPath: (cp as NSString).deletingLastPathComponent,
                                withIntermediateDirectories: true,
                                attributes: [.posixPermissions: 0o700])
        if fm.fileExists(atPath: cp) && !masterAlive(host) {
            Log.info("\(host): removing stale socket \(cp)")
            do {
                try fm.removeItem(atPath: cp)
            } catch {
                throw SSHError("cannot remove stale socket \(cp): \(error)")
            }
        }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: binary)
        p.arguments = baseArgs + masterArgs(host, resolved: resolved, mode: mode)
        p.environment = askpassEnv(host: host)
        // The idle session's stdin must stay open (never written to) so the
        // remote shell/cat keeps waiting; the Pipe lives as long as the Process.
        p.standardInput = mode == .none ? FileHandle.nullDevice : Pipe()
        // A login shell prints motd/profile chatter on the pty: keep that out
        // of the log. ssh's own diagnostics go to stderr.
        // ssh's own output goes to the log line by line, prefixed with the
        // host, and its stderr is also kept for the daemon to inspect.
        let tail = OutputTail()
        p.standardOutput = mode == .shell ? FileHandle.nullDevice : linePipe(host, tail: nil)
        p.standardError = linePipe(host, tail: tail)
        try p.run()
        Log.info("\(host): spawned master pid \(p.processIdentifier) [\(mode.title)] (socket \(cp))")
        return (p, tail)
    }

    /// Copy local files and directories into the host's home directory with
    /// scp, through the master. BatchMode so a missing master fails at once
    /// instead of waiting for an answer nobody will give.
    static func upload(_ paths: [String], to host: String) -> Result {
        run(["-r", "-q", "-o", "BatchMode=yes"] + paths + ["\(host):"], binary: "/usr/bin/scp")
    }

    /// One-shot login with the stored secrets, bypassing any socket.
    static func testLogin(_ host: String, verbose: Bool = false) -> Result {
        var args = ["-o", "ControlMaster=no", "-o", "ControlPath=none", "-o", "NumberOfPasswordPrompts=1"]
        if verbose { args.insert("-v", at: 0) }
        args += [host, "echo", "shellder: login OK"]
        return run(args, env: askpassEnv(host: host))
    }
}
