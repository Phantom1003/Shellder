import CryptoKit
import Foundation

enum TOTPError: Error, CustomStringConvertible {
    case badURI, badSecret
    var description: String {
        switch self {
        case .badURI: return "invalid otpauth:// URI"
        case .badSecret: return "invalid base32 secret"
        }
    }
}

/// RFC 6238 time-based one-time passwords. Accepts a bare base32 secret or an
/// otpauth://totp/... URI (digits, period and algorithm honoured).
struct TOTP {
    let key: Data
    let digits: Int
    let period: Int
    let algorithm: String

    init(parsing raw: String) throws {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        var digits = 6, period = 30, algo = "sha1"
        if s.lowercased().hasPrefix("otpauth://") {
            guard let u = URLComponents(string: s) else { throw TOTPError.badURI }
            let items = u.queryItems ?? []
            func q(_ n: String) -> String? { items.first { $0.name.lowercased() == n }?.value }
            guard let sec = q("secret") else { throw TOTPError.badURI }
            s = sec
            if let d = q("digits"), let v = Int(d) { digits = v }
            if let p = q("period"), let v = Int(p) { period = v }
            if let a = q("algorithm") { algo = a.lowercased() }
        }
        let cleaned = s.uppercased().filter { $0 != " " && $0 != "-" }
        guard let key = TOTP.base32Decode(cleaned), !key.isEmpty else { throw TOTPError.badSecret }
        self.key = key
        self.digits = digits
        self.period = period
        self.algorithm = algo
    }

    func code(at time: TimeInterval = Date().timeIntervalSince1970) -> String {
        var counter = UInt64(time / Double(period)).bigEndian
        let msg = Data(bytes: &counter, count: 8)
        let k = SymmetricKey(data: key)
        let mac: [UInt8]
        switch algorithm {
        case "sha256": mac = Array(HMAC<SHA256>.authenticationCode(for: msg, using: k))
        case "sha512": mac = Array(HMAC<SHA512>.authenticationCode(for: msg, using: k))
        default: mac = Array(HMAC<Insecure.SHA1>.authenticationCode(for: msg, using: k))
        }
        let off = Int(mac[mac.count - 1] & 0x0f)
        let bin = (UInt32(mac[off]) << 24 | UInt32(mac[off + 1]) << 16
                   | UInt32(mac[off + 2]) << 8 | UInt32(mac[off + 3])) & 0x7fff_ffff
        var mod: UInt32 = 1
        for _ in 0..<digits { mod *= 10 }
        let v = bin % mod
        let s = String(v)
        return String(repeating: "0", count: max(0, digits - s.count)) + s
    }

    var secondsRemaining: Int { period - Int(Date().timeIntervalSince1970) % period }

    static func base32Decode(_ s: String) -> Data? {
        let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
        var map = [Character: UInt32]()
        for (i, c) in alphabet.enumerated() { map[c] = UInt32(i) }
        var bits = 0
        var value: UInt32 = 0
        var out = Data()
        for c in s where c != "=" {
            guard let v = map[c] else { return nil }
            value = (value << 5) | v
            bits += 5
            if bits >= 8 {
                out.append(UInt8((value >> UInt32(bits - 8)) & 0xff))
                bits -= 8
                value &= (1 << UInt32(bits)) - 1
            }
        }
        return out
    }

    /// A code not already used for this host. Most PAM/TOTP modules reject
    /// re-use inside a window, so a quick reconnect waits for the next one.
    static func fresh(host: String, raw: String) throws -> String {
        let t = try TOTP(parsing: raw)
        let safe = host.map { c -> String in
            (c.isLetter || c.isNumber || c == "." || c == "_" || c == "@" || c == "-") ? String(c) : "_"
        }.joined()
        let marker = Config.stateDir + "/" + safe + ".totp-last"
        try? FileManager.default.createDirectory(atPath: Config.stateDir, withIntermediateDirectories: true,
                                                 attributes: [.posixPermissions: 0o700])
        let last = (try? String(contentsOfFile: marker, encoding: .utf8))?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        var code = t.code()
        if code == last {
            let wait = Double(t.secondsRemaining) + 1
            Log.info("\(host): TOTP code already used, waiting \(Int(wait))s for next window")
            Thread.sleep(forTimeInterval: wait)
            code = t.code()
        }
        try? code.write(toFile: marker, atomically: true, encoding: .utf8)
        return code
    }
}
