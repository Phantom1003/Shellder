import Foundation

/// Wire format between the askpass helper process and the running app.
struct AskpassRequest: Codable {
    var host: String
    var prompt: String
    var promptType: String?   // SSH_ASKPASS_PROMPT: "confirm" | "none" | nil
    var pid: Int32            // the helper
    var sshPid: Int32?        // the ssh process that spawned it
}

struct AskpassReply: Codable {
    var status: Int32
    var answer: String?
}

/// SSH_ASKPASS mode: ssh runs this binary with the prompt text as argv[1]
/// (SSH_ASKPASS_REQUIRE=force) and reads the answer from stdout.
enum Askpass {
    enum Kind: String {
        case password, passphrase, totp, confirm, unknown
    }

    private static let totpPattern =
        "verification code|one[- ]?time|otp|authenticat|passcode|token|2fa|two[- ]factor|\\bcode\\b|验证码|动态口令"

    private static func matches(_ pattern: String, _ s: String) -> Bool {
        s.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil
    }

    private static func strip(_ pattern: String, _ s: String) -> String {
        s.replacingOccurrences(of: pattern, with: "", options: [.regularExpression])
    }

    static func classify(_ prompt: String, promptType: String? = nil) -> Kind {
        if promptType == "confirm" { return .confirm }
        var p = prompt.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
        // Drop the user/host prefixes ssh adds ("(alice@otp-gw) Password:" /
        // "alice@otp-gw's password:") so host or user names cannot be mistaken
        // for a prompt type.
        p = strip("^\\([^)]*\\)\\s*", p)
        p = strip("^\\S+@\\S+'s\\s+", p)
        if p.contains("passphrase") { return .passphrase }
        if matches(totpPattern, p) { return .totp }
        if p.contains("password") || p.contains("密码") { return .password }
        if p.contains("yes/no") || p.contains("are you sure") { return .confirm }
        return .unknown
    }

    /// "user@host" that ssh put in front of a password prompt, if any.
    static func userHost(in prompt: String) -> (user: String, host: String)? {
        let patterns = ["^\\(([^@\\s()]+)@([^)\\s]+)\\)", "^([^@\\s]+)@(\\S+)'s\\s"]
        for pat in patterns {
            guard let re = try? NSRegularExpression(pattern: pat),
                  let m = re.firstMatch(in: prompt, range: NSRange(prompt.startIndex..., in: prompt)),
                  let ur = Range(m.range(at: 1), in: prompt), let hr = Range(m.range(at: 2), in: prompt)
            else { continue }
            return (String(prompt[ur]), String(prompt[hr]))
        }
        return nil
    }

    /// Key file named in "Enter passphrase for key '/path':".
    static func keyPath(in prompt: String) -> String? {
        guard let re = try? NSRegularExpression(pattern: "for key '([^']+)'"),
              let m = re.firstMatch(in: prompt, range: NSRange(prompt.startIndex..., in: prompt)),
              let r = Range(m.range(at: 1), in: prompt) else { return nil }
        return String(prompt[r])
    }

    static func run(prompt: String) -> Int32 {
        let env = ProcessInfo.processInfo.environment
        let host = env["SHELLDER_HOST"] ?? ""
        let ptype = env["SSH_ASKPASS_PROMPT"]
        let short = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\n", with: " | ")
        if ptype == "none" {
            Log.info("\(host): ssh says: \(short)")
            return 0
        }
        if let sock = env["SHELLDER_SOCK"] {
            let req = AskpassRequest(host: host, prompt: prompt, promptType: ptype, pid: getpid(), sshPid: getppid())
            if let reply = AskpassClient.ask(socket: sock, req) {
                if let a = reply.answer { print(a) }
                return reply.status
            }
            Log.warn("\(host): shellder app not reachable at \(sock); answering from the keychain directly")
        }
        return answerLocally(host: host, prompt: prompt, promptType: ptype)
    }

    /// Fallback when the app is not running (e.g. `shellder test` from a terminal).
    static func answerLocally(host: String, prompt: String, promptType: String?) -> Int32 {
        let kind = classify(prompt, promptType: promptType)
        Log.info("\(host): askpass prompt \"\(prompt.trimmingCharacters(in: .whitespacesAndNewlines))\" -> \(kind.rawValue)")
        switch kind {
        case .confirm:
            // Never accept host keys without a human looking at the fingerprint.
            Log.warn("\(host): host key confirmation needs the shellder app running; answering no")
            print("no")
            return 0
        case .unknown:
            Log.warn("\(host): unrecognised prompt, answering empty")
            print("")
            return 0
        case .password, .passphrase, .totp:
            let sk = SecretKind(rawValue: kind.rawValue)!
            guard let secret = Keychain.get(host, sk) else {
                Log.error("\(host): no \(sk.rawValue) stored — add it in the app or with `shellder add-secret \(host) \(sk.rawValue)`")
                return 1
            }
            if sk == .totp {
                do {
                    print(try TOTP.fresh(host: host, raw: secret))
                } catch {
                    Log.error("\(host): bad TOTP secret: \(error)")
                    return 1
                }
            } else {
                print(secret)
            }
            return 0
        }
    }
}

/// Helper-side client: one request, one reply, over a unix socket.
enum AskpassClient {
    static func ask(socket path: String, _ req: AskpassRequest) -> AskpassReply? {
        guard let fd = UnixSocket.connect(path) else { return nil }
        defer { close(fd) }
        // The user may be typing into a dialog: wait a long time, not forever.
        var tv = timeval(tv_sec: 900, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        guard let data = try? JSONEncoder().encode(req), UnixSocket.writeAll(fd, data + [0x0a]) else { return nil }
        guard let line = UnixSocket.readLine(fd) else { return nil }
        return try? JSONDecoder().decode(AskpassReply.self, from: Data(line.utf8))
    }
}

/// Minimal blocking unix-domain socket helpers (BSD sockets).
enum UnixSocket {
    static func address(_ path: String) -> sockaddr_un? {
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let capacity = MemoryLayout.size(ofValue: addr.sun_path)
        guard path.utf8.count < capacity else { return nil }
        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            let dst = UnsafeMutableRawPointer(ptr).assumingMemoryBound(to: CChar.self)
            path.withCString { src in _ = strncpy(dst, src, capacity - 1) }
        }
        return addr
    }

    static func connect(_ path: String) -> Int32? {
        guard var addr = address(path) else { return nil }
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        if rc != 0 { close(fd); return nil }
        return fd
    }

    static func listen(_ path: String) throws -> Int32 {
        guard var addr = address(path) else { throw SSHError("socket path too long: \(path)") }
        unlink(path)
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw SSHError("socket(): \(String(cString: strerror(errno)))") }
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard rc == 0 else {
            let e = String(cString: strerror(errno)); close(fd)
            throw SSHError("bind(\(path)): \(e)")
        }
        chmod(path, 0o600)
        guard Darwin.listen(fd, 16) == 0 else {
            let e = String(cString: strerror(errno)); close(fd)
            throw SSHError("listen(): \(e)")
        }
        return fd
    }

    @discardableResult
    static func writeAll(_ fd: Int32, _ data: Data) -> Bool {
        var off = 0
        while off < data.count {
            let n = data.withUnsafeBytes { buf -> Int in
                write(fd, buf.baseAddress!.advanced(by: off), data.count - off)
            }
            if n <= 0 { return false }
            off += n
        }
        return true
    }

    /// Read up to the first newline (1 MB cap). nil on error/EOF-before-newline.
    static func readLine(_ fd: Int32) -> String? {
        var buf = [UInt8](repeating: 0, count: 4096)
        var out = Data()
        while out.count < 1_048_576 {
            let n = read(fd, &buf, buf.count)
            if n <= 0 { break }
            out.append(contentsOf: buf[0..<n])
            if let nl = out.firstIndex(of: 0x0a) {
                return String(decoding: out[out.startIndex..<nl], as: UTF8.self)
            }
        }
        return nil
    }
}
