import Foundation

/// Listens on a unix socket for askpass helpers spawned by our ssh masters.
/// Each request is answered by `handler` on a background thread; it may
/// block for as long as it likes (keychain lookup, waiting for the user).
final class AskpassServer {
    typealias Handler = (AskpassRequest) -> AskpassReply

    let path: String
    var handler: Handler?
    private var fd: Int32 = -1

    init(path: String) { self.path = path }

    func start() throws {
        Config.ensureDirs()
        fd = try UnixSocket.listen(path)
        let t = Thread { [weak self] in self?.acceptLoop() }
        t.name = "shellder.askpass"
        t.start()
        Log.info("askpass socket listening at \(path)")
    }

    func stop() {
        if fd >= 0 { close(fd); fd = -1 }
        unlink(path)
    }

    private func acceptLoop() {
        while fd >= 0 {
            let c = accept(fd, nil, nil)
            if c < 0 {
                if errno == EINTR { continue }
                break
            }
            let t = Thread { [weak self] in self?.serve(c) }
            t.name = "shellder.askpass.conn"
            t.start()
        }
    }

    private func serve(_ c: Int32) {
        defer { close(c) }
        guard let line = UnixSocket.readLine(c),
              var req = try? JSONDecoder().decode(AskpassRequest.self, from: Data(line.utf8)) else {
            Log.warn("askpass: malformed request")
            return
        }
        // Anything running as this user can open the socket and ask for a
        // secret, so the request only counts if the kernel agrees it comes
        // from an ssh we started. What the request says about itself (its own
        // pid, the ssh pid) is not evidence, so the verified ssh replaces it.
        let reply: AskpassReply
        switch Proc.peer(of: c).map(Proc.verifyHelper) ?? .rejected("no peer pid on the connection") {
        case .ours(let ssh, let chain):
            req.sshPid = ssh
            Log.info("askpass: request from \(chain)")
            reply = handler?(req) ?? AskpassReply(status: 1, answer: nil)
        case .rejected(let why):
            Log.warn("askpass: REFUSED a request for \(req.host): \(why)")
            reply = AskpassReply(status: 1, answer: nil)
        }
        if let data = try? JSONEncoder().encode(reply) {
            UnixSocket.writeAll(c, data + [0x0a])
        }
    }
}
