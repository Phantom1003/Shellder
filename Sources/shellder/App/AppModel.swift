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

/// The outcome of a one-off action on a host (a copy that failed), shown in
/// an alert: the title says what happened, the body is the output.
struct ActionResult: Identifiable {
    let id = UUID()
    let title: String
    let output: String
}

/// Everything the UI shows. Mutated on the main thread only; the daemon and
/// the askpass server hand their events over with DispatchQueue.main.
final class AppModel: ObservableObject {
    @Published private(set) var hosts: [HostEntry] = []
    @Published private(set) var resolved: [String: ResolvedHost] = [:]
    @Published private(set) var resolveErrors: [String: String] = [:]
    @Published private(set) var statuses: [String: HostStatus] = [:]
    /// The Connect switches. Not remembered across launches.
    @Published private(set) var enabled: Set<String> = []
    /// Locked hosts: reconnected after drops and switched on at launch.
    /// Always a subset of `enabled`. Remembered in the preferences.
    @Published private(set) var locked: Set<String> = []
    @Published private(set) var secretPresence: [String: Set<SecretKind>] = [:]
    @Published private(set) var configFiles: [String] = []
    @Published private(set) var lastReload: Date?
    @Published private(set) var reloading = false
    /// Hosts with an scp upload in flight.
    @Published private(set) var copying: Set<String> = []
    @Published private(set) var totpCache: [String: TOTP] = [:]
    /// Secrets the user asked to see in the Credentials section ("host|kind").
    @Published private(set) var revealed: [String: String] = [:]
    @Published private(set) var logLines: [String] = []
    @Published private(set) var pendingPrompts = 0

    @Published var selection: String?
    @Published var currentPrompt: PromptRequest?
    @Published var secretEdit: SecretEdit?
    @Published var actionResult: ActionResult?
    @Published var showLog: Bool = Prefs.showLogPanel {
        didSet {
            Prefs.showLogPanel = showLog
            if showLog { refreshLog() }
        }
    }

    // Settings
    @Published var keepInBackground = Prefs.keepInBackground {
        didSet {
            Prefs.keepInBackground = keepInBackground
            // A silent start only makes sense for an app that lives in the
            // background; without it there would be no window and no icon.
            if !keepInBackground && silentLaunch { silentLaunch = false }
            onSettingsChanged?()
        }
    }
    @Published var showMenuBarIcon = Prefs.showMenuBarIcon {
        didSet { Prefs.showMenuBarIcon = showMenuBarIcon; onSettingsChanged?() }
    }
    /// The menu bar item is only there while the app can outlive its windows.
    var menuBarIconShown: Bool { keepInBackground && showMenuBarIcon }
    /// Start silently: only offered while the app runs in the background.
    @Published var silentLaunch = Prefs.silentLaunch {
        didSet { Prefs.silentLaunch = silentLaunch }
    }
    @Published var language = Prefs.language {
        didSet { Prefs.language = language }
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
        Keychain.reownIfNeeded()
        locked = Set(Prefs.lockedHosts)
        enabled = locked

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
            self?.syncLockedFromPrefs()
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
                self.apply(enabled: self.enabled, locked: self.locked)
                Log.info("ssh config loaded: \(cat.entries.map { $0.alias }.joined(separator: ", "))")
            }
        }
    }

    /// `shellder lock/unlock HOST` from a terminal edits the preferences
    /// behind our back. Pick that up with the same rules as the lock buttons.
    private func syncLockedFromPrefs() {
        let now = Set(Prefs.lockedHosts)
        guard now != locked else { return }
        Log.info("locked set changed outside the app: \(now.sorted().joined(separator: ", "))")
        var e = enabled, l = locked
        for h in now.subtracting(locked) {
            e.insert(h)
            l.insert(h)
        }
        for h in locked.subtracting(now) {
            l.remove(h)
            l.subtract(dependents(of: h))
        }
        apply(enabled: e, locked: l)
    }

    // MARK: jump hosts

    /// The ProxyJump chain of `host` (nearest hop first), as far as it stays
    /// within the catalog.
    func jumpChain(_ host: String) -> [String] {
        var out: [String] = []
        var seen: Set<String> = [host]
        var h = host
        while let j = resolved[h]?.jumpAlias, seen.insert(j).inserted {
            out.append(j)
            h = j
        }
        return out
    }

    /// Hosts whose ProxyJump chain passes through `host`.
    func dependents(of host: String) -> [String] {
        hosts.map { $0.alias }.filter { jumpChain($0).contains(host) }
    }

    /// A switched-on (or locked) host needs its jump hosts the same way.
    private func withJumpHosts(_ set: Set<String>) -> Set<String> {
        var out = set
        for h in set { out.formUnion(jumpChain(h)) }
        return out
    }

    /// Make these the switched-on and the locked hosts, after the two rules
    /// every path shares: a locked host is switched on, and both take their
    /// jump hosts along. Stores the locks, logs the changes and pushes the
    /// host list to the daemon. The push happens even when the sets did not
    /// change, at launch and after a config reload the daemon needs the
    /// (re)resolved hosts anyway. The preferences are brought in line as
    /// well, a CLI edit the rules overrode included.
    private func apply(enabled e: Set<String>, locked l: Set<String>) {
        let newLocked = withJumpHosts(l)
        let newEnabled = withJumpHosts(e).union(newLocked)
        let stored = Set(Prefs.lockedHosts)
        for h in newLocked.subtracting(stored) { Prefs.setLocked(h, true) }
        for h in stored.subtracting(newLocked) { Prefs.setLocked(h, false) }
        let on = newEnabled.subtracting(enabled), off = enabled.subtracting(newEnabled)
        let lock = newLocked.subtracting(locked), unlock = locked.subtracting(newLocked)
        if enabled != newEnabled { enabled = newEnabled }
        if locked != newLocked { locked = newLocked }
        for h in hosts.map({ $0.alias }) {
            if on.contains(h) { Log.info("\(h): switched on") }
            else if off.contains(h) { Log.info("\(h): switched off") }
            if lock.contains(h) { Log.info("\(h): locked") }
            else if unlock.contains(h) { Log.info("\(h): unlocked") }
        }
        pushSpecs()
    }

    private func pushSpecs() {
        daemon.configure(hosts.map {
            HostSpec(alias: $0.alias, enabled: enabled.contains($0.alias), locked: locked.contains($0.alias),
                     resolved: resolved[$0.alias], resolveError: resolveErrors[$0.alias])
        })
    }

    // MARK: host actions

    func isEnabled(_ host: String) -> Bool { enabled.contains(host) }
    func isLocked(_ host: String) -> Bool { locked.contains(host) }
    /// Switched on, or kept up as the jump host of a switched-on host.
    func isKept(_ host: String) -> Bool {
        enabled.contains(host) || !(statuses[host]?.neededBy.isEmpty ?? true)
    }

    /// The Connect switch. On: one attempt for this host, its jump hosts
    /// switched on with it. Off: close it, unlock it, and take down the hosts
    /// that jump through it, which cannot stay up without it.
    func setEnabled(_ host: String, _ on: Bool) {
        if on {
            apply(enabled: enabled.union([host]), locked: locked)
        } else {
            let gone = Set([host] + dependents(of: host))
            apply(enabled: enabled.subtracting(gone), locked: locked.subtracting(gone))
        }
    }

    /// The lock. On: switch the host on if it is off, and lock it and its
    /// jump hosts. Off: unlock it and the hosts that jump through it (their
    /// lock would lock this one again). The connection stays as it is.
    func setLocked(_ host: String, _ on: Bool) {
        if on {
            apply(enabled: enabled.union([host]), locked: locked.union([host]))
        } else {
            apply(enabled: enabled, locked: locked.subtracting([host] + dependents(of: host)))
        }
    }

    /// The daemon flipped a switch itself (failed attempt, drop of an
    /// unlocked host, Close socket, Reconnect on an off host): mirror it in
    /// the preferences and the UI. The whole snapshot is taken over, not just
    /// this host: a jump host that failed turns its dependents off in the same
    /// breath, and pushing a stale set back would switch them on again for a
    /// moment.
    private func switchChangedByDaemon(_ host: String, reason: String?) {
        let snap = daemon.snapshot()
        apply(enabled: Set(snap.filter { $0.enabled }.map { $0.host }),
              locked: Set(snap.filter { $0.locked }.map { $0.host }))
    }

    /// Switch on = one connection attempt, off = stop.
    func connect(_ host: String) { setEnabled(host, true) }
    /// Also takes down the hosts that jump through this one (the daemon
    /// reports each switch it turns off).
    func disconnect(_ host: String) { daemon.disconnect(host) }
    func reconnect(_ host: String) { daemon.reconnect(host) }
    func closeSocket(_ host: String) { daemon.closeSocket(host) }
    func setIdleMode(_ host: String, _ mode: SSH.IdleMode) { daemon.setIdleMode(host, mode) }
    func connectAll() { daemon.connectAll() }
    func disconnectAll() { daemon.disconnectAll() }

    func status(_ host: String) -> HostStatus? { statuses[host] }

    /// Let the user pick files and folders, then scp them into the host's
    /// home directory through the master. One upload per host at a time.
    func copyFiles(_ host: String) {
        guard !copying.contains(host) else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        panel.resolvesAliases = true
        panel.message = L("Copy the selected files and folders to the home directory on \(host)")
        guard panel.runModal() == .OK, !panel.urls.isEmpty else { return }
        let paths = panel.urls.map(\.path)
        copying.insert(host)
        let what = paths.count == 1 ? paths[0].split(separator: "/").last.map(String.init) ?? paths[0] : "\(paths.count) items"
        Log.info("\(host): copying \(what) to the home directory")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let r = SSH.upload(paths, to: host)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.copying.remove(host)
                var body = (r.stderr + r.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
                if body.count > 2000 { body = String(body.suffix(2000)) }
                for line in body.split(separator: "\n") { Log.ssh(host, String(line)) }
                if r.status == 0 {
                    Log.info("\(host): copied \(what)")
                } else {
                    Log.error("\(host): copy failed with status \(r.status)")
                    if body.isEmpty { body = L("scp exited with status \(Int(r.status)). See the log for details.") }
                    self.actionResult = ActionResult(title: L("\(host): copy failed"), output: body)
                }
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

extension HostStatus {
    /// Off because an attempt failed or the link dropped: the dot stays red
    /// and the reason is shown, until the switch goes on again.
    var failed: Bool { state == .off && lastError != nil }

    /// The state, plus what an otherwise switched-off master is kept up for.
    var summary: String {
        let base = failed ? L("Failed") : state.label
        return neededBy.isEmpty ? base : L("\(base) · jump host for \(neededBy.joined(separator: ", "))")
    }
}

extension HostState {
    /// The state as a sentence start, for the dot's tooltip, the host page
    /// and the menu.
    var label: String {
        switch self {
        case .off: return L("Disconnected")
        case .up: return L("Connected")
        case .connecting: return L("Connecting…")
        case .foreign: return L("Connected (external master)")
        case .waiting(let r): return r > 0 ? L("Dropped — retrying in \(r)s") : L("Retrying…")
        case .waitingForJump(let j): return L("Waiting for jump host \(j)")
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
