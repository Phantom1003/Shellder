import AppKit
import SwiftUI
import UniformTypeIdentifiers

extension FileSource {
    var title: String { host ?? L("Local") }
    var symbol: String { isLocal ? "laptopcomputer" : "server.rack" }
}

/// Finder's own icons: the real one for a path on this Mac (folders with a
/// picture on them, app bundles, aliases), and the one macOS would give a
/// name like it on a host. Both are kept, they are asked for on every row.
enum FileIcons {
    private static var byPath: [String: NSImage] = [:]
    private static var byKind: [String: NSImage] = [:]

    static func local(_ path: String) -> NSImage {
        if let cached = byPath[path] { return cached }
        let icon = NSWorkspace.shared.icon(forFile: path)
        byPath[path] = icon
        return icon
    }

    static func remote(_ item: FileItem) -> NSImage {
        let kind = item.isDir ? "/" : (item.name as NSString).pathExtension.lowercased()
        if let cached = byKind[kind] { return cached }
        let type: UTType = item.isDir ? .folder : (UTType(filenameExtension: kind) ?? .data)
        let icon = NSWorkspace.shared.icon(for: type)
        byKind[kind] = icon
        return icon
    }
}

/// The questions a pane asks before it changes anything: a name for a new
/// folder or file, a path to go to, and the one that deletes.
enum FileActions {
    static func newFolder(_ model: FilesModel, _ pane: FilesModel.Pane) {
        guard let name = ask(title: L("New Folder"),
                             message: L("It goes into \(model.target(pane))"),
                             preset: L("untitled folder"), action: L("Create")) else { return }
        model.createDirectory(pane, named: name)
    }

    static func newFile(_ model: FilesModel, _ pane: FilesModel.Pane) {
        guard let name = ask(title: L("New File"),
                             message: L("It goes into \(model.target(pane))"),
                             preset: L("untitled.txt"), action: L("Create")) else { return }
        model.createFile(pane, named: name)
    }

    static func goToFolder(_ model: FilesModel, _ pane: FilesModel.Pane) {
        guard let path = ask(title: L("Go to Folder"),
                             message: L("A path on \(model.source(pane).title)"),
                             preset: model.root(pane), action: L("Go")) else { return }
        model.setRoot(pane, path)
    }

    /// Deleting on this Mac is the Trash and can be taken back; on a host it
    /// cannot, so the question says so and Cancel is what Return presses.
    static func delete(_ model: FilesModel, _ pane: FilesModel.Pane) {
        let paths = model.selection(pane)
        guard let path = paths.first else { return }
        let name = (path as NSString).lastPathComponent
        let source = model.source(pane)
        let alert = NSAlert()
        alert.alertStyle = .warning
        if paths.count == 1 {
            alert.messageText = source.isLocal ? L("Move “\(name)” to the Trash?")
                                               : L("Delete “\(name)” on \(source.title)?")
        } else {
            alert.messageText = source.isLocal ? L("Move \(paths.count) items to the Trash?")
                                               : L("Delete \(paths.count) items on \(source.title)?")
        }
        if !source.isLocal { alert.informativeText = L("This cannot be undone.") }
        alert.addButton(withTitle: source.isLocal ? L("Move to Trash") : L("Delete"))
            .hasDestructiveAction = true
        alert.addButton(withTitle: L("Cancel")).keyEquivalent = "\u{1b}"
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        model.delete(pane, paths)
    }

    /// scp writes over what is already there without a word, so this is
    /// asked first. With more copies waiting, one answer can cover them all.
    static func replace(_ job: TransferJob, offerAll: Bool) -> FilesModel.ReplaceAnswer {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = L("Replace “\(job.name)”?")
        alert.informativeText = L("\(job.to.title):\(job.destination) already has one.")
        // Like the delete question: the dangerous answer is the one on the
        // right, in red, and Escape is the way out.
        alert.addButton(withTitle: L("Replace")).hasDestructiveAction = true
        alert.addButton(withTitle: L("Skip")).keyEquivalent = "\u{1b}"
        if offerAll {
            alert.showsSuppressionButton = true
            alert.suppressionButton?.title = L("Apply to all")
        }
        let replacing = alert.runModal() == .alertFirstButtonReturn
        let all = alert.suppressionButton?.state == .on
        switch (replacing, all) {
        case (true, false): return .replace
        case (true, true): return .replaceAll
        case (false, false): return .skip
        case (false, true): return .skipAll
        }
    }

    private static func ask(title: String, message: String, preset: String, action: String) -> String? {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.addButton(withTitle: action)
        alert.addButton(withTitle: L("Cancel"))
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 300, height: 22))
        field.stringValue = preset
        alert.accessoryView = field
        alert.window.initialFirstResponder = field
        field.currentEditor()?.selectAll(nil)
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        let text = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }
}

/// Finder's path bar (NSPathControl): the way out of a tree is clicking a
/// directory above it. The local side hands it the URL, so it draws the
/// folders with their own icons; a host has no URL, so its components are
/// built by hand.
struct PathBar: NSViewRepresentable {
    let path: String
    let isLocal: Bool
    let jump: (String) -> Void

    func makeNSView(context: Context) -> NSPathControl {
        let control = NSPathControl()
        control.pathStyle = .standard
        control.isEditable = false
        control.focusRingType = .none
        control.controlSize = .small
        control.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        control.target = context.coordinator
        control.action = #selector(Coordinator.clicked(_:))
        control.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        control.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return control
    }

    func updateNSView(_ control: NSPathControl, context: Context) {
        context.coordinator.jump = jump
        context.coordinator.isLocal = isLocal
        if isLocal {
            control.url = URL(fileURLWithPath: path)
        } else {
            control.url = nil
            let folder = NSWorkspace.shared.icon(for: .folder)
            let root = NSPathControlItem()
            root.title = "/"
            root.image = folder
            var items = [root]
            for part in path.split(separator: "/") {
                let item = NSPathControlItem()
                item.title = String(part)
                item.image = folder
                items.append(item)
            }
            control.pathItems = items
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator: NSObject {
        var jump: (String) -> Void = { _ in }
        var isLocal = true

        @objc func clicked(_ sender: NSPathControl) {
            guard let item = sender.clickedPathItem else { return }
            if isLocal {
                if let url = item.url { jump(url.path) }
            } else if let index = sender.pathItems.firstIndex(where: { $0 === item }) {
                // Item 0 is the root; the rest are the components under it.
                let parts = sender.pathItems.dropFirst().prefix(index).map(\.title)
                jump("/" + parts.joined(separator: "/"))
            }
        }
    }
}

/// The window behind the Files tool: two trees, either of which can be this
/// Mac or a connected host, and the copies between them underneath.
struct FilesView: View {
    @ObservedObject var model: FilesModel
    @AppStorage("filesTransferHeight", store: Prefs.defaults) private var transferHeight = 160.0

    var body: some View {
        VStack(spacing: 0) {
            HSplitView {
                FilePane(model: model, pane: .left)
                FilePane(model: model, pane: .right)
            }
            if model.showHistory {
                ResizeHandle(height: $transferHeight, range: 60...600)
            } else {
                Divider()
            }
            TransferBar(model: model, height: transferHeight)
        }
        .frame(minWidth: 760, minHeight: 420)
        .onAppear {
            model.askReplace = { job, more in FileActions.replace(job, offerAll: more) }
            model.start()
        }
    }
}

/// One side: which machine it looks at, its tree, and the path bar out of it.
struct FilePane: View {
    @ObservedObject var model: FilesModel
    let pane: FilesModel.Pane

    private var source: FileSource { model.source(pane) }

    /// The same box for every icon up here: a symbol with a badge on it
    /// (new folder, new file) is wider than a plain one and would be cut.
    private func icon(_ name: String) -> some View {
        Image(systemName: name)
            .font(.system(size: 13))
            .frame(width: 24, height: 20)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Menu {
                    Button { model.setSource(pane, .local) } label: {
                        Label(L("Local"), systemImage: "laptopcomputer")
                    }
                    if !model.hosts.isEmpty {
                        Divider()
                        ForEach(model.hosts, id: \.self) { alias in
                            Button { model.setSource(pane, .host(alias)) } label: {
                                Label(alias, systemImage: "server.rack")
                            }
                        }
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: source.symbol).foregroundColor(.secondary)
                        Text(source.title).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                    }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help(L("Show another machine in this pane"))

                Menu {
                    ForEach(model.shortcuts(pane)) { shortcut in
                        Button(shortcut.title) { model.setRoot(pane, shortcut.path) }
                    }
                    Divider()
                    Button(L("Go to Folder…")) { FileActions.goToFolder(model, pane) }
                } label: {
                    icon("folder")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help(L("Go to a folder on \(source.title)"))

                Spacer(minLength: 4)

                Button { FileActions.newFolder(model, pane) } label: { icon("folder.badge.plus") }
                .accessibilityLabel(L("New Folder"))
                .help(L("New Folder"))
                Button { FileActions.newFile(model, pane) } label: { icon("doc.badge.plus") }
                .accessibilityLabel(L("New File"))
                .help(L("New File"))
                Button { FileActions.delete(model, pane) } label: { icon("trash") }
                    .disabled((model.selected[pane] ?? []).isEmpty)
                    .accessibilityLabel(source.isLocal ? L("Move to Trash") : L("Delete"))
                    .help(source.isLocal ? L("Move to Trash") : L("Delete"))
                Button { model.toggleHidden(pane) } label: {
                    icon(model.showHidden[pane] == true ? "eye" : "eye.slash")
                }
                .accessibilityLabel(L("Show the entries whose name starts with a dot"))
                .help(L("Show the entries whose name starts with a dot"))
                Button { model.back(pane) } label: { icon("chevron.left") }
                    .disabled(!model.canGoBack(pane))
                    .accessibilityLabel(L("Back to the directory before this one"))
                    .help(L("Back to the directory before this one"))
                Button { model.up(pane) } label: { icon("arrow.up") }
                    .accessibilityLabel(L("Up one directory"))
                    .help(L("Up one directory"))
                Button { model.reload(pane) } label: { icon("arrow.clockwise") }
                    .accessibilityLabel(L("Refresh"))
                    .help(L("Refresh"))
                Divider().frame(height: 14)
                Button { model.copySelection(from: pane) } label: {
                    icon(pane == .left ? "arrowshape.right.fill" : "arrowshape.left.fill")
                }
                .disabled(!model.canCopySelection(from: pane))
                .accessibilityLabel(L("Copy what is selected to \(model.source(pane.other).title)"))
                .help(L("Copy what is selected to \(model.source(pane.other).title)"))
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 10)
            .frame(height: 40)
            .background(.bar)
            Divider()

            FileTreeView(model: model, pane: pane)

            if let error = model.errors[pane] {
                Divider()
                HStack(spacing: 5) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundColor(.orange)
                    Text(error).lineLimit(2).textSelection(.enabled)
                }
                .font(.caption)
                .padding(.horizontal, 12)
                .padding(.vertical, 5)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.bar)
            }

            Divider()
            PathBar(path: model.root(pane), isLocal: source.isLocal) { model.setRoot(pane, $0) }
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: 22)
                .padding(.horizontal, 8)
                .padding(.vertical, 2)
                .background(.bar)
        }
        .frame(minWidth: 360)
    }
}

/// Under the panes, in one strip: what is being copied now and — folded
/// out — every copy this window has run, so a transfer is something to look
/// back at rather than a bar that flashes past.
struct TransferBar: View {
    @ObservedObject var model: FilesModel
    /// How tall the list under the strip is, dragged by the handle above it.
    var height: Double = 160

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Button { model.showHistory.toggle() } label: {
                    Image(systemName: "chevron.right")
                        .rotationEffect(.degrees(model.showHistory ? 90 : 0))
                        .foregroundColor(.secondary)
                        .frame(width: 12)
                }
                .help(L("Every copy this window has run"))
                Text(L("Transfers")).fontWeight(.semibold).foregroundColor(.secondary)

                if let record = model.active {
                    Image(systemName: record.job.isUpload ? "arrow.up.circle" : "arrow.down.circle")
                        .foregroundColor(.accentColor)
                    Text("\(record.job.name) → \(record.job.destination)")
                        .lineLimit(1).truncationMode(.middle)
                    if let p = record.progress {
                        ProgressView(value: p).controlSize(.small).frame(width: 120)
                        Text("\(Int(p * 100))%").monospacedDigit().foregroundColor(.secondary)
                    } else {
                        ProgressView().progressViewStyle(.linear).controlSize(.small).frame(width: 120)
                    }
                    Button { model.cancel() } label: { Image(systemName: "xmark.circle.fill") }
                        .foregroundColor(.secondary)
                        .help(L("Stop this copy"))
                    if model.queued > 0 {
                        Text(L("+\(model.queued) waiting")).foregroundColor(.secondary)
                    }
                } else if !model.notice.isEmpty {
                    Image(systemName: model.noticeIsError ? "exclamationmark.triangle.fill" : "checkmark.circle")
                        .foregroundColor(model.noticeIsError ? .orange : .secondary)
                    Text(model.notice)
                        .foregroundColor(model.noticeIsError ? .red : .secondary)
                        .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                } else if let last = model.lastFinished {
                    TransferState(record: last)
                    Text("\(last.job.name) → \(last.job.destination)")
                        .foregroundColor(.secondary)
                        .lineLimit(1).truncationMode(.middle)
                }

                Spacer(minLength: 0)
                Button(L("Clear")) { model.clearHistory() }
                    .disabled(!model.transfers.contains { $0.isOver })
            }
            .buttonStyle(.borderless)
            .font(.system(size: 11))
            .padding(.horizontal, 12)
            .frame(height: 26)
            .background(.bar)

            if model.showHistory {
                Divider()
                TransferHistory(model: model).frame(height: height)
            }
        }
    }
}

/// The list itself: the one running, the ones waiting, and what became of
/// the ones before them.
struct TransferHistory: View {
    @ObservedObject var model: FilesModel

    var body: some View {
        Group {
            if model.transfers.isEmpty {
                Text(L("Nothing has been copied yet."))
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                // Rows as wide as the strip above them, with the stripes
                // drawn here: the inset list style would pull them in from
                // both edges and nothing else in the window is inset.
                List {
                    ForEach(Array(model.transfers.reversed().enumerated()), id: \.element.id) { index, record in
                        TransferRow(model: model, record: record)
                            .listRowInsets(EdgeInsets(top: 0, leading: 12, bottom: 0, trailing: 12))
                            .listRowSeparator(.hidden)
                            .listRowBackground(index.isMultiple(of: 2) ? Color.clear
                                : Color(nsColor: .alternatingContentBackgroundColors[1]))
                    }
                }
                .listStyle(.plain)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// One copy as a line of the list: the same height as a row in a pane, so
/// the two read as one window rather than a list of little cards.
struct TransferRow: View {
    @ObservedObject var model: FilesModel
    let record: TransferRecord

    var body: some View {
        HStack(spacing: 8) {
            TransferState(record: record)
            Text(record.job.name)
                .font(.system(size: 12))
                .lineLimit(1)
                .layoutPriority(1)
            Text("\(record.job.from.title):\(record.job.source)  →  \(record.job.to.title):\(record.job.destination)")
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            if record.state == .running, let p = record.progress {
                ProgressView(value: p).controlSize(.small).frame(width: 90)
                Text("\(Int(p * 100))%").font(.system(size: 10)).monospacedDigit().foregroundColor(.secondary)
            } else {
                Text(status)
                    .font(.system(size: 10))
                    .foregroundColor(isError ? .red : .secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: 300, alignment: .trailing)
            }
            if record.canRetry {
                Button { model.retry(record) } label: {
                    Image(systemName: "arrow.clockwise").font(.system(size: 10))
                }
                .buttonStyle(.borderless)
                .help(L("Try Again"))
            }
        }
        .frame(height: 22)
        .help(detail)
        .contextMenu {
            if record.canRetry {
                Button(L("Try Again")) { model.retry(record) }
            }
        }
    }

    private var isError: Bool {
        if case .failed = record.state { return true }
        return false
    }

    private var status: String {
        switch record.state {
        case .waiting: return L("Waiting")
        case .running: return L("Copying…")
        case .cancelled: return L("Cancelled")
        case .skipped: return L("Skipped, it was already there")
        case .failed(let why): return why
        case .done:
            let size = record.total.map { Fmt.bytes($0) + " · " } ?? ""
            return size + Fmt.time(record.endedAt ?? record.queuedAt)
        }
    }

    private var detail: String {
        "\(record.job.from.title):\(record.job.source) → \(record.job.to.title):\(record.job.destination)\n\(status)"
    }
}

/// The little icon that says how a copy is doing.
struct TransferState: View {
    let record: TransferRecord

    var body: some View {
        Image(systemName: symbol).foregroundColor(color)
    }

    private var symbol: String {
        switch record.state {
        case .waiting: return "clock"
        case .running: return record.job.isUpload ? "arrow.up.circle" : "arrow.down.circle"
        case .done: return "checkmark.circle"
        case .cancelled: return "xmark.circle"
        case .skipped: return "minus.circle"
        case .failed: return "exclamationmark.triangle.fill"
        }
    }

    private var color: Color {
        switch record.state {
        case .running: return .accentColor
        case .failed: return .orange
        default: return .secondary
        }
    }
}
