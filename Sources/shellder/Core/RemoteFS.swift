import Foundation

/// Where a pane is looking: this Mac, or one of the hosts whose master is
/// up. Every filesystem call in the window goes through one of these.
enum FileSource: Hashable {
    case local
    case host(String)

    var isLocal: Bool {
        if case .local = self { return true }
        return false
    }

    var host: String? {
        if case .host(let h) = self { return h }
        return nil
    }

    /// Stable across panes, which is how a drag says where it came from.
    var id: String { host.map { "host:" + $0 } ?? "local" }

    static func read(id: String) -> FileSource {
        id == "local" ? .local : .host(String(id.dropFirst("host:".count)))
    }
}

/// One filesystem, whichever side it is on: the calls a pane makes.
enum FS {
    static func home(_ source: FileSource) -> Result<String, FSError> {
        guard let host = source.host else { return .success(Config.home) }
        return RemoteFS.home(host)
    }

    static func list(_ source: FileSource, _ path: String) -> Result<[FileItem], FSError> {
        guard let host = source.host else { return LocalFS.list(path) }
        return RemoteFS.list(host, path)
    }

    static func isDirectory(_ source: FileSource, _ path: String) -> Bool {
        guard let host = source.host else {
            var isDir: ObjCBool = false
            return FileManager.default.fileExists(atPath: path, isDirectory: &isDir) && isDir.boolValue
        }
        return RemoteFS.isDirectory(host, path)
    }

    /// Bytes a copy of this path has to move, or — pointed at the copy —
    /// how much of it has arrived.
    static func bytes(_ source: FileSource, _ path: String) -> Int64? {
        guard let host = source.host else { return LocalFS.bytes(path) }
        return RemoteFS.bytes(host, path)
    }

    /// Is something there already? What a copy would write over.
    static func exists(_ source: FileSource, _ path: String) -> Bool {
        guard let host = source.host else { return FileManager.default.fileExists(atPath: path) }
        return RemoteFS.exists(host, path)
    }

    static func makeDirectory(_ source: FileSource, _ path: String) -> FSError? {
        guard let host = source.host else { return LocalFS.makeDirectory(path) }
        return RemoteFS.makeDirectory(host, path)
    }

    static func makeFile(_ source: FileSource, _ path: String) -> FSError? {
        guard let host = source.host else { return LocalFS.makeFile(path) }
        return RemoteFS.makeFile(host, path)
    }

    /// On this Mac into the Trash, on a host gone for good — which is why
    /// the window words its question differently for each.
    static func remove(_ source: FileSource, _ path: String) -> FSError? {
        guard let host = source.host else { return LocalFS.trash(path) }
        return RemoteFS.remove(host, path)
    }
}

/// Why a directory could not be read, in the words the window shows.
struct FSError: Error, CustomStringConvertible {
    let description: String
    init(_ d: String) { description = d }
}

/// One entry of a directory listing, on a host or on this Mac.
struct FileItem: Identifiable, Hashable {
    let name: String
    /// Absolute path in its own filesystem.
    let path: String
    let isDir: Bool
    /// Bytes for a file, meaningless for a directory.
    let size: Int64
    /// When its content last changed. nil when the side could not say: a
    /// link to nowhere, or a host whose ls only gives the day.
    var modified: Date? = nil

    var id: String { path }
}

/// Directories on a host, read through the master with `ls`. BatchMode so a
/// host that is not up fails at once instead of waiting for an answer nobody
/// will give.
enum RemoteFS {
    /// The remote shell gets the command as one word list: single-quote the
    /// path, splicing in the single quote itself.
    static func quote(_ path: String) -> String {
        "'" + path.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    static func join(_ dir: String, _ name: String) -> String {
        dir == "/" ? "/" + name : dir + "/" + name
    }

    /// The directory above `path`, or nil at the root.
    static func parent(_ path: String) -> String? {
        guard path != "/" else { return nil }
        let up = (path as NSString).deletingLastPathComponent
        return up.isEmpty ? "/" : up
    }

    private static func sh(_ host: String, _ command: String) -> SSH.Result {
        SSH.run(["-o", "BatchMode=yes", "-T", host, command])
    }

    private static func failure(_ r: SSH.Result) -> FSError {
        let text = (r.stderr + r.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
        return FSError(text.isEmpty ? "ssh exited with status \(r.status)" : text)
    }

    /// The home directory, where a tree starts.
    static func home(_ host: String) -> Result<String, FSError> {
        let r = sh(host, "cd && pwd")
        let path = r.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        guard r.status == 0, path.hasPrefix("/") else { return .failure(failure(r)) }
        return .success(path)
    }

    static func isDirectory(_ host: String, _ path: String) -> Bool {
        sh(host, "test -d \(quote(path))").status == 0
    }

    static func exists(_ host: String, _ path: String) -> Bool {
        sh(host, "test -e \(quote(path))").status == 0
    }

    /// What `path` contains, in the order `ls` gives it: the pane sorts. `ls
    /// -L` follows symlinks, so a link to a directory opens like one. A link
    /// to nowhere keeps a line of its own (GNU) or is left out (BusyBox, the
    /// BSDs), and `ls` exits non-zero, which is only an error when nothing
    /// was read.
    static func list(_ host: String, _ path: String) -> Result<[FileItem], FSError> {
        let r = sh(host, listCommand(path))
        let items = parse(r.stdout, in: path)
        if items.isEmpty && r.status != 0 { return .failure(failure(r)) }
        return .success(items)
    }

    /// The listing, with times to the second in whichever way this host's
    /// ls knows: --full-time (GNU, BusyBox), -T (macOS, the BSDs), or failing
    /// both the plain form. In UTC, so a time that carries no zone reads the
    /// same on this Mac. Owners as numbers (-n): a group name with a blank in
    /// it would shift every field after it. Run by sh, whatever the login
    /// shell is.
    static func listCommand(_ path: String) -> String {
        let script = "if ls --full-time -d / >/dev/null 2>&1; then t=--full-time; "
            + "elif ls -T -d / >/dev/null 2>&1; then t=-T; else t=; fi; "
            + "LC_ALL=C TZ=UTC0 ls -nAL $t -- \(quote(path))"
        return "sh -c \(quote(script))"
    }

    /// `ls -nL` output into entries. A line is the mode, the link count, the
    /// owner, the group, the size (a device's "major, minor"), the time in
    /// one of the forms `LsTime` reads, then one blank and the name, which
    /// may itself start with a blank.
    static func parse(_ text: String, in dir: String, now: Date = Date()) -> [FileItem] {
        var out: [FileItem] = []
        for line in text.split(separator: "\n", omittingEmptySubsequences: true) {
            var fields = LsFields(line)
            guard let mode = fields.next(), let kind = mode.first, "-dlbcsp".contains(kind),
                  fields.next() != nil, fields.next() != nil, fields.next() != nil,
                  var size = fields.next() else { continue }
            if size.hasSuffix(",") {
                guard fields.next() != nil else { continue }
                size = "0"
            }
            guard let time = LsTime.read(&fields, now: now) else { continue }
            var name = String(fields.rest)
            if kind == "l", let arrow = name.range(of: " -> ") { name = String(name[..<arrow.lowerBound]) }
            guard !name.isEmpty, name != ".", name != ".." else { continue }
            out.append(FileItem(name: name, path: join(dir, name), isDir: kind == "d",
                                size: Int64(size) ?? 0, modified: time.date))
        }
        return out
    }

    /// Make a directory. Fails when something is there already, which is
    /// mkdir's own behaviour, and the message says so.
    static func makeDirectory(_ host: String, _ path: String) -> FSError? {
        let r = sh(host, "mkdir -- \(quote(path))")
        return r.status == 0 ? nil : failure(r)
    }

    /// Make an empty file. `set -C` is the shell's own refusal to write over
    /// something that is there already.
    static func makeFile(_ host: String, _ path: String) -> FSError? {
        let r = sh(host, "set -C; : > \(quote(path))")
        return r.status == 0 ? nil : failure(r)
    }

    /// Delete a path and, when it is a directory, what is under it. There is
    /// no undoing this on a host, so the window asks first.
    static func remove(_ host: String, _ path: String) -> FSError? {
        let r = sh(host, "rm -rf -- \(quote(path))")
        return r.status == 0 ? nil : failure(r)
    }

    /// Bytes in the regular files under `path` (the path itself when it is a
    /// file): what a copy of it has to move, and — pointed at the copy — how
    /// much of it has arrived. find/ls/awk are on every server; a path that
    /// is not there yet counts as zero, not as an error.
    static func bytes(_ host: String, _ path: String) -> Int64? {
        let r = sh(host, "find \(quote(path)) -type f -exec ls -ln -- {} + 2>/dev/null | awk '{s+=$5} END {print s+0}'")
        guard r.status == 0 else { return nil }
        return Int64(r.stdout.trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

/// The blank-separated fields of one `ls -l` line, read from the left.
/// What is left after the last one read is the name, blanks and all.
struct LsFields {
    private let line: Substring
    private var at: Substring.Index

    init(_ line: Substring) {
        self.line = line
        at = line.startIndex
    }

    mutating func next() -> Substring? {
        while at < line.endIndex, line[at] == " " { at = line.index(after: at) }
        guard at < line.endIndex else { return nil }
        let start = at
        while at < line.endIndex, line[at] != " " { at = line.index(after: at) }
        return line[start..<at]
    }

    /// The rest of the line after the one blank that ends the last field.
    var rest: Substring { at < line.endIndex ? line[line.index(after: at)...] : "" }
}

/// The time on an `ls -l` line, in each of the forms ls writes it:
///
///     2024-01-02 03:04:05.000000000 +0000   --full-time (GNU, BusyBox)
///     Jan  2 03:04:05 2024                  -T (macOS, the BSDs)
///     Jan  2 03:04    Jan  2  2024          plain: to the minute within
///                                           six months, the day before
///     ?                                     GNU, for a link to nowhere
enum LsTime {
    case at(Date)
    /// There is a time, but not one to sort by: the "?", or a day alone.
    case unknown

    var date: Date? {
        if case .at(let date) = self { return date }
        return nil
    }

    private static let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                 "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

    private static let utc: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()

    /// Reads the time off the next fields, nil when they are not one: the
    /// name cannot be found on a line whose time is not understood.
    static func read(_ fields: inout LsFields, now: Date) -> LsTime? {
        guard let first = fields.next() else { return nil }
        if first == "?" { return .unknown }
        let day = first.split(separator: "-")
        if day.count == 3 {
            guard let clock = fields.next(), let zone = fields.next(), let offset = offset(zone),
                  let date = utcDate(Int(day[0]), Int(day[1]), Int(day[2]), clock) else { return nil }
            return .at(date.addingTimeInterval(-offset))
        }
        guard let month = months.firstIndex(of: String(first)).map({ $0 + 1 }),
              let dayOfMonth = fields.next().flatMap({ Int($0) }),
              let third = fields.next() else { return nil }
        switch third.split(separator: ":").count {
        case 3:
            guard let year = fields.next().flatMap({ Int($0) }),
                  let date = utcDate(year, month, dayOfMonth, third) else { return nil }
            return .at(date)
        case 2:
            // No year: ls leaves it out within six months of its own clock,
            // so a date ahead of this one is last year's.
            let year = utc.component(.year, from: now)
            guard let date = utcDate(year, month, dayOfMonth, third) else { return nil }
            if date > now.addingTimeInterval(86400) {
                return utcDate(year - 1, month, dayOfMonth, third).map { .at($0) }
            }
            return .at(date)
        default:
            return Int(third) == nil ? nil : .unknown
        }
    }

    /// A UTC date from its day and a "03:04" or "03:04:05.5" clock.
    private static func utcDate(_ year: Int?, _ month: Int?, _ day: Int?, _ clock: Substring) -> Date? {
        let parts = clock.split(separator: ":")
        guard let year = year, let month = month, let day = day, parts.count >= 2,
              let hour = Int(parts[0]), let minute = Int(parts[1]) else { return nil }
        let seconds = parts.count > 2 ? Double(parts[2]) : 0
        guard let seconds = seconds,
              let whole = utc.date(from: DateComponents(year: year, month: month, day: day,
                                                         hour: hour, minute: minute,
                                                         second: Int(seconds))) else { return nil }
        return whole.addingTimeInterval(seconds - seconds.rounded(.down))
    }

    /// "+0800" in seconds.
    private static func offset(_ zone: Substring) -> TimeInterval? {
        guard zone.count == 5, let sign = zone.first, sign == "+" || sign == "-",
              let hhmm = Int(zone.dropFirst()) else { return nil }
        let seconds = TimeInterval(hhmm / 100 * 3600 + hhmm % 100 * 60)
        return sign == "-" ? -seconds : seconds
    }
}

/// The same two questions about this Mac's own filesystem.
enum LocalFS {
    static func list(_ path: String) -> Result<[FileItem], FSError> {
        let fm = FileManager.default
        do {
            let urls = try fm.contentsOfDirectory(at: URL(fileURLWithPath: path),
                                                  includingPropertiesForKeys: nil, options: [])
            // Under the directory as it was asked for, not as the URLs spell
            // it: /tmp and /private/tmp are the same place, and a pane whose
            // rows disagree with its root does not notice what lands in it.
            return .success(urls.map { item(RemoteFS.join(path, $0.lastPathComponent)) })
        } catch {
            return .failure(FSError((error as NSError).localizedDescription))
        }
    }

    /// Symlinks are followed, the way `ls -L` follows them on a host: a link
    /// to a directory opens like one. A link to nowhere stays a leaf, dated
    /// by the link itself.
    private static func item(_ path: String) -> FileItem {
        let name = (path as NSString).lastPathComponent
        var st = stat()
        guard stat(path, &st) == 0 else {
            let own = lstat(path, &st) == 0 ? date(st.st_mtimespec) : nil
            return FileItem(name: name, path: path, isDir: false, size: 0, modified: own)
        }
        let isDir = st.st_mode & S_IFMT == S_IFDIR
        return FileItem(name: name, path: path, isDir: isDir, size: isDir ? 0 : Int64(st.st_size),
                        modified: date(st.st_mtimespec))
    }

    private static func date(_ t: timespec) -> Date {
        Date(timeIntervalSince1970: TimeInterval(t.tv_sec) + TimeInterval(t.tv_nsec) / 1_000_000_000)
    }

    static func makeDirectory(_ path: String) -> FSError? {
        do {
            try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: false)
            return nil
        } catch {
            return FSError((error as NSError).localizedDescription)
        }
    }

    /// An empty file. FileManager would write over one that is there, so the
    /// name is checked first.
    static func makeFile(_ path: String) -> FSError? {
        let fm = FileManager.default
        guard !fm.fileExists(atPath: path) else {
            return FSError("\((path as NSString).lastPathComponent) is already there")
        }
        guard fm.createFile(atPath: path, contents: nil) else {
            return FSError("could not create \((path as NSString).lastPathComponent)")
        }
        return nil
    }

    /// Into the Trash, not gone: deleting on this Mac is undoable the way it
    /// is everywhere else on it.
    static func trash(_ path: String) -> FSError? {
        do {
            try FileManager.default.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: nil)
            return nil
        } catch {
            return FSError((error as NSError).localizedDescription)
        }
    }

    /// Bytes in the files under `path`, the counterpart of `RemoteFS.bytes`.
    static func bytes(_ path: String) -> Int64 {
        let fm = FileManager.default
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: path, isDirectory: &isDir) else { return 0 }
        if !isDir.boolValue {
            let attrs = try? fm.attributesOfItem(atPath: path)
            return (attrs?[.size] as? NSNumber)?.int64Value ?? 0
        }
        var total: Int64 = 0
        let keys: [URLResourceKey] = [.isRegularFileKey, .fileSizeKey]
        guard let e = fm.enumerator(at: URL(fileURLWithPath: path), includingPropertiesForKeys: keys,
                                    options: []) else { return 0 }
        for case let url as URL in e {
            let v = try? url.resourceValues(forKeys: Set(keys))
            if v?.isRegularFile == true { total += Int64(v?.fileSize ?? 0) }
        }
        return total
    }
}
