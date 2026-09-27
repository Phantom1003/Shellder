import AppKit
import Quartz
import SwiftUI

/// The outline view with what NSOutlineView does not do by itself: the
/// delete key on the selected row, and Quick Look on the space bar for the
/// files of this Mac.
final class FileOutlineView: NSOutlineView {
    var onDelete: (() -> Void)?
    weak var preview: FilePreviewController?

    override func keyDown(with event: NSEvent) {
        // delete and forward delete, with or without command (Finder's
        // "move to Trash" chord lands here too).
        if (event.keyCode == 51 || event.keyCode == 117), selectedRow >= 0 {
            onDelete?()
            return
        }
        if event.keyCode == 49, event.modifierFlags.isDisjoint(with: [.command, .option, .control]),
           selectedRow >= 0, preview?.canPreview == true {
            preview?.toggle()
            return
        }
        super.keyDown(with: event)
    }

    /// An open panel follows the keyboard from one pane to the other, and
    /// goes when the keyboard moves to a host's pane: there is nothing there
    /// it would show.
    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became, QLPreviewPanel.sharedPreviewPanelExists(), QLPreviewPanel.shared().isVisible {
            if preview?.canPreview == true {
                QLPreviewPanel.shared().updateController()
            } else {
                QLPreviewPanel.shared().orderOut(nil)
            }
        }
        return became
    }

    override func acceptsPreviewPanelControl(_ panel: QLPreviewPanel!) -> Bool { preview?.canPreview == true }
    override func beginPreviewPanelControl(_ panel: QLPreviewPanel!) { preview?.begin(panel) }
    override func endPreviewPanelControl(_ panel: QLPreviewPanel!) { preview?.end(panel) }
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
        outline.allowsMultipleSelection = true
        outline.indentationPerLevel = 14
        outline.autoresizesOutlineColumn = false
        outline.doubleAction = #selector(Coordinator.opened(_:))
        outline.target = context.coordinator
        outline.onDelete = { [weak coordinator = context.coordinator] in coordinator?.deleteSelection() }
        outline.menu = context.coordinator.contextMenu()
        outline.preview = context.coordinator.preview
        context.coordinator.preview.outline = outline

        // The name takes what the pane gives or takes away (the outline has
        // to follow the pane's width for that): the date and the size keep
        // the width they were given. A pane narrower than the columns it
        // started with does not shrink them, it cuts the size off, so they
        // start narrow enough for the narrowest pane and grow from there.
        // A click on a header sorts by that column, a second one turns the
        // order round. A date or a size starts with the newest, the
        // biggest, as in the Finder.
        outline.autoresizingMask = [.width]
        outline.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        let name = NSTableColumn(identifier: .name)
        name.title = L("Name")
        name.minWidth = 100
        name.width = 110
        name.resizingMask = [.autoresizingMask, .userResizingMask]
        name.sortDescriptorPrototype = NSSortDescriptor(key: FileSort.Key.name.rawValue, ascending: true)
        outline.addTableColumn(name)
        outline.outlineTableColumn = name

        let modified = NSTableColumn(identifier: .modified)
        modified.title = L("Date Modified")
        modified.minWidth = 70
        modified.width = 135
        modified.resizingMask = .userResizingMask
        modified.sortDescriptorPrototype = NSSortDescriptor(key: FileSort.Key.modified.rawValue, ascending: false)
        outline.addTableColumn(modified)

        let size = NSTableColumn(identifier: .size)
        size.title = L("Size")
        size.minWidth = 50
        size.width = 62
        size.resizingMask = .userResizingMask
        size.headerCell.alignment = .right
        size.sortDescriptorPrototype = NSSortDescriptor(key: FileSort.Key.size.rawValue, ascending: false)
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
        context.coordinator.preview.model = model
        context.coordinator.apply(revision: model.revision)
    }

    final class Coordinator: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate, NSMenuDelegate,
                             NSMenuItemValidation {
        var model: FilesModel
        let pane: FilesModel.Pane
        weak var outline: FileOutlineView?
        let preview: FilePreviewController
        private var shown = -1
        /// True while the model is driving the outline, so the outline's own
        /// notifications do not drive the model back.
        private var applying = false

        init(model: FilesModel, pane: FilesModel.Pane) {
            self.model = model
            self.pane = pane
            preview = FilePreviewController(model: model, pane: pane)
        }

        private var isLocal: Bool { model.source(pane).isLocal }

        /// Redraw when the model says the tree changed, then put the open
        /// directories and the selection back.
        func apply(revision: Int) {
            guard let outline = outline, revision != shown else { return }
            shown = revision
            applying = true
            let sort = model.sort(pane)
            if outline.sortDescriptors.first?.key != sort.key.rawValue
                || outline.sortDescriptors.first?.ascending != sort.ascending {
                outline.sortDescriptors = [NSSortDescriptor(key: sort.key.rawValue, ascending: sort.ascending)]
            }
            outline.reloadData()
            applyExpansion(of: nil)
            let rows = (model.selected[pane] ?? []).compactMap { path -> Int? in
                guard let node = model.node(pane, path) else { return nil }
                let row = outline.row(forItem: node)
                return row >= 0 ? row : nil
            }
            if rows.isEmpty {
                outline.deselectAll(nil)
            } else {
                outline.selectRowIndexes(IndexSet(rows), byExtendingSelection: false)
                if let first = rows.min() { outline.scrollRowToVisible(first) }
            }
            applying = false
            preview.selectionChanged()
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

        /// A directory becomes the root, a file of this Mac opens in Quick Look.
        @objc func opened(_ sender: NSOutlineView) {
            guard sender.clickedRow >= 0,
                  let node = sender.item(atRow: sender.clickedRow) as? FileNode else { return }
            if node.item.isDir { model.setRoot(pane, node.item.path) } else { preview.show() }
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
            } else if column.identifier == .modified {
                cell.textField?.stringValue = node.item.modified.map { date in
                    FileTreeView.date(date, fitting: column.width - 10, font: cell.textField?.font)
                } ?? "--"
            } else {
                cell.textField?.stringValue = node.item.isDir ? "--" : Fmt.bytes(node.item.size)
            }
            return cell
        }

        func outlineView(_ outlineView: NSOutlineView, sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]) {
            guard !applying, let first = outlineView.sortDescriptors.first,
                  let key = first.key.flatMap(FileSort.Key.init(rawValue:)) else { return }
            model.setSort(pane, FileSort(key: key, ascending: first.ascending))
        }

        /// A narrower date column writes its dates shorter.
        func outlineViewColumnDidResize(_ notification: Notification) {
            guard let outline = outline,
                  (notification.userInfo?["NSTableColumn"] as? NSTableColumn)?.identifier == .modified else { return }
            let rows = outline.rows(in: outline.visibleRect)
            outline.reloadData(forRowIndexes: IndexSet(integersIn: rows.location..<rows.location + rows.length),
                               columnIndexes: IndexSet(integer: outline.column(withIdentifier: .modified)))
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
            var picked: Set<String> = []
            for row in outline.selectedRowIndexes {
                if let node = outline.item(atRow: row) as? FileNode { picked.insert(node.item.path) }
            }
            model.selected[pane] = picked
            preview.selectionChanged()
        }

        // MARK: the menu under a right click

        func contextMenu() -> NSMenu {
            let menu = NSMenu()
            menu.delegate = self
            menu.addItem(withTitle: L("Quick Look"), action: #selector(quickLook), keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(withTitle: L("New Folder"), action: #selector(newFolder), keyEquivalent: "")
            menu.addItem(withTitle: L("New File"), action: #selector(newFile), keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(withTitle: L("Delete"), action: #selector(deleteClicked), keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(withTitle: L("Refresh"), action: #selector(refresh), keyEquivalent: "")
            for item in menu.items { item.target = self }
            return menu
        }

        /// A right click acts on the row under it, so it selects that row
        /// first — unless it is already part of what is selected, the way a
        /// Finder click does.
        func menuNeedsUpdate(_ menu: NSMenu) {
            guard let outline = outline else { return }
            let row = outline.clickedRow
            if row >= 0, let node = outline.item(atRow: row) as? FileNode,
               model.selected[pane]?.contains(node.item.path) != true {
                outline.selectRowIndexes([row], byExtendingSelection: false)
                model.selected[pane] = [node.item.path]
            }
            let title = model.source(pane).isLocal ? L("Move to Trash") : L("Delete")
            menu.items.first { $0.action == #selector(deleteClicked) }?.title = title
            // Quick Look and the line under it are for this Mac's files only.
            if let index = menu.items.firstIndex(where: { $0.action == #selector(quickLook) }) {
                menu.items[index].isHidden = !preview.canPreview
                menu.items[index + 1].isHidden = !preview.canPreview
            }
        }

        /// Asked as the menu opens, after the row under the click has been
        /// picked: setting isEnabled by hand does not last, the menu enables
        /// every item whose action its target answers to.
        func validateMenuItem(_ item: NSMenuItem) -> Bool {
            switch item.action {
            case #selector(deleteClicked), #selector(quickLook): return !(model.selected[pane] ?? []).isEmpty
            default: return true
            }
        }

        @objc private func quickLook() { preview.show() }
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
            // A date reads from the left, a size lines up on the right.
            text.alignment = identifier == .size ? .right : .left
            text.lineBreakMode = .byTruncatingTail
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

    /// The longest of the Finder's ways of writing a date that fits the
    /// column: "Today at 10:31", then "24/09/2026, 10:31", then the day.
    static func date(_ date: Date, fitting width: CGFloat, font: NSFont?) -> String {
        var text = ""
        for form in Fmt.fileDates {
            text = form.string(from: date)
            if (text as NSString).size(withAttributes: [.font: font ?? NSFont.systemFont(ofSize: 11)]).width <= width {
                break
            }
        }
        return text
    }
}

extension NSUserInterfaceItemIdentifier {
    static let name = NSUserInterfaceItemIdentifier("name")
    static let modified = NSUserInterfaceItemIdentifier("modified")
    static let size = NSUserInterfaceItemIdentifier("size")
}
