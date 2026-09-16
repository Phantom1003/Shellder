import Foundation

/// Append-only file logger shared by every process mode (app, askpass, CLI).
enum Log {
    private static let lock = NSLock()
    private static let fmt: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return f
    }()
    static var alsoStderr = false

    static func write(_ level: String, _ msg: String) {
        let line = "\(fmt.string(from: Date())) \(level) [\(getpid())] \(msg)\n"
        let data = line.data(using: .utf8)!
        lock.lock(); defer { lock.unlock() }
        if let h = appendHandle() {
            h.write(data)
            h.closeFile()
        }
        if alsoStderr { FileHandle.standardError.write(data) }
    }

    static func info(_ m: String) { write("INFO", m) }
    /// Output of an ssh process we spawned (its stderr/stdout), one line each.
    static func ssh(_ host: String, _ line: String) { write("SSH", "\(host): \(line)") }
    /// A prompt ssh raised and how it was answered.
    static func prompt(_ m: String) { write("PROMPT", m) }
    static func warn(_ m: String) { write("WARNING", m) }
    static func error(_ m: String) { write("ERROR", m) }

    /// A handle positioned at the end of the log file (also used as stdout/stderr
    /// of the spawned ssh masters so their diagnostics land in the same file).
    static func appendHandle() -> FileHandle? {
        let fm = FileManager.default
        try? fm.createDirectory(atPath: (Config.logFile as NSString).deletingLastPathComponent,
                                withIntermediateDirectories: true)
        if !fm.fileExists(atPath: Config.logFile) {
            fm.createFile(atPath: Config.logFile, contents: nil, attributes: [.posixPermissions: 0o600])
        }
        guard let h = FileHandle(forWritingAtPath: Config.logFile) else { return nil }
        h.seekToEndOfFile()
        return h
    }
}
