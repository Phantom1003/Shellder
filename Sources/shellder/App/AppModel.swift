import AppKit
import Combine
import Foundation
import LocalAuthentication

/// A question ssh asked that the stored credentials could not answer.
struct PromptRequest: Identifiable, Equatable {
    enum Kind { case password, passphrase, totp, confirm, other }
    let id = UUID()
    let host: String
    let kind: Kind
    let prompt: String
    let allowSave: Bool
}

struct PromptResponse {
    var answer: String?
    var save = false
}

struct SecretEdit: Identifiable {
    let id = UUID()
    let host: String
    let kind: SecretKind
}

struct TestResult: Identifiable {
    let id = UUID()
    let host: String
    let ok: Bool
    let output: String
}

/// Everything the UI shows. Mutated on the main thread only; the daemon and
/// the askpass server hand their events over with DispatchQueue.main.
final class AppModel: ObservableObject {
    @Published private(set) var hosts: [HostEntry] = []
    @Published private(set) var resolved: [String: ResolvedHost] = [:]
    @Published private(set) var resolveErrors: [String: String] = [:]
    @Published private(set) var statuses: [String: HostStatus] = [:]
    @Published private(set) var enabled: Set<String> = []
    @Published private(set) var secretPresence: [String: Set<SecretKind>] = [:]
    @Published private(set) var configFiles: [String] = []
    @Published private(set) var lastReload: Date?
    @Published private(set) var reloading = false
    @Published private(set) var testing: Set<String> = []
    @Published private(set) var totpCache: [String: TOTP] = [:]
    /// Secrets the user asked to see in the Credentials section ("host|kind").
    @Published private(set) var revealed: [String: String] = [:]
    @Published private(set) var logLines: [String] = []
    @Published private(set) var pendingPrompts = 0

    @Published var selection: String?
    @Published var currentPrompt: PromptRequest?
    @Published var secretEdit: SecretEdit?
    @Published var testResult: TestResult?
    @Published var showLog: Bool = Prefs.showLogPanel {
        didSet {
            Prefs.showLogPanel = showLog
            if showLog { refreshLog() }
        }
    }

    // Settings
    @Published var showDockIcon = Prefs.showDockIcon {
        didSet { Prefs.showDockIcon = showDockIcon; onSettingsChanged?() }
    }
    @Published var showMenuBarIcon = Prefs.showMenuBarIcon {
        didSet { Prefs.showMenuBarIcon = showMenuBarIcon; onSettingsChanged?() }
    }
    @Published var openWindowAtLaunch = Prefs.openWindowAtLaunch {
        didSet { Prefs.openWindowAtLaunch = openWindowAtLaunch }
    }
    @Published var startAtLogin = Launchd.installed {
        didSet {
            guard startAtLogin != Launchd.installed else { return }
            if startAtLogin {
                do { try Launchd.install(bootstrap: false) } catch {
                    settingsError = "\(error)"
                    startAtLogin = false
                }
            } else {
                Launchd.uninstall(bootout: false)
            }
        }
    }
    @Published var settingsError: String?

    let daemon = Daemon()
    let askpass = AskpassServer(path: Config.socketFile)

    /// Set by the AppDelegate: bring the main window forward (a prompt is waiting).
    var onNeedsAttention: (() -> Void)?
    var onSettingsChanged: (() -> Void)?

    private var promptQueue: [(PromptRequest, (PromptResponse) -> Void)] = []
    /// Stored secrets handed to a particular ssh process, keyed by
    /// "sshpid|host|kind": the same process asking the same question again
    /// means the server rejected the answer. (Another ssh — a ProxyJump hop,
    /// a retry — asking for the same secret is normal.)
    private var recentAnswers: [String: (Date, Int)] = [:]
    private let recentLock = NSLock()

    /// How often this ssh process was already given this secret (and record this one).
    private func repeatCount(ssh: Int32?, _ host: String, _ kind: SecretKind) -> Int {
        guard let ssh = ssh else { return 0 }
        recentLock.lock(); defer { recentLock.unlock() }
        let now = Date()
        recentAnswers = recentAnswers.filter { now.timeIntervalSince($0.value.0) < 600 }
        let k = "\(ssh)|\(host)|\(kind.rawValue)"
        let count = recentAnswers[k]?.1 ?? 0
        recentAnswers[k] = (now, count + 1)
        return count
    }
    private var mtimes: [String: Date?] = [:]
    private var timers: [Timer] = []
    private let work = DispatchQueue(label: Config.label + ".model", qos: .userInitiated)
    private var lastLogSignature: (UInt64, Date?) = (0, nil)

    // MARK: lifecycle

    func start() {
        Keychain.purgeLegacyItems()
        Keychain.adoptRenamedVault()
        Keychain.reownIfNeeded()
        enabled = Set(Prefs.enabledHosts)

        daemon.onChange = { [weak self] snap in
            DispatchQueue.main.async {
                self?.statuses = Dictionary(uniqueKeysWithValues: snap.map { ($0.host, $0) })
            }
        }
        daemon.onDisabled = { [weak self] host, reason in
            DispatchQueue.main.async { self?.switchChangedByDaemon(host, reason: reason) }
        }
        daemon.start()

        askpass.handler = { [weak self] req in
            self?.handleAskpass(req) ?? AskpassReply(status: 1, answer: nil)
        }
        do { try askpass.start() } catch {
            Log.error("askpass socket: \(error) — prompts will fall back to the keychain only")
        }

        reloadCatalog(force: true)
        timers.append(Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { [weak self] _ in
            self?.reloadCatalog(force: false)
            self?.syncEnabledFromPrefs()
        })
        timers.append(Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            guard let self = self, self.showLog else { return }
            self.refreshLog()
        })
        if showLog { refreshLog() }
    }

    func shutdown() {
        timers.forEach { $0.invalidate() }
        askpass.stop()
        daemon.shutdown()
    }

    // MARK: catalog

    /// Re-read ~/.ssh/config (and its includes) when any of them changed.
    func reloadCatalog(force: Bool) {
        var files = configFiles
        if !files.contains(Config.sshConfigFile) { files.insert(Config.sshConfigFile, at: 0) }
        let now = HostCatalog.modificationDates(files)
        if !force && now == mtimes && !hosts.isEmpty { return }
        if !force && now == mtimes && hosts.isEmpty && lastReload != nil { return }
        mtimes = now
        guard !reloading else { return }
        reloading = true
        work.async { [weak self] in
            let cat = HostCatalog.load()
            var res: [String: ResolvedHost] = [:]
            var errs: [String: String] = [:]
            for e in cat.entries {
                do { res[e.alias] = try SSH.resolve(e.alias) } catch { errs[e.alias] = "\(error)" }
            }
            let aliases = Set(cat.entries.map { $0.alias }) 
            for (alias, var r) in res {
                if let hop = r.firstJumpHost, aliases.contains(hop), hop != alias {
                    r.jumpAlias = hop
                    res[alias] = r
                }
            }
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.hosts = cat.entries
                self.configFiles = cat.files
                self.resolved = res
                self.resolveErrors = errs
                self.mtimes = HostCatalog.modificationDates(cat.files)
                self.lastReload = Date()
                self.reloading = false
                if let s = self.selection, !aliases.contains(s) { self.selection = nil }
                if self.selection == nil { self.selection = cat.entries.first?.alias }
                self.refreshSecrets()
                self.pushSpecs()
                Log.info("ssh config loaded: \(cat.entries.map { $0.alias }.joined(separator: ", "))")
            }
        }
    }

    /// `shellder enable/disable HOST` from a terminal edits the preferences
    /// behind our back; pick that up.
    private func syncEnabledFromPrefs() {
        let now = Set(Prefs.enabledHosts)
        guard now != enabled else { return }
        Log.info("keep-connected set changed outside the app: \(now.sorted().joined(separator: ", "))")
        enabled = now
        pushSpecs()
    }

    private func pushSpecs() {
        daemon.configure(hosts.map {
            HostSpec(alias: $0.alias, enabled: enabled.contains($0.alias),
                     resolved: resolved[$0.alias], resolveError: resolveErrors[$0.alias])
        })
    }

    // MARK: host actions

    func isEnabled(_ host: String) -> Bool { enabled.contains(host) }

    func setEnabled(_ host: String, _ on: Bool) {
        if on { enabled.insert(host) } else { enabled.remove(host) }
        Prefs.setEnabled(host, on)
        Log.info("\(host): keep connected \(on ? "on" : "off")")
        pushSpecs()
    }

    /// The daemon flipped a switch itself (failed first attempt, Close socket,
    /// Reconnect on an off host): mirror it in the preferences and the UI.
    private func switchChangedByDaemon(_ host: String, reason: String?) {
        let on = daemon.snapshot().first { $0.host == host }.map { $0.state != .off } ?? false
        if on { enabled.insert(host) } else { enabled.remove(host) }
        Prefs.setEnabled(host, on)
    }

    /// Switch on = one connection attempt; off = stop.
    func connect(_ host: String) { setEnabled(host, true) }
    func disconnect(_ host: String) { setEnabled(host, false) }
    func reconnect(_ host: String) { daemon.reconnect(host) }
    func closeSocket(_ host: String) { daemon.closeSocket(host) }
    func setIdleMode(_ host: String, _ mode: SSH.IdleMode) { daemon.setIdleMode(host, mode) }
    func connectAll() { daemon.connectAll() }
    func disconnectAll() { daemon.disconnectAll() }

    func status(_ host: String) -> HostStatus? { statuses[host] }

    func testLogin(_ host: String) {
        guard !testing.contains(host) else { return }
        testing.insert(host)
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let r = SSH.testLogin(host)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.testing.remove(host)
                var body = (r.stderr + r.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
                if body.count > 2000 { body = String(body.suffix(2000)) }
                if r.status != 0 && body.isEmpty { body = "ssh exited with status \(r.status). See the log for details." }
                self.testResult = TestResult(host: host, ok: r.status == 0, output: body)
            }
        }
    }

    // MARK: secrets

    func refreshSecrets() {
        var p: [String: Set<SecretKind>] = [:]
        for h in hosts {
            p[h.alias] = Set(SecretKind.allCases.filter { Keychain.has(h.alias, $0) })
        }
        secretPresence = p
    }

    func hasSecret(_ host: String, _ kind: SecretKind) -> Bool {
        secretPresence[host]?.contains(kind) ?? false
    }

    func setSecret(_ host: String, _ kind: SecretKind, _ value: String) throws {
        if kind == .totp { _ = try TOTP(parsing: value) }
        try Keychain.set(host, kind, value)
        Log.info("\(host): \(kind.rawValue) stored")
        totpCache[host] = nil
        revealed[revealKey(host, kind)] = nil
        refreshSecrets()
    }

    func removeSecret(_ host: String, _ kind: SecretKind) {
        Keychain.delete(host, kind)
        Log.info("\(host): \(kind.rawValue) removed")
        totpCache[host] = nil
        revealed[revealKey(host, kind)] = nil
        refreshSecrets()
    }

    func revealKey(_ host: String, _ kind: SecretKind) -> String { host + "|" + kind.rawValue }

    /// Show the stored value in the Credentials section (nil = hide). Showing
    /// plain text is gated by Touch ID (or the account password) when available.
    func setRevealed(_ host: String, _ kind: SecretKind, _ show: Bool) {
        let k = revealKey(host, kind)
        if !show { revealed[k] = nil; return }
        let ctx = LAContext()
        ctx.localizedCancelTitle = "Cancel"
        var err: NSError?
        guard ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: &err) else {
            if let v = Keychain.get(host, kind) { revealed[k] = v }
            return
        }
        ctx.evaluatePolicy(.deviceOwnerAuthentication,
                           localizedReason: "show the \(kind.title.lowercased()) stored for \(host)") { ok, _ in
            DispatchQueue.main.async {
                guard ok, let v = Keychain.get(host, kind) else { return }
                self.revealed[k] = v
            }
        }
    }

    func hideAllRevealed() { revealed = [:] }

    /// Load the TOTP secret once so the detail view can show live codes.
    @discardableResult
    func loadTOTP(_ host: String) -> Bool {
        if totpCache[host] != nil { return true }
        guard let raw = Keychain.get(host, .totp), let t = try? TOTP(parsing: raw) else { return false }
        totpCache[host] = t
        return true
    }

    // MARK: log panel

    func refreshLog() {
        guard let h = FileHandle(forReadingAtPath: Config.logFile) else { return }
        defer { h.closeFile() }
        let attrs = try? FileManager.default.attributesOfItem(atPath: Config.logFile)
        let size = (attrs?[.size] as? UInt64) ?? 0
        let mtime = attrs?[.modificationDate] as? Date
        if size == lastLogSignature.0 && mtime == lastLogSignature.1 { return }
        lastLogSignature = (size, mtime)
        let tail: UInt64 = 96 * 1024
        if size > tail { h.seek(toFileOffset: size - tail) }
        let text = String(decoding: h.readDataToEndOfFile(), as: UTF8.self)
        var lines = text.split(separator: "\n", omittingEmptySubsequences: true).map(String.init)
        if size > tail, !lines.isEmpty { lines.removeFirst() }   // partial first line
        if lines.count > 600 { lines = Array(lines.suffix(600)) }
        logLines = lines
    }

    // MARK: prompts (UI side, main thread)

    private func enqueuePrompt(_ req: PromptRequest, _ completion: @escaping (PromptResponse) -> Void) {
        promptQueue.append((req, completion))
        pendingPrompts = promptQueue.count
        if currentPrompt == nil { currentPrompt = req }
        onNeedsAttention?()
    }

    func answerPrompt(_ id: UUID, _ response: PromptResponse) {
        guard let idx = promptQueue.firstIndex(where: { $0.0.id == id }) else { return }
        let (_, completion) = promptQueue.remove(at: idx)
        pendingPrompts = promptQueue.count
        currentPrompt = promptQueue.first?.0
        completion(response)
    }


    // MARK: prompts (askpass side, background thread)

    private func askUser(_ req: PromptRequest) -> PromptResponse {
        let sem = DispatchSemaphore(value: 0)
        var result = PromptResponse(answer: nil)
        DispatchQueue.main.async {
            self.enqueuePrompt(req) { r in
                result = r
                sem.signal()
            }
        }
        sem.wait()
        return result
    }

    /// Which host's keychain entry a prompt belongs to. A ProxyJump hop runs
    /// with SHELLDER_HOST set to the final destination, so match the user@host
    /// ssh printed in the prompt against every resolved host first.
    private func credentialAlias(for req: AskpassRequest, kind: Askpass.Kind, secret: SecretKind) -> String {
        var alias = req.host
        DispatchQueue.main.sync {
            if kind == .passphrase, let key = Askpass.keyPath(in: req.prompt) {
                let candidates = [req.host] + self.hosts.map { $0.alias }
                for a in candidates where self.hasSecret(a, .passphrase) {
                    if a == req.host || self.resolved[a]?.identityFilesExpanded.contains(key) == true {
                        alias = a; return
                    }
                }
                return
            }
            guard let uh = Askpass.userHost(in: req.prompt) else { return }
            func matches(_ a: String) -> Bool {
                guard let r = self.resolved[a] else { return false }
                return r.user == uh.user && r.hostname.lowercased() == uh.host.lowercased()
            }
            // Prefer the requesting host if it matches and has the secret, then
            // any other host with the same user@hostname that has it, then any match.
            if matches(req.host) && self.hasSecret(req.host, secret) { return }
            if let a = self.hosts.map({ $0.alias }).first(where: { matches($0) && self.hasSecret($0, secret) }) { alias = a }
            else if matches(req.host) { return }
            else if let a = self.hosts.map({ $0.alias }).first(where: matches) { alias = a }
        }
        return alias
    }

    private func handleAskpass(_ req: AskpassRequest) -> AskpassReply {
        let kind = Askpass.classify(req.prompt, promptType: req.promptType)
        let verbatim = req.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\n", with: " ⏎ ")
        Log.prompt("\(req.host): ssh asks \"\(verbatim)\"  [classified as \(kind.rawValue)\(req.promptType.map { ", SSH_ASKPASS_PROMPT=\($0)" } ?? "")]")
        switch kind {
        case .password, .passphrase, .totp:
            let sk = SecretKind(rawValue: kind.rawValue)!
            let alias = credentialAlias(for: req, kind: kind, secret: sk)
            if let secret = Keychain.get(alias, sk) {
                // The same question again within two minutes means the server
                // rejected what we sent. A TOTP code may legitimately be asked
                // once more (already-used code); anything else is a bad secret.
                let repeats = repeatCount(ssh: req.sshPid, alias, sk)
                let allowed = sk == .totp ? 1 : 0
                if repeats > allowed {
                    Log.prompt("\(req.host): → ssh asked for the \(sk.rawValue) again; the stored \(sk.rawValue) for \(alias) was rejected by the server, not sending it again")
                    daemon.giveUp(req.host, reason: "stored \(sk.rawValue) for \(alias) rejected by the server")
                    return AskpassReply(status: 1, answer: nil)
                }
                if sk == .totp {
                    do {
                        let code = try TOTP.fresh(host: alias, raw: secret)
                        let t = try TOTP(parsing: secret)
                        Log.prompt("\(req.host): → answered with a TOTP code generated from the \(alias) secret (\(t.secondsRemaining)s left in this window)")
                        return AskpassReply(status: 0, answer: code)
                    } catch {
                        Log.error("\(alias): bad TOTP secret: \(error)")
                    }
                } else {
                    Log.prompt("\(req.host): → answered with the \(sk.rawValue) stored for \(alias)")
                    return AskpassReply(status: 0, answer: secret)
                }
            }
            Log.prompt("\(req.host): → no \(sk.rawValue) stored for \(alias); asking the user")
            let pk: PromptRequest.Kind = sk == .password ? .password : sk == .passphrase ? .passphrase : .totp
            let resp = askUser(PromptRequest(host: alias, kind: pk, prompt: req.prompt, allowSave: sk != .totp))
            guard let answer = resp.answer else {
                Log.prompt("\(req.host): → user cancelled; this attempt fails")
                daemon.giveUp(req.host, reason: "\(sk.title.lowercased()) prompt cancelled")
                return AskpassReply(status: 1, answer: nil)
            }
            if resp.save {
                do {
                    try Keychain.set(alias, sk, answer)
                    Log.prompt("\(req.host): → user answered, saved to the keychain as \(sk.rawValue) for \(alias)")
                    DispatchQueue.main.async { self.refreshSecrets() }
                } catch {
                    Log.error("\(alias): could not save \(sk.rawValue): \(error)")
                }
            } else {
                Log.prompt("\(req.host): → user answered, not saved")
            }
            return AskpassReply(status: 0, answer: answer)
        case .confirm:
            Log.prompt("\(req.host): → host key confirmation; asking the user")
            let resp = askUser(PromptRequest(host: req.host, kind: .confirm, prompt: req.prompt, allowSave: false))
            let yes = resp.answer == "yes"
            Log.prompt("\(req.host): → user answered \(yes ? "yes" : "no")")
            if !yes { daemon.giveUp(req.host, reason: "host key not accepted") }
            return AskpassReply(status: 0, answer: yes ? "yes" : "no")
        case .unknown:
            Log.prompt("\(req.host): → unrecognised prompt; asking the user")
            let resp = askUser(PromptRequest(host: req.host, kind: .other, prompt: req.prompt, allowSave: false))
            guard let answer = resp.answer else {
                Log.prompt("\(req.host): → user cancelled; this attempt fails")
                daemon.giveUp(req.host, reason: "prompt cancelled")
                return AskpassReply(status: 1, answer: nil)
            }
            Log.prompt("\(req.host): → user answered")
            return AskpassReply(status: 0, answer: answer)
        }
    }
}

// MARK: - presentation helpers

extension HostState {
    var label: String {
        switch self {
        case .off: return "not connected"
        case .up: return "connected"
        case .connecting: return "connecting…"
        case .foreign: return "connected (external master)"
        case .waiting(let r): return r > 0 ? "dropped — retrying in \(r)s" : "retrying…"
        case .waitingForJump(let j): return "waiting for jump host \(j)"
        case .error(let e): return e
        }
    }

    var symbol: String {
        switch self {
        case .off: return "circle"
        case .up, .foreign: return "circle.fill"
        case .connecting, .waitingForJump: return "circle.dotted"
        case .waiting: return "arrow.clockwise.circle"
        case .error: return "exclamationmark.triangle.fill"
        }
    }
}

enum Fmt {
    static func duration(_ t: TimeInterval) -> String {
        let s = Int(max(0, t))
        if s < 60 { return "\(s)s" }
        if s < 3600 { return "\(s / 60)m \(s % 60)s" }
        if s < 86400 { return "\(s / 3600)h \((s % 3600) / 60)m" }
        return "\(s / 86400)d \((s % 86400) / 3600)h"
    }
}
