import AppKit
import Combine

/// Menu bar item: connection overview and quick actions.
final class StatusMenu: NSObject, NSMenuDelegate {
    private let model: AppModel
    private unowned let delegate: AppDelegate
    private let item: NSStatusItem
    private let menu = NSMenu()
    private var subs = Set<AnyCancellable>()

    init(model: AppModel, delegate: AppDelegate) {
        self.model = model
        self.delegate = delegate
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        item.button?.image = StatusMenu.picture
        item.button?.toolTip = "Shellder"
        menu.delegate = self
        menu.autoenablesItems = false
        item.menu = menu
        model.$statuses.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        model.$enabled.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        model.$pendingPrompts.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        updateIcon()
    }

    deinit {
        NSStatusBar.system.removeStatusItem(item)
    }

    /// The app's picture at menu-bar size (menubar.png / @2x in the bundle,
    /// produced by build.sh from Assets/icon-source).
    private static let picture: NSImage? = {
        guard let img = Bundle.main.image(forResource: "menubar") else { return nil }
        img.size = NSSize(width: 22, height: 22)
        img.isTemplate = true      // alpha-only mask, tinted by the menu bar
        return img
    }()

    /// The picture with a small coloured dot in the corner.
    private static func badged(_ base: NSImage, _ color: NSColor) -> NSImage {
        let out = NSImage(size: base.size, flipped: false) { rect in
            base.draw(in: rect)
            color.setFill()
            NSBezierPath(ovalIn: NSRect(x: rect.maxX - 7, y: rect.minY, width: 7, height: 7)).fill()
            return true
        }
        out.isTemplate = true
        return out
    }

    private var enabledStatuses: [HostStatus] {
        model.hosts.map { $0.alias }.filter { model.isEnabled($0) }.compactMap { model.statuses[$0] }
    }

    private func updateIcon() {
        let all = enabledStatuses
        let up = all.filter { $0.state.isUp }.count
        let degraded = !all.isEmpty && up < all.count
        if let pic = StatusMenu.picture {
            item.button?.image = model.pendingPrompts > 0 ? StatusMenu.badged(pic, .systemOrange) : pic
        }
        item.button?.appearsDisabled = degraded
        item.button?.toolTip = all.isEmpty ? "Shellder — no hosts kept connected" : "Shellder — \(up)/\(all.count) masters up"
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let all = enabledStatuses
        let up = all.filter { $0.state.isUp }.count
        let header = all.isEmpty ? "Shellder — no hosts kept connected" : "Shellder — \(up)/\(all.count) connected"
        menu.addItem(disabled(header))
        if model.pendingPrompts > 0 {
            let it = NSMenuItem(title: "⚠︎ \(model.pendingPrompts) prompt(s) waiting for you…", action: #selector(openWindow), keyEquivalent: "")
            it.target = self
            menu.addItem(it)
        }
        menu.addItem(.separator())
        for h in model.hosts {
            let st = model.statuses[h.alias]?.state ?? .off
            let dot: String
            switch st {
            case .up, .foreign: dot = "●"
            case .connecting, .waitingForJump: dot = "◐"
            case .off, .waiting: dot = "○"
            case .error: dot = "✕"
            }
            let it = NSMenuItem(title: "\(dot)  \(h.alias) — \(st.label)", action: #selector(selectHost(_:)), keyEquivalent: "")
            it.target = self
            it.representedObject = h.alias
            let sub = NSMenu()
            sub.autoenablesItems = false
            let keep = NSMenuItem(title: "Connected", action: #selector(toggleKeep(_:)), keyEquivalent: "")
            keep.target = self; keep.representedObject = h.alias
            keep.state = model.isEnabled(h.alias) ? .on : .off
            sub.addItem(keep)
            sub.addItem(action("Reconnect", #selector(reconnectHost(_:)), h.alias))
            sub.addItem(action("Disconnect", #selector(disconnectHost(_:)), h.alias))
            if st.isUp { sub.addItem(action("Close socket (ssh -O exit)", #selector(closeSocket(_:)), h.alias)) }
            sub.addItem(.separator())
            sub.addItem(action("Show in shellder…", #selector(selectHost(_:)), h.alias))
            it.submenu = sub
            menu.addItem(it)
        }
        if model.hosts.isEmpty { menu.addItem(disabled("No Host entries found in ~/.ssh/config")) }
        menu.addItem(.separator())
        let open = NSMenuItem(title: "Open shellder", action: #selector(openWindow), keyEquivalent: "")
        open.target = self
        menu.addItem(open)
        menu.addItem(action("Connect All Enabled", #selector(connectAll), nil))
        menu.addItem(action("Disconnect All", #selector(disconnectAll), nil))
        menu.addItem(.separator())
        let settings = NSMenuItem(title: "Settings…", action: #selector(openSettings), keyEquivalent: "")
        settings.target = self
        menu.addItem(settings)
        let quit = NSMenuItem(title: "Quit Shellder", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        menu.addItem(quit)
    }

    private func disabled(_ title: String) -> NSMenuItem {
        let it = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        it.isEnabled = false
        return it
    }

    private func action(_ title: String, _ sel: Selector, _ obj: Any?) -> NSMenuItem {
        let it = NSMenuItem(title: title, action: sel, keyEquivalent: "")
        it.target = self
        it.representedObject = obj
        return it
    }

    private func host(_ sender: Any?) -> String? { (sender as? NSMenuItem)?.representedObject as? String }

    @objc private func openWindow() { delegate.showMainWindow() }
    @objc private func openSettings() { delegate.showSettings() }
    @objc private func selectHost(_ sender: Any?) { if let h = host(sender) { delegate.select(h) } }
    @objc private func toggleKeep(_ sender: Any?) { if let h = host(sender) { model.setEnabled(h, !model.isEnabled(h)) } }
    @objc private func connectHost(_ sender: Any?) { if let h = host(sender) { model.connect(h) } }
    @objc private func reconnectHost(_ sender: Any?) { if let h = host(sender) { model.reconnect(h) } }
    @objc private func disconnectHost(_ sender: Any?) { if let h = host(sender) { model.disconnect(h) } }
    @objc private func closeSocket(_ sender: Any?) { if let h = host(sender) { model.closeSocket(h) } }
    @objc private func connectAll() { model.connectAll() }
    @objc private func disconnectAll() { model.disconnectAll() }
}
