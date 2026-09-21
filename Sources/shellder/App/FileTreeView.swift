import AppKit
import SwiftUI

/// The outline view with the one thing NSOutlineView does not do by itself:
/// the delete key on the selected row.
final class FileOutlineView: NSOutlineView {
    var onDelete: (() -> Void)?

    override func keyDown(with event: NSEvent) {
        // delete and forward delete, with or without command (Finder's
        // "move to Trash" chord lands here too).
        if (event.keyCode == 51 || event.keyCode == 117), selectedRow >= 0 {
            onDelete?()
            return
        }
        super.keyDown(with: event)
    }
}

/// One pane's tree, drawn by the outline view Finder's own list view uses:
/// native disclosure triangles, selection, alternating rows, keyboard
/// navigation — and, the reason it is not a SwiftUI List, native drag and
/// drop, which a row that is also a gesture target never gets right.
struct FileTreeView: NSViewRepresentable {
    @ObservedObject var model: FilesModel
    let pane: FilesModel.Pane

    func makeCoordinator() -> Coordinator { Coordinator(model: model, pane: pane) }

    func makeNSView(context: Context) -> NSScrollView {
        let outline = FileOutlineView()
        outline.dataSource = context.coordinator
        outline.delegate = context.coordinator
        outline.headerView = NSTableHeaderView()
        outline.style = .inset
        outline.rowHeight = 20
        outline.usesAlternatingRowBackgroundColors = true
        outline.allowsMultipleSelection = false
        outline.indentationPerLevel = 14
        outline.autoresizesOutlineColumn = false
        outline.doubleAction = #selector(Coordinator.opened(_:))
        outline.target = context.coordinator
        outline.onDelete = { [weak coordinator = context.coordinator] in coordinator?.deleteSelection() }
        outline.menu = context.coordinator.contextMenu()

        let name = NSTableColumn(identifier: .name)
        name.title = L("Name")
        name.minWidth = 140
        name.width = 220
        outline.addTableColumn(name)
        outline.outlineTableColumn = name

        let size = NSTableColumn(identifier: .size)
        size.title = L("Size")
        size.minWidth = 70
        size.width = 100
        size.headerCell.alignment = .right
        outline.addTableColumn(size)

        // Rows are dragged to the other pane, files come in from the other
        // pane or from the Finder.
        outline.setDraggingSourceOperationMask(.copy, forLocal: true)
        outline.registerForDraggedTypes([.string, .fileURL])

        let scroll = NSScrollView()
        scroll.documentView = outline
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        context.coordinator.outline = outline
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.model = model
        context.coordinator.apply(revision: model.revision)
    }

    final class Coordinator: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate, NSMenuDelegate {
        var model: FilesModel
        let pane: FilesModel.Pane
        weak var outline: FileOutlineView?
        private var shown = -1
        /// True while the model is driving the outline, so the outline's own
        /// notifications do not drive the model back.
        private var applying = false

        init(model: FilesModel, pane: FilesModel.Pane) {
            self.model = model
            self.pane = pane
        }

        private var isLocal: Bool { model.source(pane).isLocal }

        /// Redraw when the model says the tree changed, then put the open
        /// directories and the selection back.
        func apply(revision: Int) {
            guard let outline = outline, revision != shown else { return }
            shown = revision
            applying = true
            outline.reloadData()
            applyExpansion(of: nil)
            if let path = model.selected[pane], let node = model.node(pane, path) {
                let row = outline.row(forItem: node)
                if row >= 0 {
                    outline.selectRowIndexes([row], byExtendingSelection: false)
                    outline.scrollRowToVisible(row)
                }
            } else {
                outline.deselectAll(nil)
            }
            applying = false
        }

        private func applyExpansion(of parent: FileNode?) {
            guard let outline = outline else { return }
            let open = model.expanded[pane] ?? []
            let children = parent?.children ?? model.trees[pane] ?? []
            for node in children where node.item.isDir {
                if open.contains(node.item.path) {
                    outline.expandItem(node)
                    applyExpansion(of: node)
                } else {
                    outline.collapseItem(node)
                }
            }
        }

        @objc func opened(_ sender: NSOutlineView) {
            guard sender.clickedRow >= 0,
                  let node = sender.item(atRow: sender.clickedRow) as? FileNode else { return }
            if node.item.isDir { model.setRoot(pane, node.item.path) }
        }

        // MARK: data

        func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
            guard let node = item as? FileNode else { return (model.trees[pane] ?? []).count }
            return node.children?.count ?? 0
        }

        func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
            guard let node = item as? FileNode else { return (model.trees[pane] ?? [])[index] }
            return node.children?[index] ?? node
        }

        func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
            (item as? FileNode)?.item.isDir ?? false
        }

        func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
            guard let node = item as? FileNode, let column = tableColumn else { return nil }
            let cell = (outlineView.makeView(withIdentifier: column.identifier, owner: self) as? NSTableCellView)
                ?? FileTreeView.cell(for: column.identifier)
            if column.identifier == .name {
                cell.imageView?.image = isLocal ? FileIcons.local(node.item.path)
                                                : FileIcons.remote(node.item)
                cell.textField?.stringValue = node.item.name
            } else {
                cell.textField?.stringValue = node.item.isDir ? "--" : Fmt.bytes(node.item.size)
            }
            return cell
        }

        // MARK: what the user does to it

        func outlineViewItemWillExpand(_ notification: Notification) {
            guard !applying, let node = notification.userInfo?["NSObject"] as? FileNode else { return }
            model.expand(pane, node.item.path)
        }

        func outlineViewItemWillCollapse(_ notification: Notification) {
            guard !applying, let node = notification.userInfo?["NSObject"] as? FileNode else { return }
            model.collapse(pane, node.item.path)
        }

        func outlineViewSelectionDidChange(_ notification: Notification) {
            guard !applying, let outline = outline else { return }
            let node = outline.item(atRow: outline.selectedRow) as? FileNode
            model.selected[pane] = node?.item.path
        }

        // MARK: the menu under a right click

        func contextMenu() -> NSMenu {
            let menu = NSMenu()
            menu.delegate = self
            menu.addItem(withTitle: L("New Folder"), action: #selector(newFolder), keyEquivalent: "")
            menu.addItem(withTitle: L("New File"), action: #selector(newFile), keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(withTitle: L("Delete"), action: #selector(deleteClicked), keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(withTitle: L("Read this directory again"), action: #selector(refresh), keyEquivalent: "")
            for item in menu.items { item.target = self }
            return menu
        }

        /// A right click acts on the row under it, so it selects that row
        /// first — the way a Finder click does.
        func menuNeedsUpdate(_ menu: NSMenu) {
            guard let outline = outline else { return }
            let row = outline.clickedRow
            if row >= 0, let node = outline.item(atRow: row) as? FileNode {
                outline.selectRowIndexes([row], byExtendingSelection: false)
                model.selected[pane] = node.item.path
            }
            let onRow = model.selected[pane] != nil
            menu.item(withTitle: L("Delete"))?.isEnabled = onRow
            menu.item(withTitle: L("Delete"))?.title = model.source(pane).isLocal ? L("Move to Trash") : L("Delete")
        }

        @objc private func newFolder() { FileActions.newFolder(model, pane) }
        @objc private func newFile() { FileActions.newFile(model, pane) }
        @objc private func deleteClicked() { deleteSelection() }
        @objc private func refresh() { model.reload(pane) }

        func deleteSelection() { FileActions.delete(model, pane) }

        // MARK: drag and drop

        func outlineView(_ outlineView: NSOutlineView, pasteboardWriterForItem item: Any) -> NSPasteboardWriting? {
            guard let node = item as? FileNode else { return nil }
            let entry = NSPasteboardItem()
            entry.setString(FilesModel.payload(model.source(pane), node.item.path), forType: .string)
            return entry
        }

        func outlineView(_ outlineView: NSOutlineView, validateDrop info: NSDraggingInfo,
                         proposedItem item: Any?, proposedChildIndex index: Int) -> NSDragOperation {
            guard model.canAccept(info.draggingPasteboard, on: pane) else { return [] }
            // A copy always lands *in* a directory: a drop between rows, or
            // on a file, is retargeted to the directory around it.
            var target = item as? FileNode
            if let node = target, !node.item.isDir { target = outlineView.parent(forItem: node) as? FileNode }
            if index != NSOutlineViewDropOnItemIndex || target !== (item as? FileNode) {
                outlineView.setDropItem(target, dropChildIndex: NSOutlineViewDropOnItemIndex)
            }
            return .copy
        }

        func outlineView(_ outlineView: NSOutlineView, acceptDrop info: NSDraggingInfo,
                         item: Any?, childIndex index: Int) -> Bool {
            let directory = (item as? FileNode)?.item.path ?? model.root(pane)
            return model.accept(info.draggingPasteboard, into: directory, on: pane)
        }
    }

    /// The two kinds of cell: an icon with a name, and a right-aligned size.
    private static func cell(for identifier: NSUserInterfaceItemIdentifier) -> NSTableCellView {
        let cell = NSTableCellView()
        cell.identifier = identifier
        let text = NSTextField(labelWithString: "")
        text.font = .systemFont(ofSize: 13)
        text.lineBreakMode = .byTruncatingMiddle
        text.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(text)
        cell.textField = text

        if identifier == .name {
            let image = NSImageView()
            image.imageScaling = .scaleProportionallyDown
            image.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(image)
            cell.imageView = image
            NSLayoutConstraint.activate([
                image.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 2),
                image.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
                image.widthAnchor.constraint(equalToConstant: 16),
                image.heightAnchor.constraint(equalToConstant: 16),
                text.leadingAnchor.constraint(equalTo: image.trailingAnchor, constant: 5),
                text.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -4),
                text.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
        } else {
            text.alignment = .right
            text.textColor = .secondaryLabelColor
            text.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
            NSLayoutConstraint.activate([
                text.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 4),
                text.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -6),
                text.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
        }
        return cell
    }
}

extension NSUserInterfaceItemIdentifier {
    static let name = NSUserInterfaceItemIdentifier("name")
    static let size = NSUserInterfaceItemIdentifier("size")
}
