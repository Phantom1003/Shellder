import Foundation

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
            return .success(RemoteFS.sorted(urls.map { item($0.path) }))
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
