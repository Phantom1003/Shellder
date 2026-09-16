import Foundation

/// One `Host` block from the user's ssh configuration.
struct HostEntry: Identifiable, Hashable {
    let alias: String          // first non-pattern name of the Host line
    let aliases: [String]      // remaining names on the same line
    let source: String         // file the block was read from
    var id: String { alias }
}

/// What `ssh -G alias` resolves to for a host.
struct ResolvedHost: Equatable {
    var hostname = ""
    var user = ""
    var port = "22"
    var proxyJump: String?
    var proxyCommand: String?
    var controlPath: String?
    var controlMaster = ""
    var controlPersist = ""
    var identityFiles: [String] = []
    var serverAliveInterval = 0
    var strictHostKeyChecking = ""
    /// Alias in the catalog that the first ProxyJump hop refers to, if any.
    var jumpAlias: String?

    init() {}

    init(_ cfg: [String: [String]]) {
        func first(_ k: String) -> String? { cfg[k]?.first }
        hostname = first("hostname") ?? ""
        user = first("user") ?? ""
        port = first("port") ?? "22"
        if let pj = first("proxyjump"), pj.lowercased() != "none" { proxyJump = pj }
        if let pc = first("proxycommand"), pc.lowercased() != "none" { proxyCommand = pc }
        if let cp = first("controlpath"), cp.lowercased() != "none" { controlPath = cp }
        controlMaster = first("controlmaster") ?? ""
        controlPersist = first("controlpersist") ?? ""
        identityFiles = cfg["identityfile"] ?? []
        serverAliveInterval = Int(first("serveraliveinterval") ?? "0") ?? 0
        strictHostKeyChecking = first("stricthostkeychecking") ?? ""
    }

    /// First hop of ProxyJump with user@ and :port stripped.
    var firstJumpHost: String? {
        guard let pj = proxyJump else { return nil }
        guard var hop = pj.split(separator: ",").first.map(String.init) else { return nil }
        if let at = hop.lastIndex(of: "@") { hop = String(hop[hop.index(after: at)...]) }
        if hop.hasPrefix("[") { hop = String(hop.dropFirst()).split(separator: "]").first.map(String.init) ?? hop }
        else if let colon = hop.lastIndex(of: ":") { hop = String(hop[..<colon]) }
        return hop.isEmpty ? nil : hop
    }

    var identityFilesExpanded: [String] { identityFiles.map(Config.expandTilde) }
}

/// Reads Host aliases straight from ~/.ssh/config (following Include).
/// Patterns (`*`, `?`, `!`) and Match blocks are skipped: they are not
/// something one can open a master connection to.
enum HostCatalog {
    struct Result {
        var entries: [HostEntry] = []
        var files: [String] = []
    }

    static func load() -> Result {
        var r = Result()
        var seen = Set<String>()
        parse(file: Config.sshConfigFile, depth: 0, into: &r, seen: &seen)
        return r
    }

    static func modificationDates(_ files: [String]) -> [String: Date?] {
        var out: [String: Date?] = [:]
        for f in files {
            out[f] = (try? FileManager.default.attributesOfItem(atPath: f))?[.modificationDate] as? Date
        }
        return out
    }

    private static func parse(file: String, depth: Int, into r: inout Result, seen: inout Set<String>) {
        guard depth < 8, !r.files.contains(file) else { return }
        r.files.append(file)
        guard let text = try? String(contentsOfFile: file, encoding: .utf8) else { return }
        for raw in text.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty || line.hasPrefix("#") { continue }
            let (key, value) = splitKeyValue(line)
            switch key.lowercased() {
            case "host":
                let names = tokens(value).filter { !$0.contains("*") && !$0.contains("?") && !$0.hasPrefix("!") }
                guard let primary = names.first, !seen.contains(primary) else { continue }
                seen.insert(primary)
                r.entries.append(HostEntry(alias: primary, aliases: Array(names.dropFirst()), source: file))
            case "include":
                for pat in tokens(value) {
                    for f in expand(pat) { parse(file: f, depth: depth + 1, into: &r, seen: &seen) }
                }
            default:
                break
            }
        }
    }

    /// "Key value", "Key=value" and "Key = value" are all valid ssh_config.
    private static func splitKeyValue(_ line: String) -> (String, String) {
        guard let idx = line.firstIndex(where: { $0 == " " || $0 == "\t" || $0 == "=" }) else { return (line, "") }
        let key = String(line[..<idx])
        var rest = line[idx...].trimmingCharacters(in: .whitespaces)
        if rest.hasPrefix("=") { rest = rest.dropFirst().trimmingCharacters(in: .whitespaces) }
        return (key, rest)
    }

    /// Whitespace-separated tokens with support for double-quoted strings.
    private static func tokens(_ s: String) -> [String] {
        var out: [String] = []
        var cur = ""
        var quoted = false
        var has = false
        for c in s {
            if c == "\"" { quoted.toggle(); has = true; continue }
            if !quoted && (c == " " || c == "\t") {
                if has { out.append(cur); cur = ""; has = false }
                continue
            }
            cur.append(c); has = true
        }
        if has { out.append(cur) }
        return out
    }

    /// Include paths: ~ expansion, relative to ~/.ssh, shell globs.
    private static func expand(_ pattern: String) -> [String] {
        var p = Config.expandTilde(pattern)
        if !p.hasPrefix("/") { p = Config.sshDir + "/" + p }
        var g = glob_t()
        defer { globfree(&g) }
        guard glob(p, GLOB_TILDE | GLOB_BRACE, nil, &g) == 0, let pathv = g.gl_pathv else { return [] }
        return (0..<Int(g.gl_pathc)).compactMap { pathv[$0].map { String(cString: $0) } }
    }
}
