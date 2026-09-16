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
        lock.lock(); defer { lock.unlock() }
        if let (v, t) = cached, Date().timeIntervalSince(t) < cacheTTL { return v }
        var q = vaultQuery()
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let st = SecItemCopyMatching(q as CFDictionary, &item)
        var v: Vault = [:]
        if st == errSecSuccess, let data = item as? Data {
            v = (try? JSONDecoder().decode(Vault.self, from: data)) ?? [:]
        } else if st != errSecItemNotFound {
            Log.error("keychain: cannot read the shellder vault: \(KeychainError(st))")
        }
        cached = (v, Date())
        return v
    }

    /// Always delete + add rather than update: the keychain records the
    /// *creator* in the item's access list and uses its Team ID as the item's
    /// partition, so a vault written by the current signature never prompts
    /// for that signature again.
    private static func save(_ v: Vault) throws {
        lock.lock(); defer { lock.unlock() }
        let data = try JSONEncoder().encode(v)
        let del = SecItemDelete(vaultQuery() as CFDictionary)
        if del != errSecSuccess && del != errSecItemNotFound { throw KeychainError(del) }
        var add = vaultQuery()
        add[kSecValueData as String] = data
        add[kSecAttrLabel as String] = "\(Config.app) vault"
        let st = SecItemAdd(add as CFDictionary, nil)
        if st != errSecSuccess { throw KeychainError(st) }
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
    /// the old item may show the keychain dialog one last time.
    static func reownIfNeeded() {
        let me = codeIdentity
        let previous = Prefs.vaultOwner
        guard previous != me else { return }
        let v = vault()
        if !v.isEmpty || previous != nil {
            do {
                try save(v)
                Log.info("keychain: vault re-created under \(me) (was \(previous ?? "untracked"))")
            } catch {
                Log.error("keychain: could not re-create the vault: \(error)")
                return
            }
        }
        Prefs.vaultOwner = me
    }
}
