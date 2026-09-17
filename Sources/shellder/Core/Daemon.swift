import Foundation

enum HostState: Equatable {
    case off                        // switch off, nothing running
    case up(pid: Int32)
    case connecting
    case foreign                    // a master we did not start owns the socket
    case waiting(retryIn: Int)      // dropped after a successful connection; retrying
    case waitingForJump(String)
    case error(String)              // cannot even try (no ControlPath, config error)

    var isUp: Bool {
        switch self {
        case .up, .foreign: return true
        default: return false
        }
    }

    var isRunning: Bool {
        switch self {
        case .up, .connecting: return true
        default: return false
        }
    }
}

struct HostStatus: Equatable {
    let host: String
    let state: HostState
    let since: Date?
    let lastError: String?
    let quickFailures: Int
    let idleMode: SSH.IdleMode      // how the master stays connected
    /// Switched-on hosts whose ProxyJump goes through this one. The master is
    /// kept up for them even while its own switch is off.
    let neededBy: [String]
}

struct HostSpec: Equatable {
    let alias: String
    var enabled: Bool
    var resolved: ResolvedHost?
    var resolveError: String?
}

final class Master {
    var spec: HostSpec
    var host: String { spec.alias }
    var proc: Process?
    var tail: SSH.OutputTail?
    var started = Date.distantPast
    var establishedAt: Date?
    var backoff = Config.backoffMin
    var nextTry = Date.distantPast
    var lastCheck = Date.distantPast
    var nextProbe = Date.distantPast
    var quickFailures = 0
    var foreign = false
    var lastError: String?
    var fatal: String?
    var waitingForJump: String?
    /// Hosts (switched on, or themselves needed) whose first ProxyJump hop is
    /// this host. Recomputed by the daemon before every tick.
    var neededBy: [String] = []
    /// `wanted` as of the previous tick, to notice the implicit transitions.
    var wasWanted = false
    /// True once a connection succeeded since the switch was turned on.
    /// Before that a failure turns the switch off; after that we retry.
    var everEstablished = false
    /// Some servers drop a session-less (-N) connection within seconds; the
    /// master then escalates none -> shell -> cat (remembered per host).
    var idleMode: SSH.IdleMode

    init(_ spec: HostSpec) {
        self.spec = spec
        idleMode = Prefs.idleMode(spec.alias).flatMap(SSH.IdleMode.init(rawValue:)) ?? .none
    }

    var established: Bool { establishedAt != nil }
    var running: Bool { proc?.isRunning ?? false }
    /// Kept connected: switched on, or a jump host some wanted host goes through.
    var wanted: Bool { spec.enabled || !neededBy.isEmpty }
    /// Kept connected only because other hosts route through it.
    var implicit: Bool { !spec.enabled && !neededBy.isEmpty }

    /// Schedule the next attempt after a failure.
    func failed(quick: Bool) {
        if quick {
            quickFailures += 1
            backoff = quickFailures >= Config.maxQuickFailures
                ? Config.backoffMax : min(backoff * 2, Config.backoffMax)
        } else {
            quickFailures = 0
            backoff = Config.backoffMin
        }
        nextTry = Date().addingTimeInterval(backoff)
    }

    /// Fresh start (switch turned on, Reconnect pressed).
    func reset() {
        quickFailures = 0
        backoff = Config.backoffMin
        nextTry = .distantPast
        fatal = nil
        lastError = nil
        waitingForJump = nil
        everEstablished = false
    }

    func stop() {
        if let p = proc, p.isRunning {
            Log.info("\(host): stopping master pid \(p.processIdentifier)")
            p.terminate()
            let deadline = Date().addingTimeInterval(5)
            while p.isRunning && Date() < deadline {
                Thread.sleep(forTimeInterval: 0.1)
            }
            if p.isRunning { kill(p.processIdentifier, SIGKILL) }
        }
        proc = nil
        establishedAt = nil
    }

    var status: HostStatus {
        let state: HostState
        if let p = proc, p.isRunning {
            state = established ? .up(pid: p.processIdentifier) : .connecting
        } else if foreign {
            state = .foreign
        } else if !wanted {
            state = .off
        } else if let f = fatal {
            state = .error(f)
        } else if let j = waitingForJump {
            state = .waitingForJump(j)
        } else {
            state = .waiting(retryIn: max(0, Int(nextTry.timeIntervalSinceNow.rounded(.up))))
        }
        return HostStatus(host: host, state: state,
                          since: establishedAt ?? (running ? started : nil),
                          lastError: lastError, quickFailures: quickFailures, idleMode: idleMode,
                          neededBy: neededBy)
    }
}

/// Keeps one ControlMaster per switched-on host alive. All state lives on a
/// private serial queue; the UI reads snapshots via `onChange`.
///
/// Switch semantics: turning a host on is one connection attempt. If that
/// attempt fails the switch goes back off (`onDisabled`) and nothing is
/// retried. Once a connection has succeeded, drops are retried with back-off;
/// a link that keeps dying right after connecting gives up after a few tries.
final class Daemon {
    private let queue = DispatchQueue(label: Config.label + ".daemon")
    private var timer: DispatchSourceTimer?
    private var masters: [String: Master] = [:]
    private var order: [String] = []
    private var logHandle: FileHandle?
    private var lastSnapshot: [HostStatus] = []

    /// Called on the daemon queue after every tick in which something changed.
    var onChange: (([HostStatus]) -> Void)?
    /// The daemon turned a host's switch off (reason nil = user action).
    var onDisabled: ((String, String?) -> Void)?

    func start() {
        Config.ensureDirs()
        logHandle = Log.appendHandle()
        Log.info("daemon starting (pid \(getpid()))")
        SSH.killOrphanedMasters()
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now(), repeating: Config.pollInterval, leeway: .milliseconds(200))
        t.setEventHandler { [weak self] in self?.tickAll() }
        t.resume()
        timer = t
    }

    /// Stop every master. Safe to call from any thread; returns when done.
    func shutdown() {
        queue.sync {
            timer?.cancel()
            timer = nil
            Log.info("daemon stopping")
            for m in masters.values { m.stop() }
        }
    }

    /// Replace the host list. Called whenever ~/.ssh/config or the enabled
    /// set changes; existing masters keep running when nothing relevant changed.
    func configure(_ specs: [HostSpec]) {
        queue.async {
            for spec in specs {
                if let m = self.masters[spec.alias] {
                    let was = m.spec
                    m.spec = spec
                    if was.enabled && !spec.enabled {
                        if m.neededBy.isEmpty {
                            m.stop()
                            m.foreign = false
                            m.nextProbe = .distantPast
                        } else {
                            Log.info("\(spec.alias): switched off, kept up as the jump host for \(m.neededBy.joined(separator: ", "))")
                        }
                    } else if !was.enabled && spec.enabled {
                        if m.running {
                            Log.info("\(spec.alias): switched on, already connected as a jump host")
                        } else {
                            m.reset()
                            Log.info("\(spec.alias): switched on, connecting")
                        }
                    } else if was.resolved != spec.resolved || was.resolveError != spec.resolveError {
                        m.fatal = nil
                    }
                } else {
                    self.masters[spec.alias] = Master(spec)
                }
            }
            let keep = Set(specs.map { $0.alias })
            for (alias, m) in self.masters where !keep.contains(alias) {
                m.stop()
                self.masters.removeValue(forKey: alias)
            }
            self.order = specs.map { $0.alias }
            self.tickAll()
        }
    }

    /// Stop and start again (keeps the switch on). Takes over a socket that
    /// belongs to a master started outside shellder.
    func reconnect(_ host: String) {
        queue.async {
            guard let m = self.masters[host] else { return }
            m.stop()
            if m.foreign {
                let r = SSH.run(["-O", "exit", host])
                Log.info("\(host): closing external master (ssh -O exit rc \(r.status))")
                m.foreign = false
            }
            m.reset()
            if !m.wanted {
                m.spec.enabled = true
                self.onDisabled?(host, nil)   // model mirrors the switch state
            }
            Log.info("\(host): reconnect requested")
            self.tickAll()
        }
    }

    /// Disconnect and turn the switch off. Hosts that reach the network
    /// through this one (ProxyJump) cannot stay up without it, so they are
    /// switched off as well.
    func disconnect(_ host: String) {
        queue.async {
            guard let m = self.masters[host] else { return }
            self.stopWithDependents(m)
            self.tickAll()
        }
    }

    /// `ssh -O exit`: asks whoever owns the socket (our master or one started
    /// by an interactive ssh) to shut down, and turns the switch off. Hosts
    /// jumping through it go down with it.
    func closeSocket(_ host: String) {
        queue.async {
            guard let m = self.masters[host] else { return }
            let r = SSH.run(["-O", "exit", host])
            let msg = (r.stderr + r.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
            Log.info("\(host): ssh -O exit -> rc \(r.status)\(msg.isEmpty ? "" : " (\(msg))")")
            self.stopWithDependents(m)
            self.tickAll()
        }
    }

    private func stopWithDependents(_ m: Master) {
        for d in dependents(of: m) where d.wanted {
            Log.info("\(d.host): disconnected together with its jump host \(m.host)")
            d.stop()
            d.foreign = false
            turnOff(d, reason: nil)
        }
        m.stop()
        m.foreign = false
        m.nextProbe = .distantPast
        turnOff(m, reason: nil)
    }

    /// Masters whose ProxyJump chain passes through `m` (nearest hop first).
    private func dependents(of m: Master) -> [Master] {
        var out: [Master] = []
        var seen: Set<String> = [m.host]
        var frontier = [m.host]
        while let j = frontier.popLast() {
            for h in order {
                guard let d = masters[h], !seen.contains(h), d.spec.resolved?.jumpAlias == j else { continue }
                seen.insert(h)
                out.append(d)
                frontier.append(h)
            }
        }
        return out
    }

    /// A prompt was cancelled or a host key declined: this attempt failed.
    func giveUp(_ host: String, reason: String) {
        queue.async {
            guard let m = self.masters[host] else { return }
            m.stop()
            self.turnOff(m, reason: reason)
            self.tickAll()
        }
    }

    /// User picked an idle mode explicitly; restart the master if it is running.
    func setIdleMode(_ host: String, _ mode: SSH.IdleMode) {
        queue.async {
            guard let m = self.masters[host], m.idleMode != mode else { return }
            m.idleMode = mode
            Prefs.setIdleMode(host, mode == .none ? nil : mode.rawValue)
            Log.info("\(host): idle mode set to \(mode.title)")
            if m.running {
                m.stop()
                m.reset()
                m.everEstablished = false
                self.tickAll()
            }
        }
    }

    func connectAll() {
        queue.async {
            for m in self.masters.values where m.spec.enabled && !m.running { m.reset() }
            Log.info("connect all requested")
            self.tickAll()
        }
    }

    func disconnectAll() {
        queue.async {
            for m in self.masters.values where m.spec.enabled {
                m.stop()
                m.foreign = false
                self.turnOff(m, reason: nil)
            }
            Log.info("disconnect all requested")
            self.tickAll()
        }
    }

    func snapshot() -> [HostStatus] {
        queue.sync { order.compactMap { masters[$0]?.status } }
    }

    // MARK: - internals (daemon queue only)

    private func turnOff(_ m: Master, reason: String?) {
        if let r = reason {
            m.lastError = r
            Log.warn("\(m.host): \(m.spec.enabled ? "switched off" : "giving up") — \(r)")
            // Hosts held back for this jump host fail the same way. Ones
            // already connected through it notice the drop themselves.
            for d in dependents(of: m) where d.wanted && !d.running && !d.foreign
                && d.spec.resolved?.jumpAlias == m.host {
                let why = "jump host \(m.host): \(r)"
                if d.everEstablished {
                    d.lastError = why
                    d.failed(quick: true)
                    Log.warn("\(d.host): \(why), retry in \(Int(d.backoff))s")
                } else {
                    turnOff(d, reason: why)
                }
            }
        }
        m.everEstablished = false
        m.waitingForJump = nil
        guard m.spec.enabled else { return }
        m.spec.enabled = false
        onDisabled?(m.host, reason)
    }

    /// Work out which hosts are needed as jump hosts and start or stop the
    /// masters that are kept up only for that reason.
    private func reconcileJumpHosts() {
        var needed: [String: [String]] = [:]
        var visited = Set<String>()
        var stack = order.filter { masters[$0]?.spec.enabled == true }
        while let h = stack.popLast() {
            guard visited.insert(h).inserted, let m = masters[h] else { continue }
            if let j = m.spec.resolved?.jumpAlias, j != h, masters[j] != nil {
                needed[j, default: []].append(h)
                stack.append(j)
            }
        }
        for h in order {
            guard let m = masters[h] else { continue }
            let by = needed[h] ?? []
            m.neededBy = order.filter { by.contains($0) }
            let wanted = m.wanted
            if wanted && !m.wasWanted && m.implicit {
                m.reset()
                Log.info("\(h): needed as the jump host for \(m.neededBy.joined(separator: ", ")), connecting")
            } else if !wanted && m.wasWanted && !m.spec.enabled {
                if m.running { Log.info("\(h): no longer needed as a jump host, stopping") }
                m.stop()
                m.foreign = false
                m.nextProbe = .distantPast
            }
            m.wasWanted = wanted
        }
    }

    private func tickAll() {
        reconcileJumpHosts()
        for h in order {
            if let m = masters[h] { tick(m) }
        }
        let snap = order.compactMap { masters[$0]?.status }
        if snap != lastSnapshot {
            lastSnapshot = snap
            onChange?(snap)
        }
    }

    private func tick(_ m: Master) {
        let now = Date()
        if let p = m.proc {
            if p.isRunning {
                if !m.established {
                    // Still authenticating: the socket only appears once auth
                    // succeeds, so its absence is not a fault yet.
                    if SSH.masterAlive(m.host) {
                        m.establishedAt = now
                        m.everEstablished = true
                        m.quickFailures = 0
                        m.backoff = Config.backoffMin
                        m.lastCheck = now
                        m.lastError = nil
                        Log.info("\(m.host): master established (pid \(p.processIdentifier)) [\(m.idleMode.title)]")
                    } else if now.timeIntervalSince(m.started) > Config.connectTimeout {
                        Log.warn("\(m.host): master pid \(p.processIdentifier) did not come up in \(Int(Config.connectTimeout))s, killing")
                        m.stop()
                        if m.everEstablished {
                            m.lastError = "timed out while reconnecting"
                            m.failed(quick: true)
                        } else {
                            m.failed(quick: true)
                            turnOff(m, reason: "timed out while connecting")
                        }
                    }
                } else if now.timeIntervalSince(m.lastCheck) >= Config.healthInterval {
                    m.lastCheck = now
                    if !SSH.masterAlive(m.host) {
                        Log.warn("\(m.host): master pid \(p.processIdentifier) unresponsive, killing")
                        m.stop()
                        m.lastError = "master stopped responding"
                        m.failed(quick: false)
                    }
                }
                return
            }
            // exited
            let life = now.timeIntervalSince(m.started)
            let rc = p.terminationStatus
            let wasEstablished = m.established
            m.proc = nil
            m.establishedAt = nil
            guard m.wanted else {
                Log.info("\(m.host): master exited rc=\(rc) after \(Int(life))s")
                return
            }
            if !wasEstablished {
                let why = m.tail?.lastLine ?? "ssh exited with status \(rc)"
                if m.everEstablished {
                    // Reconnect after a drop failed (network still down?): keep trying.
                    m.lastError = "reconnect failed: \(why)"
                    m.failed(quick: true)
                    Log.warn("\(m.host): reconnect failed rc=\(rc); retry in \(Int(m.backoff))s")
                } else {
                    m.failed(quick: true)
                    turnOff(m, reason: "could not connect: \(why)")
                }
                return
            }
            let uptime = Int(life - (m.establishedAt.map { now.timeIntervalSince($0) } ?? 0))
            let serverClosed = m.tail?.text.contains("closed by remote host") ?? false
            let detail = m.tail?.lastLine.map { " — \($0)" } ?? ""
            if life < Config.shortLife && serverClosed, let next = m.idleMode.next {
                // Authenticated, then dropped within seconds: the server most
                // likely rejects this kind of idle connection. Escalate
                // -N -> idle shell -> cat and remember what worked.
                m.idleMode = next
                Prefs.setIdleMode(m.host, next.rawValue)
                m.quickFailures += 1
                m.nextTry = now.addingTimeInterval(Config.backoffMin)
                m.lastError = "server closed the connection after \(Int(life))s; retrying with \(next.title)"
                Log.warn("\(m.host): \(m.lastError!)")
                return
            }
            m.failed(quick: life < Config.shortLife)
            if m.quickFailures >= Config.maxQuickFailures {
                turnOff(m, reason: "connection keeps dropping right after connecting (rc \(rc))\(detail)")
            } else {
                m.lastError = "connection dropped after \(Fmt.duration(life)) (rc \(rc))\(detail)"
                Log.warn("\(m.host): master exited rc=\(rc) after \(Int(life))s (\(uptime)s up); retry in \(Int(m.backoff))s")
            }
            return
        }

        guard m.wanted else {
            // Idle host: still notice a master the user started interactively.
            m.waitingForJump = nil
            if now >= m.nextProbe {
                m.nextProbe = now.addingTimeInterval(Config.healthInterval)
                if m.spec.resolved?.controlPath != nil {
                    m.foreign = SSH.masterAlive(m.host)
                } else {
                    m.foreign = false
                }
            }
            return
        }
        if now < m.nextTry { return }

        // Bring the jump host up first (reconcileJumpHosts made it wanted)
        // so the ProxyJump hop goes through our master instead of starting a
        // throw-away one of its own. A jump host that cannot be managed at
        // all (fatal: no ControlPath, config error) is left to ssh.
        if let j = m.spec.resolved?.jumpAlias, let jm = masters[j], jm !== m,
           jm.wanted, jm.fatal == nil, !jm.status.state.isUp {
            if m.waitingForJump == nil { Log.info("\(m.host): waiting for jump host \(j)") }
            m.waitingForJump = j
            return
        }
        m.waitingForJump = nil

        if let err = m.spec.resolveError {
            m.fatal = "ssh config error: \(err)"
            turnOff(m, reason: m.fatal)
            return
        }
        guard let resolved = m.spec.resolved else {
            m.nextTry = now.addingTimeInterval(Config.pollInterval)   // still resolving
            return
        }
        guard resolved.controlPath != nil else {
            m.fatal = "no ControlPath configured for this host in ~/.ssh/config"
            turnOff(m, reason: m.fatal)
            return
        }
        if let pid = SSH.masterPid(m.host) {
            if SSH.isOurMaster(pid: pid) {
                // Left behind by an earlier shellder that did not shut down
                // cleanly (crash, kill -9). Replace it with one we control.
                Log.warn("\(m.host): socket owned by an orphaned shellder master (pid \(pid)); replacing it")
                let r = SSH.run(["-O", "exit", m.host])
                if r.status != 0 { kill(pid, SIGTERM) }
                Thread.sleep(forTimeInterval: 0.5)
            } else {
                // Some other master (e.g. ControlMaster=auto from an interactive
                // ssh) owns the socket; leave it alone until it goes away.
                if !m.foreign { Log.info("\(m.host): socket owned by another master (pid \(pid)), not spawning") }
                m.foreign = true
                m.everEstablished = true
                m.nextTry = now.addingTimeInterval(Config.healthInterval)
                return
            }
        }
        m.foreign = false
        do {
            let (proc, tail) = try SSH.spawnMaster(m.host, resolved: resolved, mode: m.idleMode,
                                                   log: logHandle ?? FileHandle.nullDevice)
            m.proc = proc
            m.tail = tail
            m.started = now
            m.lastCheck = now
            m.establishedAt = nil
            m.fatal = nil
        } catch {
            Log.error("\(m.host): \(error)")
            m.proc = nil
            if m.everEstablished {
                m.lastError = "\(error)"
                m.failed(quick: true)
            } else {
                turnOff(m, reason: "\(error)")
            }
        }
    }
}
