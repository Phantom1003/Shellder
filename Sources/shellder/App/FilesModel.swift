import AppKit
import Combine

/// One entry as the outline view holds it: a reference the view can keep
/// across reloads, with its children once that directory has been read.
final class FileNode: NSObject {
    let item: FileItem
    /// nil until the directory has been read.
    var children: [FileNode]?

    init(_ item: FileItem) { self.item = item }
}

/// One line of a pane as the tests read it: the item and how deep it sits.
/// The outline view draws from the nodes themselves.
struct FileRow: Identifiable {
    let item: FileItem
    let depth: Int
    var id: String { item.path }
}

/// A copy, from the moment it is dropped until long after it has finished:
/// the list under the panes keeps them so a transfer is something to look
/// back at, not a bar that flashes past.
struct TransferRecord: Identifiable {
    enum State: Equatable {
        case waiting, running, done, cancelled
        case failed(String)
    }

    let id: UUID
    let job: TransferJob
    var state: State = .waiting
    /// 0…1 while it runs, nil while the size is unknown.
    var progress: Double?
    var total: Int64?
    var queuedAt = Date()
    var endedAt: Date?

    var isOver: Bool {
        switch state {
        case .waiting, .running: return false
        default: return true
        }
    }
}

/// A place a pane can jump to, the way Finder's sidebar and Go menu do.
struct Shortcut: Identifiable {
    let title: String
    let path: String
    var id: String { path }
}

/// The two trees of a Files window and the copies dragged between them.
/// Each pane looks at its own side — this Mac or any host whose master is
/// up — so the same window also copies between two hosts. Listings and
/// copies run off the main thread; everything the views read is published
/// from it.
final class FilesModel: ObservableObject {
    enum Pane: String, CaseIterable {
        case left, right

        var other: Pane { self == .left ? .right : .left }
    }

    /// The host the window was opened from: the left pane's first side.
    let host: String
    /// The hosts a pane can switch to, asked every time the menu opens.
    var connectedHosts: () -> [String] = { [] }

    @Published private(set) var sources: [Pane: FileSource] = [:]
    /// What each pane shows: the entries of its root, with the children of
    /// every directory that has been opened.
    @Published private(set) var trees: [Pane: [FileNode]] = [.left: [], .right: []]
    /// Bumped whenever a tree changed, which is what makes a pane reload.
    @Published private(set) var revision = 0
    @Published private(set) var roots: [Pane: String] = [:]
    @Published private(set) var errors: [Pane: String] = [:]
    /// The directories the user has opened, by path, so a pane can be read
    /// again without losing its shape.
    @Published private(set) var expanded: [Pane: Set<String>] = [.left: [], .right: []]
    /// Dot entries stay out of the way until the pane is asked for them.
    @Published private(set) var showHidden: [Pane: Bool] = [.left: false, .right: false]
    /// The row each pane has selected.
    @Published var selected: [Pane: String] = [:]

    /// Every copy this window has run, oldest first.
    @Published private(set) var transfers: [TransferRecord] = []
    /// Whether the list of them is open under the panes. It is, to begin
    /// with: a copy is something to look at, not a bar that flashes past.
    @Published var showHistory = true
    /// What a new folder, a new file or a delete had to say.
    @Published private(set) var notice = ""
    @Published private(set) var noticeIsError = false

    private var nodes: [Pane: [String: FileNode]] = [.left: [:], .right: [:]]
    private var loading: [Pane: Set<String>] = [.left: [], .right: []]
    private var homes: [FileSource: String] = [:]

    private var running: TransferRun?
    /// Listings are read one after another; a copy has its own queue so a
    /// slow directory never holds it up.
    private let lister = DispatchQueue(label: "local.shellder.files.list", qos: .userInitiated)
    private let copier = DispatchQueue(label: "local.shellder.files.copy", qos: .userInitiated)
    /// Long enough to look back at, short enough to stay a list.
    private let historyLimit = 200

    init(host: String) {
        self.host = host
    }

    // MARK: sides

    func source(_ pane: Pane) -> FileSource { sources[pane] ?? .local }
    func root(_ pane: Pane) -> String { roots[pane] ?? "" }

    /// Opens the window: the host it was asked for on the left, this Mac on
    /// the right.
    func start() {
        guard sources.isEmpty else { return }
        setSource(.left, .host(host))
        setSource(.right, .local)
    }

    /// Point a pane at another side and open it at that side's home
    /// directory — for a host, at ~/Shellder when it is there.
    func setSource(_ pane: Pane, _ source: FileSource) {
        sources[pane] = source
        roots[pane] = nil
        trees[pane] = []
        nodes[pane] = [:]
        expanded[pane] = []
        errors[pane] = nil
        selected[pane] = nil
        revision += 1
        lister.async { [weak self] in
            guard let self = self else { return }
            let found = FS.home(source)
            DispatchQueue.main.async {
                guard self.source(pane) == source else { return }
                switch found {
                case .success(let home):
                    self.homes[source] = home
                    let shelf = RemoteFS.join(home, SSH.uploadDirectory)
                    let start = source.isLocal ? home
                        : (FS.isDirectory(source, shelf) ? shelf : home)
                    self.setRoot(pane, start)
                case .failure(let e):
                    self.errors[pane] = e.description
                    self.revision += 1
                }
            }
        }
    }

    /// The places the Go menu offers for this pane's side.
    func shortcuts(_ pane: Pane) -> [Shortcut] {
        let source = source(pane)
        let home = homes[source] ?? (source.isLocal ? Config.home : "/")
        if source.isLocal {
            let fm = FileManager.default
            var out = [Shortcut(title: fm.displayName(atPath: home), path: home)]
            for directory in [FileManager.SearchPathDirectory.desktopDirectory, .documentDirectory,
                              .downloadsDirectory, .moviesDirectory, .musicDirectory, .picturesDirectory,
                              .applicationDirectory] {
                guard let url = fm.urls(for: directory, in: .userDomainMask).first,
                      fm.fileExists(atPath: url.path) else { continue }
                out.append(Shortcut(title: fm.displayName(atPath: url.path), path: url.path))
            }
            out.append(Shortcut(title: fm.displayName(atPath: "/"), path: "/"))
            return out
        }
        return [
            Shortcut(title: L("Home"), path: home),
            Shortcut(title: SSH.uploadDirectory, path: RemoteFS.join(home, SSH.uploadDirectory)),
            Shortcut(title: "/", path: "/"),
            Shortcut(title: "/tmp", path: "/tmp"),
            Shortcut(title: "/etc", path: "/etc"),
            Shortcut(title: "/var/log", path: "/var/log"),
        ]
    }

    // MARK: trees

    func setRoot(_ pane: Pane, _ path: String) {
        roots[pane] = path
        trees[pane] = []
        nodes[pane] = [:]
        expanded[pane] = []
        errors[pane] = nil
        selected[pane] = nil
        revision += 1
        load(pane, path)
    }

    /// The directory above the root becomes the root: the way out of a tree.
    func up(_ pane: Pane) {
        guard let parent = RemoteFS.parent(root(pane)) else { return }
        setRoot(pane, parent)
    }

    /// Read the root and every open directory again, keeping the shape.
    func reload(_ pane: Pane) {
        load(pane, root(pane))
        for dir in expanded[pane] ?? [] { load(pane, dir) }
    }

    func toggleHidden(_ pane: Pane) {
        showHidden[pane] = !(showHidden[pane] ?? false)
        reload(pane)
    }

    /// Fold a directory open or shut, reading it the first time it opens.
    func toggle(_ pane: Pane, _ item: FileItem) {
        guard item.isDir else { return }
        if expanded[pane]?.contains(item.path) == true {
            collapse(pane, item.path)
        } else {
            expand(pane, item.path)
        }
    }

    func expand(_ pane: Pane, _ path: String) {
        expanded[pane]?.insert(path)
        if nodes[pane]?[path]?.children == nil { load(pane, path) } else { revision += 1 }
    }

    func collapse(_ pane: Pane, _ path: String) {
        expanded[pane]?.remove(path)
        revision += 1
    }

    func isLoading(_ pane: Pane, _ path: String) -> Bool { loading[pane]?.contains(path) == true }

    private func load(_ pane: Pane, _ dir: String) {
        guard !dir.isEmpty, loading[pane]?.contains(dir) != true else { return }
        let source = source(pane)
        loading[pane]?.insert(dir)
        lister.async { [weak self] in
            guard let self = self else { return }
            let listed = FS.list(source, dir)
            DispatchQueue.main.async {
                self.loading[pane]?.remove(dir)
                guard self.source(pane) == source else { return }
                switch listed {
                case .success(let items):
                    self.place(items, of: dir, on: pane)
                    if dir == self.root(pane) { self.errors[pane] = nil }
                case .failure(let e):
                    self.place([], of: dir, on: pane)
                    self.errors[pane] = e.description
                    Log.warn("\(self.host): \(source.id) \(dir): \(e.description)")
                }
                self.revision += 1
            }
        }
    }

    /// Put a listing into the tree, keeping the nodes of entries that were
    /// there before so their open directories stay open.
    private func place(_ items: [FileItem], of dir: String, on pane: Pane) {
        let hidden = showHidden[pane] ?? false
        let kept = items
            .filter { hidden || !$0.name.hasPrefix(".") }
            .map { item -> FileNode in
                if let old = nodes[pane]?[item.path], old.item.isDir == item.isDir { return old }
                return FileNode(item)
            }
        for node in kept { nodes[pane]?[node.item.path] = node }
        if dir == root(pane) {
            trees[pane] = kept
        } else {
            nodes[pane]?[dir]?.children = kept
        }
    }

    /// The lines a pane shows right now, flattened: what the outline draws.
    func rows(_ pane: Pane) -> [FileRow] {
        var out: [FileRow] = []
        func walk(_ list: [FileNode], _ depth: Int) {
            for node in list {
                out.append(FileRow(item: node.item, depth: depth))
                if expanded[pane]?.contains(node.item.path) == true, let kids = node.children {
                    walk(kids, depth + 1)
                }
            }
        }
        walk(trees[pane] ?? [], 0)
        return out
    }

    func node(_ pane: Pane, _ path: String) -> FileNode? { nodes[pane]?[path] }

    /// Where a drop on this item goes: into a directory, or into the
    /// directory a file sits in.
    func destination(for item: FileItem, on pane: Pane) -> String {
        item.isDir ? item.path : (RemoteFS.parent(item.path) ?? root(pane))
    }

    /// Where a new folder or file goes: into what is selected when that is a
    /// directory, into the directory around it when it is a file, and into
    /// the pane's root when nothing is selected.
    func target(_ pane: Pane) -> String {
        guard let path = selected[pane], let node = node(pane, path) else { return root(pane) }
        return destination(for: node.item, on: pane)
    }

    // MARK: making and deleting

    func createDirectory(_ pane: Pane, named name: String) {
        create(pane, named: name, directory: true)
    }

    func createFile(_ pane: Pane, named name: String) {
        create(pane, named: name, directory: false)
    }

    private func create(_ pane: Pane, named name: String, directory: Bool) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !trimmed.contains("/") else {
            report(L("A name cannot be empty or contain a slash."), isError: true)
            return
        }
        let source = source(pane)
        let parent = target(pane)
        let path = RemoteFS.join(parent, trimmed)
        lister.async { [weak self] in
            let failed = directory ? FS.makeDirectory(source, path) : FS.makeFile(source, path)
            DispatchQueue.main.async {
                guard let self = self else { return }
                if let failed = failed {
                    self.report(failed.description, isError: true)
                    Log.error("\(self.host): \(source.id): \(failed.description)")
                } else {
                    self.report("\(trimmed) → \(parent)", isError: false)
                    Log.info("\(self.host): \(source.id): created \(path)")
                    self.refresh(parent, on: source)
                }
            }
        }
    }

    /// Delete what `path` names. On this Mac it goes to the Trash, on a host
    /// it is gone — the window says which before asking.
    func delete(_ pane: Pane, _ path: String) {
        let source = source(pane)
        let parent = RemoteFS.parent(path) ?? root(pane)
        let name = (path as NSString).lastPathComponent
        lister.async { [weak self] in
            let failed = FS.remove(source, path)
            DispatchQueue.main.async {
                guard let self = self else { return }
                if let failed = failed {
                    self.report(failed.description, isError: true)
                    Log.error("\(self.host): \(source.id): \(failed.description)")
                } else {
                    self.report(source.isLocal ? L("\(name) moved to the Trash") : L("\(name) deleted"),
                                isError: false)
                    Log.info("\(self.host): \(source.id): deleted \(path)")
                    if self.selected[pane] == path { self.selected[pane] = nil }
                    self.expanded[pane]?.remove(path)
                    self.refresh(parent, on: source)
                }
            }
        }
    }

    private func report(_ text: String, isError: Bool) {
        notice = text
        noticeIsError = isError
    }

    // MARK: drag and drop

    /// What a dragged row carries: the side it is on and its path. Anything
    /// else dropped on a pane (text from another app) is left alone.
    static func payload(_ source: FileSource, _ path: String) -> String { "shellder-\(source.id)\u{1}\(path)" }

    static func read(_ payload: String) -> (source: FileSource, path: String)? {
        guard payload.hasPrefix("shellder-") else { return nil }
        let body = payload.dropFirst("shellder-".count)
        let parts = body.split(separator: "\u{1}", maxSplits: 1)
        guard parts.count == 2 else { return nil }
        return (FileSource.read(id: String(parts[0])), String(parts[1]))
    }

    /// What a pane would do with this drag: a row from another side is
    /// copied over, files dragged in from the Finder are copied to a host.
    func canAccept(_ pasteboard: NSPasteboard, on pane: Pane) -> Bool {
        if let text = pasteboard.string(forType: .string), let from = FilesModel.read(text) {
            return from.source != source(pane)
        }
        return !source(pane).isLocal && pasteboard.canReadObject(forClasses: [NSURL.self],
                                                                 options: [.urlReadingFileURLsOnly: true])
    }

    /// A drop on `directory` of one pane.
    @discardableResult
    func accept(_ pasteboard: NSPasteboard, into directory: String, on pane: Pane) -> Bool {
        guard canAccept(pasteboard, on: pane) else { return false }
        let to = source(pane)
        if let text = pasteboard.string(forType: .string), let from = FilesModel.read(text) {
            enqueue(from: from.source, to: to, source: from.path, destination: directory)
            return true
        }
        let urls = pasteboard.readObjects(forClasses: [NSURL.self],
                                          options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        for url in urls { enqueue(from: .local, to: to, source: url.path, destination: directory) }
        return !urls.isEmpty
    }

    // MARK: copies

    /// The selected row of one pane, copied into what the other pane shows:
    /// the same as dragging it across, for a hand that would rather click.
    func copySelection(from pane: Pane) {
        guard let path = selected[pane], !root(pane.other).isEmpty,
              source(pane) != source(pane.other) else { return }
        enqueue(from: source(pane), to: source(pane.other), source: path, destination: root(pane.other))
    }

    func canCopySelection(from pane: Pane) -> Bool {
        selected[pane] != nil && source(pane) != source(pane.other) && !root(pane.other).isEmpty
    }

    func enqueue(from: FileSource, to: FileSource, source: String, destination: String) {
        notice = ""
        let job = TransferJob(from: from, to: to, source: source, destination: destination)
        transfers.append(TransferRecord(id: job.id, job: job))
        if transfers.count > historyLimit { transfers.removeFirst(transfers.count - historyLimit) }
        next()
    }

    /// The copy running now, if one is.
    var active: TransferRecord? { transfers.first { $0.state == .running } }
    var queued: Int { transfers.filter { $0.state == .waiting }.count }
    /// What the bar says when nothing is running: the last copy, or the last
    /// thing a new folder or a delete had to say.
    var lastFinished: TransferRecord? { transfers.last { $0.isOver } }

    /// Stop the copy that is running. What has already landed stays.
    func cancel() { running?.cancel() }

    /// Forget the copies that are over; the ones still to run stay.
    func clearHistory() {
        transfers.removeAll { $0.isOver }
    }

    private func next() {
        guard active == nil, let index = transfers.firstIndex(where: { $0.state == .waiting }) else { return }
        transfers[index].state = .running
        transfers[index].progress = nil
        let job = transfers[index].job
        let run = TransferRun()
        running = run
        Log.info("\(host): copying \(job.from.id) \(job.source) → \(job.to.id) \(job.destination)")
        copier.async { [weak self] in
            let total = TransferRun.total(job)
            DispatchQueue.main.async { self?.update(job.id) { $0.total = total } }
            let result = run.run(job, total: total) { fraction in
                DispatchQueue.main.async { self?.update(job.id) { $0.progress = fraction } }
            }
            DispatchQueue.main.async { self?.finished(job, run, result) }
        }
    }

    private func update(_ id: UUID, _ change: (inout TransferRecord) -> Void) {
        guard let index = transfers.firstIndex(where: { $0.id == id }) else { return }
        change(&transfers[index])
    }

    private func finished(_ job: TransferJob, _ run: TransferRun, _ result: SSH.Result) {
        running = nil
        var body = (result.stderr + result.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
        if body.count > 2000 { body = String(body.suffix(2000)) }
        for line in body.split(separator: "\n") { Log.ssh(host, String(line)) }
        update(job.id) { record in
            record.progress = nil
            record.endedAt = Date()
            if run.isCancelled {
                record.state = .cancelled
            } else if result.status == 0 {
                record.state = .done
            } else {
                // scp says it in a line or two; one line is what there is room for.
                let said = body.split(whereSeparator: \.isNewline)
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                    .joined(separator: " · ")
                record.state = .failed(said.isEmpty
                    ? L("scp exited with status \(Int(result.status)). See the log for details.") : said)
            }
        }
        if run.isCancelled {
            Log.info("\(host): copy of \(job.name) cancelled")
        } else if result.status == 0 {
            Log.info("\(host): copied \(job.name) to \(job.destination)")
            refresh(job.destination, on: job.to)
        } else {
            Log.error("\(host): copy of \(job.name) failed with status \(result.status)")
        }
        next()
    }

    /// Show what just changed: read that directory again in every pane
    /// looking at it.
    private func refresh(_ directory: String, on source: FileSource) {
        for pane in Pane.allCases where self.source(pane) == source {
            if directory == root(pane) || nodes[pane]?[directory]?.children != nil {
                load(pane, directory)
            }
        }
    }
}
