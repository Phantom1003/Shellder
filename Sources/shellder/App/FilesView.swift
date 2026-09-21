import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Finder's own icons: the real one for a path on this Mac (folders with a
/// picture on them, app bundles, aliases), and the one macOS would give a
/// name like it on the host. Both are kept, they are asked for on every row.
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

/// Finder's path bar (NSPathControl): the way out of a tree is clicking a
/// directory above it. The local side hands it the URL, so it draws the
/// folders with their own icons; the host's side has no URL, so its
/// components are built by hand.
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

/// The window behind the Files tool: the host's tree on the left, this
/// Mac's on the right, what is being copied underneath.
struct FilesView: View {
    @ObservedObject var model: FilesModel

    var body: some View {
        VStack(spacing: 0) {
            HSplitView {
                FilePane(model: model, side: .remote)
                FilePane(model: model, side: .local)
            }
            Divider()
            TransferBar(model: model)
        }
        .frame(minWidth: 720, minHeight: 380)
        .onAppear { model.start() }
    }
}

/// One side: where it is rooted, its tree, and the path bar out of it.
struct FilePane: View {
    @ObservedObject var model: FilesModel
    let side: FilesModel.Side

    private var isRemote: Bool { side == .remote }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: isRemote ? "server.rack" : "laptopcomputer")
                    .foregroundColor(.secondary)
                Text(isRemote ? model.host : L("This Mac"))
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
                Spacer(minLength: 4)
                Button { model.toggleHidden(side) } label: {
                    Image(systemName: model.showHidden[side] == true ? "eye" : "eye.slash")
                }
                .accessibilityLabel(L("Show the entries whose name starts with a dot"))
                .help(L("Show the entries whose name starts with a dot"))
                Button { model.up(side) } label: { Image(systemName: "arrow.up") }
                    .accessibilityLabel(L("Up one directory"))
                    .help(L("Up one directory"))
                Button { model.reload(side) } label: { Image(systemName: "arrow.clockwise") }
                    .accessibilityLabel(L("Read this directory again"))
                    .help(L("Read this directory again"))
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 12)
            .frame(height: 38)
            .background(.bar)
            Divider()

            FileTreeView(model: model, side: side)

            if let error = model.errors[side] {
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
            PathBar(path: model.root(side), isLocal: !isRemote) { model.setRoot(side, $0) }
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: 22)
                .padding(.horizontal, 8)
                .padding(.vertical, 2)
                .background(.bar)
        }
        .frame(minWidth: 300)
    }
}

/// Under the panes, where Finder puts its status bar: what is being copied,
/// how far it is, what is waiting, and how the last copy ended. The two
/// arrows copy what a pane has selected, for a hand that would rather click
/// than drag.
struct TransferBar: View {
    @ObservedObject var model: FilesModel

    var body: some View {
        HStack(spacing: 10) {
            if let job = model.active {
                Image(systemName: job.direction == .upload ? "arrow.up.circle" : "arrow.down.circle")
                    .foregroundColor(.accentColor)
                Text("\(job.name) → \(job.destination)")
                    .lineLimit(1).truncationMode(.middle)
                if let p = model.progress {
                    ProgressView(value: p).controlSize(.small).frame(width: 150)
                    Text("\(Int(p * 100))%").monospacedDigit().foregroundColor(.secondary)
                } else {
                    ProgressView().progressViewStyle(.linear).controlSize(.small).frame(width: 150)
                }
                if model.queued > 0 {
                    Text(L("+\(model.queued) waiting")).foregroundColor(.secondary)
                }
                Button { model.cancel() } label: { Image(systemName: "xmark.circle.fill") }
                    .buttonStyle(.borderless)
                    .foregroundColor(.secondary)
                    .help(L("Stop this copy"))
            } else {
                Image(systemName: "arrow.left.arrow.right").foregroundColor(.secondary)
                Text(model.message.isEmpty ? L("Drag a file from one side to the other to copy it.") : model.message)
                    .foregroundColor(model.messageIsError ? .red : .secondary)
                    .lineLimit(2)
                    .textSelection(.enabled)
            }
            Spacer(minLength: 0)
            Button { model.copySelection(from: .local) } label: { Image(systemName: "arrow.left") }
                .disabled(model.selected[.local] == nil)
                .help(L("Copy what is selected here to \(model.host)"))
            Button { model.copySelection(from: .remote) } label: { Image(systemName: "arrow.right") }
                .disabled(model.selected[.remote] == nil)
                .help(L("Copy what is selected on \(model.host) to this Mac"))
        }
        .buttonStyle(.borderless)
        .font(.system(size: 11))
        .padding(.horizontal, 12)
        .frame(height: 28)
        .background(.bar)
    }
}
