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

    /// What `path` contains, directories first. `ls -L` follows symlinks, so
    /// a link to a directory opens like one; a broken link keeps its own line
    /// and `ls` exits non-zero, which is only an error when nothing was read.
    static func list(_ host: String, _ path: String) -> Result<[FileItem], FSError> {
        let r = sh(host, "LC_ALL=C ls -lAL -- \(quote(path))")
        let items = parse(r.stdout, in: path)
        if items.isEmpty && r.status != 0 { return .failure(failure(r)) }
        return .success(sorted(items))
    }

    static func sorted(_ items: [FileItem]) -> [FileItem] {
        items.sorted {
            $0.isDir == $1.isDir ? $0.name.localizedStandardCompare($1.name) == .orderedAscending : $0.isDir
        }
    }

    /// `ls -l` output into entries: nine fields, the ninth being the name
    /// (with " -> target" after it for a link `ls` could not follow).
    static func parse(_ text: String, in dir: String) -> [FileItem] {
        var out: [FileItem] = []
        for line in text.split(separator: "\n", omittingEmptySubsequences: true) {
            if line.hasPrefix("total ") { continue }
            let f = line.split(separator: " ", maxSplits: 8, omittingEmptySubsequences: true)
            guard f.count == 9, let kind = f[0].first, "-dlbcsp".contains(kind) else { continue }
            var name = String(f[8])
            if kind == "l", let arrow = name.range(of: " -> ") { name = String(name[..<arrow.lowerBound]) }
            guard !name.isEmpty, name != ".", name != ".." else { continue }
            out.append(FileItem(name: name, path: join(dir, name), isDir: kind == "d",
                                size: Int64(f[4]) ?? 0))
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
            return .success(RemoteFS.sorted(urls.map { item(RemoteFS.join(path, $0.lastPathComponent)) }))
        } catch {
            return .failure(FSError((error as NSError).localizedDescription))
        }
    }

    /// Symlinks are followed, the way `ls -L` follows them on a host: a link
    /// to a directory opens like one. A link to nowhere stays a leaf.
    private static func item(_ path: String) -> FileItem {
        let fm = FileManager.default
        var isDir: ObjCBool = false
        let exists = fm.fileExists(atPath: path, isDirectory: &isDir)
        var size: Int64 = 0
        if exists && !isDir.boolValue {
            let target = (path as NSString).resolvingSymlinksInPath
            size = ((try? fm.attributesOfItem(atPath: target))?[.size] as? NSNumber)?.int64Value ?? 0
        }
        return FileItem(name: (path as NSString).lastPathComponent, path: path,
                        isDir: exists && isDir.boolValue, size: size)
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
