import AppKit
import Quartz

/// A row as Quick Look sees it. A new one whenever a listing says the file
/// changed, so the panel draws it again.
final class PreviewItem: NSObject, QLPreviewItem {
    let item: FileItem

    init(_ item: FileItem) { self.item = item }

    var previewItemURL: URL? { URL(fileURLWithPath: item.path) }
    var previewItemTitle: String? { item.name }
}

/// Quick Look for a pane that shows this Mac, the Finder's way: the space
/// bar opens the panel on what is selected and shuts it again, and the up
/// and down arrows walk the pane under it. A double click on a file and the
/// row menu open it too. The pane's outline view is the panel's controller
/// while the keyboard is in it. A host's files are not previewed.
final class FilePreviewController: NSObject, QLPreviewPanelDataSource, QLPreviewPanelDelegate {
    var model: FilesModel
    let pane: FilesModel.Pane
    weak var outline: NSOutlineView?
    /// Set while this pane is the one the panel shows.
    private var panel: QLPreviewPanel?
    private var items: [PreviewItem] = []

    init(model: FilesModel, pane: FilesModel.Pane) {
        self.model = model
        self.pane = pane
    }

    var canPreview: Bool { model.source(pane).isLocal }

    private var isShown: Bool { panel?.isVisible == true }

    func toggle() {
        if isShown { panel?.orderOut(nil) } else { show() }
    }

    func show() {
        guard canPreview, let outline = outline, !model.selection(pane).isEmpty else { return }
        // The panel takes its controller from the responder chain: the
        // keyboard has to be in this pane, not the other one.
        outline.window?.makeFirstResponder(outline)
        guard let shared = QLPreviewPanel.shared() else { return }
        if shared.isVisible { shared.updateController() } else { shared.makeKeyAndOrderFront(nil) }
    }

    // MARK: the panel's controller, handed over by the outline view

    func begin(_ panel: QLPreviewPanel) {
        self.panel = panel
        panel.dataSource = self
        panel.delegate = self
        items = []
        selectionChanged()
    }

    func end(_ panel: QLPreviewPanel) {
        self.panel = nil
        items = []
    }

    /// Show what is selected now. A pane switched to a host has nothing to
    /// show any more, and the panel goes.
    func selectionChanged() {
        guard let panel = panel else { return }
        guard canPreview else {
            panel.orderOut(nil)
            return
        }
        let picked = model.selection(pane).compactMap { model.node(pane, $0)?.item }
        guard picked != items.map(\.item) else { return }
        let kept = Dictionary(items.map { ($0.item, $0) }, uniquingKeysWith: { first, _ in first })
        items = picked.map { kept[$0] ?? PreviewItem($0) }
        panel.reloadData()
    }

    // MARK: QLPreviewPanelDataSource

    func numberOfPreviewItems(in panel: QLPreviewPanel!) -> Int { items.count }

    func previewPanel(_ panel: QLPreviewPanel!, previewItemAt index: Int) -> QLPreviewItem! {
        items.indices.contains(index) ? items[index] : nil
    }

    // MARK: QLPreviewPanelDelegate

    /// Up and down move through the pane, as in the Finder, and the panel
    /// follows the selection. The space bar puts the panel away.
    func previewPanel(_ panel: QLPreviewPanel!, handle event: NSEvent!) -> Bool {
        guard let event = event, event.type == .keyDown, let outline = outline else { return false }
        switch event.keyCode {
        case 125, 126:
            outline.keyDown(with: event)
            return true
        case 49:
            panel.orderOut(nil)
            return true
        default:
            return false
        }
    }

    /// The row's icon, where the panel zooms out of and back into.
    func previewPanel(_ panel: QLPreviewPanel!, sourceFrameOnScreenFor item: QLPreviewItem!) -> NSRect {
        guard let entry = item as? PreviewItem, let icon = icon(of: entry), let window = icon.window else { return .zero }
        return window.convertToScreen(icon.convert(icon.bounds, to: nil))
    }

    func previewPanel(_ panel: QLPreviewPanel!, transitionImageFor item: QLPreviewItem!,
                      contentRect: UnsafeMutablePointer<NSRect>!) -> Any! {
        guard let entry = item as? PreviewItem else { return nil }
        return FileIcons.local(entry.item.path)
    }

    /// The icon of the row an item came from, while that row is on screen.
    private func icon(of entry: PreviewItem) -> NSImageView? {
        guard let outline = outline, let node = model.node(pane, entry.item.path) else { return nil }
        let row = outline.row(forItem: node)
        let column = outline.column(withIdentifier: .name)
        guard row >= 0, column >= 0, outline.rows(in: outline.visibleRect).contains(row),
              let cell = outline.view(atColumn: column, row: row, makeIfNecessary: false) as? NSTableCellView
        else { return nil }
        return cell.imageView
    }
}
