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

/// The two trees of one host's Files window and the copies dragged between
/// them. Listings and copies run off the main thread; everything the views
/// read is published from it.
final class FilesModel: ObservableObject {
    enum Side: String {
        case remote, local

        var other: Side { self == .remote ? .local : .remote }
    }

    let host: String

    /// What each pane shows: the entries of its root, with the children of
    /// every directory that has been opened.
    @Published private(set) var trees: [Side: [FileNode]] = [.remote: [], .local: []]
    /// Bumped whenever a tree changed, which is what makes a pane reload.
    @Published private(set) var revision = 0
    @Published private(set) var roots: [Side: String] = [:]
    @Published private(set) var errors: [Side: String] = [:]
    /// The directories the user has opened, by path, so a pane can be read
    /// again without losing its shape.
    @Published private(set) var expanded: [Side: Set<String>] = [.remote: [], .local: []]
    /// Dot entries stay out of the way until the pane is asked for them.
    @Published private(set) var showHidden: [Side: Bool] = [.remote: false, .local: false]
    /// The row each pane has selected, for the copy buttons under them.
    @Published var selected: [Side: String] = [:]

    /// The copy running now, what is queued behind it and how far it is.
    @Published private(set) var active: TransferJob?
    @Published private(set) var progress: Double?
    @Published private(set) var queued = 0
    /// The line under the panes when nothing is running.
    @Published private(set) var message = ""
    @Published private(set) var messageIsError = false

    private var nodes: [Side: [String: FileNode]] = [.remote: [:], .local: [:]]
    private var loading: [Side: Set<String>] = [.remote: [], .local: []]

    private var queue: [TransferJob] = []
    private var running: TransferRun?
    /// Listings are read one after another; a copy has its own queue so a
    /// slow directory never holds it up.
    private let lister = DispatchQueue(label: "local.shellder.files.list", qos: .userInitiated)
    private let copier = DispatchQueue(label: "local.shellder.files.copy", qos: .userInitiated)

    init(host: String) {
        self.host = host
    }

    // MARK: trees

    /// Opens both trees: this Mac at the home directory, the host at
    /// ~/Shellder when it is there (what earlier versions copied into) and
    /// at its home directory otherwise.
    func start() {
        if roots[.local] == nil { setRoot(.local, Config.home) }
        guard roots[.remote] == nil else { return }
        lister.async { [weak self] in
            guard let self = self else { return }
            switch RemoteFS.home(self.host) {
            case .success(let home):
                let shelf = RemoteFS.join(home, SSH.uploadDirectory)
                let start = RemoteFS.isDirectory(self.host, shelf) ? shelf : home
                DispatchQueue.main.async { self.setRoot(.remote, start) }
            case .failure(let e):
                DispatchQueue.main.async {
                    self.errors[.remote] = e.description
                    self.revision += 1
                }
            }
        }
    }

    func root(_ side: Side) -> String { roots[side] ?? "" }

    func setRoot(_ side: Side, _ path: String) {
        roots[side] = path
        trees[side] = []
        nodes[side] = [:]
        expanded[side] = []
        errors[side] = nil
        selected[side] = nil
        revision += 1
        load(side, path)
    }

    /// The directory above the root becomes the root: the way out of a tree.
    func up(_ side: Side) {
        guard let parent = RemoteFS.parent(root(side)) else { return }
        setRoot(side, parent)
    }

    /// Read the root and every open directory again, keeping the shape.
    func reload(_ side: Side) {
        load(side, root(side))
        for dir in expanded[side] ?? [] { load(side, dir) }
    }

    func toggleHidden(_ side: Side) {
        showHidden[side] = !(showHidden[side] ?? false)
        reload(side)
    }

    /// Fold a directory open or shut, reading it the first time it opens.
    func toggle(_ side: Side, _ item: FileItem) {
        guard item.isDir else { return }
        if expanded[side]?.contains(item.path) == true {
            collapse(side, item.path)
        } else {
            expand(side, item.path)
        }
    }

    func expand(_ side: Side, _ path: String) {
        expanded[side]?.insert(path)
        if nodes[side]?[path]?.children == nil { load(side, path) } else { revision += 1 }
    }

    func collapse(_ side: Side, _ path: String) {
        expanded[side]?.remove(path)
        revision += 1
    }

    /// True while that directory is being read.
    func isLoading(_ side: Side, _ path: String) -> Bool { loading[side]?.contains(path) == true }

    private func load(_ side: Side, _ dir: String) {
        guard !dir.isEmpty, loading[side]?.contains(dir) != true else { return }
        loading[side]?.insert(dir)
        lister.async { [weak self] in
            guard let self = self else { return }
            let listed = side == .remote ? RemoteFS.list(self.host, dir) : LocalFS.list(dir)
            DispatchQueue.main.async {
                self.loading[side]?.remove(dir)
                switch listed {
                case .success(let items):
                    self.place(items, of: dir, on: side)
                    if dir == self.root(side) { self.errors[side] = nil }
                case .failure(let e):
                    self.place([], of: dir, on: side)
                    self.errors[side] = e.description
                    Log.warn("\(self.host): \(side.rawValue) \(dir): \(e.description)")
                }
                self.revision += 1
            }
        }
    }

    /// Put a listing into the tree, keeping the nodes of entries that were
    /// there before so their open directories stay open.
    private func place(_ items: [FileItem], of dir: String, on side: Side) {
        let hidden = showHidden[side] ?? false
        let kept = items
            .filter { hidden || !$0.name.hasPrefix(".") }
            .map { item -> FileNode in
                if let old = nodes[side]?[item.path], old.item.isDir == item.isDir { return old }
                return FileNode(item)
            }
        for node in kept { nodes[side]?[node.item.path] = node }
        if dir == root(side) {
            trees[side] = kept
        } else {
            nodes[side]?[dir]?.children = kept
        }
    }

    /// The lines a pane shows right now, flattened: what the outline draws.
    func rows(_ side: Side) -> [FileRow] {
        var out: [FileRow] = []
        func walk(_ list: [FileNode], _ depth: Int) {
            for node in list {
                out.append(FileRow(item: node.item, depth: depth))
                if expanded[side]?.contains(node.item.path) == true, let kids = node.children {
                    walk(kids, depth + 1)
                }
            }
        }
        walk(trees[side] ?? [], 0)
        return out
    }

    func node(_ side: Side, _ path: String) -> FileNode? { nodes[side]?[path] }

    /// Where a drop on this item goes: into a directory, or into the
    /// directory a file sits in.
    func destination(for item: FileItem, on side: Side) -> String {
        item.isDir ? item.path : (RemoteFS.parent(item.path) ?? root(side))
    }

    // MARK: drag and drop

    /// What a dragged row carries: its side and its path. Anything else
    /// dropped on a pane (text from another app) is left alone.
    static func payload(_ side: Side, _ path: String) -> String { "shellder-\(side.rawValue):\(path)" }

    static func read(_ payload: String) -> (side: Side, path: String)? {
        for side in [Side.remote, Side.local] {
            let tag = "shellder-\(side.rawValue):"
            if payload.hasPrefix(tag) { return (side, String(payload.dropFirst(tag.count))) }
        }
        return nil
    }

    /// What a pane would do with this drag: a row from the other pane is
    /// copied over, files dragged in from the Finder are uploaded.
    func canAccept(_ pasteboard: NSPasteboard, on side: Side) -> Bool {
        if let text = pasteboard.string(forType: .string), let from = FilesModel.read(text) {
            return from.side != side
        }
        return side == .remote && pasteboard.canReadObject(forClasses: [NSURL.self],
                                                           options: [.urlReadingFileURLsOnly: true])
    }

    /// A drop on `directory` of the pane `side`.
    @discardableResult
    func accept(_ pasteboard: NSPasteboard, into directory: String, on side: Side) -> Bool {
        guard canAccept(pasteboard, on: side) else { return false }
        if let text = pasteboard.string(forType: .string), let from = FilesModel.read(text) {
            enqueue(side == .remote ? .upload : .download, source: from.path, destination: directory)
            return true
        }
        let urls = pasteboard.readObjects(forClasses: [NSURL.self],
                                          options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        for url in urls { enqueue(.upload, source: url.path, destination: directory) }
        return !urls.isEmpty
    }

    // MARK: copies

    /// The selected row of one pane, copied into what the other pane shows:
    /// the same as dragging it across, for a hand that would rather click.
    func copySelection(from side: Side) {
        guard let source = selected[side], !root(side.other).isEmpty else { return }
        enqueue(side == .remote ? .download : .upload, source: source, destination: root(side.other))
    }

    func enqueue(_ direction: TransferJob.Direction, source: String, destination: String) {
        queue.append(TransferJob(direction: direction, host: host, source: source, destination: destination))
        queued = queue.count
        next()
    }

    /// Stop the copy that is running. What has already landed stays.
    func cancel() { running?.cancel() }

    private func next() {
        guard active == nil, !queue.isEmpty else { return }
        let job = queue.removeFirst()
        queued = queue.count
        active = job
        progress = nil
        let run = TransferRun()
        running = run
        Log.info("\(host): \(job.direction == .upload ? "uploading" : "downloading") \(job.source) → \(job.destination)")
        copier.async { [weak self] in
            let total = TransferRun.total(job)
            let result = run.run(job, total: total) { fraction in
                DispatchQueue.main.async {
                    guard let self = self, self.active?.id == job.id else { return }
                    self.progress = fraction
                }
            }
            DispatchQueue.main.async { self?.finished(job, run, result) }
        }
    }

    private func finished(_ job: TransferJob, _ run: TransferRun, _ result: SSH.Result) {
        active = nil
        running = nil
        progress = nil
        var body = (result.stderr + result.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
        if body.count > 2000 { body = String(body.suffix(2000)) }
        for line in body.split(separator: "\n") { Log.ssh(host, String(line)) }
        if run.isCancelled {
            message = L("\(job.name): copy cancelled")
            messageIsError = false
            Log.info("\(host): copy of \(job.name) cancelled")
        } else if result.status == 0 {
            message = "\(job.name) → \(job.destination)"
            messageIsError = false
            Log.info("\(host): copied \(job.name) to \(job.destination)")
            refresh(after: job)
        } else {
            // scp says it in a line or two; the bar has room for one.
            let said = body.split(separator: "\n")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .joined(separator: " · ")
            message = said.isEmpty ? L("scp exited with status \(Int(result.status)). See the log for details.") : said
            messageIsError = true
            Log.error("\(host): copy of \(job.name) failed with status \(result.status)")
        }
        next()
    }

    /// Show what just arrived: read the directory it landed in again.
    private func refresh(after job: TransferJob) {
        let side: Side = job.direction == .upload ? .remote : .local
        if job.destination == root(side) || nodes[side]?[job.destination]?.children != nil {
            load(side, job.destination)
        }
    }
}
