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
/// keychain asks for permission at most once; signed with a Team ID identity
/// that answer survives rebuilds (see reownIfNeeded).
enum Keychain {
    typealias Vault = [String: [String: String]]

    static let vaultService = "\(Config.app):vault"
    static let vaultAccount = Config.app

    private static let lock = NSLock()
    private static var cached: (Vault, Date)?
    private static let cacheTTL: TimeInterval = 1   // the CLI may edit the vault while the app runs

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

    /// Whether the vault should be written again under the current
    /// signature. Never from a read that was refused: that would put an
    /// empty vault over the real one.
    static func shouldReown(previous: String?, me: String, read: VaultRead) -> Bool {
        guard previous != me else { return false }
        switch read {
        case .refused: return false
        case .empty: return previous != nil
        case .ok(let v): return !v.isEmpty || previous != nil
        }
    }

    // MARK: public API (host + kind)

    static func get(_ host: String, _ kind: SecretKind) -> String? {
        vault()[host]?[kind.rawValue]
    }

    static func has(_ host: String, _ kind: SecretKind) -> Bool {
        vault()[host]?[kind.rawValue] != nil
    }

    static func set(_ host: String, _ kind: SecretKind, _ secret: String) throws {
        var v = vault()
        v[host, default: [:]][kind.rawValue] = secret
        try save(v)
    }

    static func delete(_ host: String, _ kind: SecretKind) {
        var v = vault()
        v[host]?[kind.rawValue] = nil
        if v[host]?.isEmpty == true { v[host] = nil }
        try? save(v)
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
    /// is not cached, so the next try asks the keychain again.
    @discardableResult
    static func read(force: Bool = false) -> VaultRead {
        lock.lock(); defer { lock.unlock() }
        if force { cached = nil }
        if let (v, t) = cached, Date().timeIntervalSince(t) < cacheTTL { return .ok(v) }
        var q = vaultQuery()
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let outcome = interpret(SecItemCopyMatching(q as CFDictionary, &item), item)
        switch outcome {
        case .ok(let v):
            refusal = nil
            cached = (v, Date())
        case .empty:
            refusal = nil
            cached = ([:], Date())
        case .refused(let status):
            refusal = KeychainError(status)
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
    /// item", which lost every save while reads kept working.)
    private static func save(_ v: Vault) throws {
        lock.lock(); defer { lock.unlock() }
        let data = try JSONEncoder().encode(v)
        let up = SecItemUpdate(vaultQuery() as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if up == errSecItemNotFound {
            try add(data)
        } else if up != errSecSuccess {
            throw KeychainError(up)
        }
        cached = (v, Date())
    }

    private static func add(_ data: Data) throws {
        var add = vaultQuery()
        add[kSecValueData as String] = data
        add[kSecAttrLabel as String] = "\(Config.app) vault"
        let st = SecItemAdd(add as CFDictionary, nil)
        if st != errSecSuccess { throw KeychainError(st) }
    }

    /// Delete + add: the keychain records the *creator* in the item's access
    /// list and uses its Team ID as the item's partition, so a vault written
    /// this way by the current signature never prompts for that signature
    /// again. Only used when the signature changed (reownIfNeeded).
    private static func recreate(_ v: Vault) throws {
        lock.lock(); defer { lock.unlock() }
        let data = try JSONEncoder().encode(v)
        let del = SecItemDelete(vaultQuery() as CFDictionary)
        if del != errSecSuccess && del != errSecItemNotFound { throw KeychainError(del) }
        try add(data)
        cached = (v, Date())
    }

    /// Identity of the running code as the keychain sees it: the Team ID of
    /// an Apple-issued signature, else the per-build cdhash.
    static var codeIdentity: String {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess, let c = code else { return "unknown" }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(unsafeBitCast(c, to: SecStaticCode.self), SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let d = info as? [String: Any] else { return "unknown" }
        if let team = d[kSecCodeInfoTeamIdentifier as String] as? String, !team.isEmpty { return "teamid:" + team }
        if let unique = d[kSecCodeInfoUnique as String] as? Data { return "cdhash:" + unique.map { String(format: "%02x", $0) }.joined() }
        return "unknown"
    }

    /// Re-create the vault under the current signature when the signature
    /// changed since the last time (first launch after re-signing). Reading
    /// the old item may show the keychain dialog one last time. If the
    /// keychain refuses to re-create it, the existing item stays: saves go
    /// through SecItemUpdate anyway, only the "always allow" answer may have
    /// to be given again.
    static func reownIfNeeded() {
        let me = codeIdentity
        let previous = Prefs.vaultOwner
        guard previous != me else { return }
        let outcome = read()
        if case .refused(let status) = outcome {
            // Whatever is in there stays in there: re-creating it from a
            // read that was refused would write an empty vault over the
            // real one. Try again next launch.
            Log.warn("keychain: the vault could not be read (\(KeychainError(status))); "
                     + "leaving it under \(previous ?? "an untracked signature")")
            return
        }
        if shouldReown(previous: previous, me: me, read: outcome) {
            let v: Vault = { if case .ok(let v) = outcome { return v }; return [:] }()
            do {
                try recreate(v)
                Log.info("keychain: vault re-created under \(me) (was \(previous ?? "untracked"))")
            } catch {
                Log.warn("keychain: could not re-create the vault under \(me): \(error); keeping the existing item")
            }
        }
        Prefs.vaultOwner = me
    }
}
