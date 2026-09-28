import Foundation
import Security

enum SecretKind: String, CaseIterable {
    case password, passphrase, totp

    var title: String {
        switch self {
        case .password: return "Password"
        case .passphrase: return "Key passphrase"
        case .totp: return "TOTP secret"
        }
    }
}

struct KeychainError: Error, CustomStringConvertible {
    let status: OSStatus
    init(_ s: OSStatus) { status = s }
    var description: String {
        if let m = SecCopyErrorMessageString(status, nil) as String? { return m }
        return "OSStatus \(status)"
    }
}

/// All credentials live in ONE login-keychain item, the "shellder vault": a JSON
/// object {host: {password|passphrase|totp: secret}}. One item means the
/// keychain asks for permission at most once per signature: "Always Allow"
/// adds the app to the item's access list and its Team ID to the partition
/// list, and that answer survives rebuilds signed with the same identity.
/// The item is never deleted or re-created by the app: a delete of an item
/// another signature made is a second password dialog, and a fresh item
/// trusts only its creator, which is what made two copies of the app with
/// different signatures ask each other's password on every switch.
enum Keychain {
    typealias Vault = [String: [String: String]]

    static let vaultService = "\(Config.app):vault"
    static let vaultAccount = Config.app

    private static let lock = NSLock()
    /// The vault as last read or written. It does not expire: every read
    /// that reaches the keychain under a signature the item does not trust
    /// yet is a dialog (two, the access list and the partition list), and a
    /// one-second cache made each "Allow" good for one second. Writes from
    /// another process (the CLI, a second copy of the app) post
    /// changedNotification, which drops it.
    private static var cached: Vault?

    /// Posted by every write, observed by the app. The object is the pid of
    /// the writer, so a process ignores its own saves (its cache is already
    /// the vault it wrote).
    static let changedNotification = Notification.Name("\(Config.label).vault-changed")

    /// When the keychain last refused, if it did. A refusal is taken at its
    /// word for refusalTTL: the dialog was dismissed once, and every host
    /// that logs in meanwhile must not bring it back. Settings asks sooner
    /// (read(force:)), and so does every write.
    private static var refusedAt: Date?
    static let refusalTTL: TimeInterval = 300

    /// What came back when the vault was last read. A keychain that refuses
    /// (the dialog dismissed or denied at launch, a locked keychain) is not
    /// an empty vault, and must never be taken for one: the secrets are
    /// still there, they just cannot be read this minute.
    enum VaultRead: Equatable {
        case ok(Vault)
        case empty
        case refused(OSStatus)
    }

    /// Why the last read failed, if it did. nil once one succeeds.
    private(set) static var refusal: KeychainError?

    /// The three outcomes of asking the keychain for the vault.
    static func interpret(_ status: OSStatus, _ item: CFTypeRef?) -> VaultRead {
        if status == errSecSuccess {
            guard let data = item as? Data,
                  let v = try? JSONDecoder().decode(Vault.self, from: data) else { return .empty }
            return .ok(v)
        }
        if status == errSecItemNotFound { return .empty }
        return .refused(status)
    }

    /// Whether a read should go to the keychain, or repeat the refusal it
    /// gave a moment ago.
    static func shouldAsk(refusedAt: Date?, now: Date, force: Bool) -> Bool {
        if force { return true }
        guard let t = refusedAt else { return true }
        return now.timeIntervalSince(t) >= refusalTTL
    }

    /// The vault a write starts from. Never a refused read: saving one
    /// secret over a vault that could not be read would replace every
    /// other secret in it with that one.
    static func writable(_ read: VaultRead) throws -> Vault {
        switch read {
        case .ok(let v): return v
        case .empty: return [:]
        case .refused(let status): throw KeychainError(status)
        }
    }

    // MARK: public API (host + kind)

    static func get(_ host: String, _ kind: SecretKind) -> String? {
        vault()[host]?[kind.rawValue]
    }

    static func has(_ host: String, _ kind: SecretKind) -> Bool {
        vault()[host]?[kind.rawValue] != nil
    }

    /// Writes ask the keychain afresh: a save is something the user just
    /// did, so one more dialog is fine there, and a vault that still cannot
    /// be read is not written over.
    static func set(_ host: String, _ kind: SecretKind, _ secret: String) throws {
        var v = try writable(read(force: true))
        v[host, default: [:]][kind.rawValue] = secret
        try save(v)
    }

    static func delete(_ host: String, _ kind: SecretKind) throws {
        var v = try writable(read(force: true))
        v[host]?[kind.rawValue] = nil
        if v[host]?.isEmpty == true { v[host] = nil }
        try save(v)
    }

    /// Hosts that have at least one secret stored.
    static func hosts() -> [String] { vault().keys.sorted() }

    // MARK: vault item

    private static func vaultQuery() -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: vaultService,
         kSecAttrAccount as String: vaultAccount]
    }

    private static func vault() -> Vault {
        switch read() {
        case .ok(let v): return v
        case .empty, .refused: return [:]
        }
    }

    /// Read the vault, saying which of the three things happened. A refusal
    /// is repeated for refusalTTL without asking the keychain again, unless
    /// forced.
    @discardableResult
    static func read(force: Bool = false) -> VaultRead {
        lock.lock(); defer { lock.unlock() }
        if force { cached = nil; refusedAt = nil }
        if let v = cached { return .ok(v) }
        if let r = refusal, !shouldAsk(refusedAt: refusedAt, now: Date(), force: force) { return .refused(r.status) }
        var q = vaultQuery()
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let outcome = interpret(SecItemCopyMatching(q as CFDictionary, &item), item)
        switch outcome {
        case .ok(let v):
            refusal = nil; refusedAt = nil
            cached = v
        case .empty:
            refusal = nil; refusedAt = nil
            cached = [:]
        case .refused(let status):
            refusal = KeychainError(status)
            refusedAt = Date()
            cached = nil
            Log.error("keychain: cannot read the shellder vault: \(KeychainError(status)). "
                      + "The secrets are still in it; nothing here treats this as an empty vault.")
        }
        return outcome
    }

    /// Write the vault in place: SecItemUpdate keeps the item's access list
    /// and partition, so a signature that can read the vault can also write
    /// it. (Delete + add used to be the only path; on an existing vault the
    /// delete can fail with "Invalid attempt to change the owner of this
    /// item", which lost every save while reads kept working, and when it
    /// works it is a second password dialog.)
    private static func save(_ v: Vault) throws {
        lock.lock(); defer { lock.unlock() }
        let data = try JSONEncoder().encode(v)
        let up = SecItemUpdate(vaultQuery() as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if up == errSecItemNotFound {
            try add(data)
        } else if up != errSecSuccess {
            throw KeychainError(up)
        }
        cached = v
        DistributedNotificationCenter.default().postNotificationName(
            changedNotification, object: String(getpid()), userInfo: nil, deliverImmediately: true)
    }

    /// Another process wrote the vault: the next read asks the keychain.
    static func observeChanges(_ onChange: @escaping () -> Void) -> NSObjectProtocol {
        DistributedNotificationCenter.default().addObserver(
            forName: changedNotification, object: nil, queue: .main) { n in
            if n.object as? String == String(getpid()) { return }
            lock.lock(); cached = nil; lock.unlock()
            onChange()
        }
    }

    private static func add(_ data: Data) throws {
        var add = vaultQuery()
        add[kSecValueData as String] = data
        add[kSecAttrLabel as String] = "\(Config.app) vault"
        let st = SecItemAdd(add as CFDictionary, nil)
        if st != errSecSuccess { throw KeychainError(st) }
    }
}
