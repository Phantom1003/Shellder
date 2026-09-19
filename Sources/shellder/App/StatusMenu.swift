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
        // The menu is not attached to the item: a left click toggles the
        // window, the menu is on the right button.
        item.button?.target = self
        item.button?.action = #selector(clicked)
        item.button?.sendAction(on: [.leftMouseUp, .rightMouseUp])
        model.$statuses.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        model.$enabled.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        model.$locked.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        model.$pendingPrompts.receive(on: DispatchQueue.main).sink { [weak self] _ in self?.updateIcon() }.store(in: &subs)
        updateIcon()
    }

    deinit {
        NSStatusBar.system.removeStatusItem(item)
    }

    /// The app's picture at menu-bar size (menubar.png / @2x in the bundle,
    /// produced by build.sh from scripts/icon-source).
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

    /// Left click opens the window (or puts it away when it is in front),
    /// right click shows the menu.
    @objc private func clicked() {
        if NSApp.currentEvent?.type == .rightMouseUp { showMenu() } else { delegate.toggleMainWindow() }
    }

    /// Pop the menu up under the item. Attaching it just for the click keeps
    /// the button highlighted and the menu placed the way a status menu is.
    private func showMenu() {
        item.menu = menu
        item.button?.performClick(nil)
        item.menu = nil
    }

    private var enabledStatuses: [HostStatus] {
        model.hosts.map { $0.alias }.filter { model.isKept($0) }.compactMap { model.statuses[$0] }
    }

    private func updateIcon() {
        let all = enabledStatuses
        let up = all.filter { $0.state.isUp }.count
        let degraded = !all.isEmpty && up < all.count
        if let pic = StatusMenu.picture {
            item.button?.image = model.pendingPrompts > 0 ? StatusMenu.badged(pic, .systemOrange) : pic
        }
        item.button?.appearsDisabled = degraded
        item.button?.toolTip = all.isEmpty ? L("Shellder — nothing switched on") : L("Shellder — \(up)/\(all.count) masters up")
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        menu.addItem(action(L("Open Shellder"), #selector(openWindow), nil))
        menu.addItem(.separator())
        for h in model.hosts {
            let st = model.statuses[h.alias]?.state ?? .off
            let dot: String
            switch st {
            case .up: dot = "●"
            case .foreign: dot = "◐"
            default: dot = "○"
            }
            // The host line only holds its submenu, it does nothing itself.
            let it = NSMenuItem(title: "\(dot)  \(h.alias)", action: nil, keyEquivalent: "")
            let sub = NSMenu()
            sub.autoenablesItems = false
            // The same controls as the host page's header, for the same state.
            let ctx = ToolContext(model: model, alias: h.alias)
            for tool in ToolRegistry.tools(ToolRegistry.inHeader) {
                for c in tool.controls(ctx) {
                    let run: () -> Void
                    switch c.kind {
                    case .action(let r): run = r
                    case .toggle(let on, let set): run = { set(!on) }
                    case .choice: continue
                    }
                    let mi = action(c.name, #selector(runControl(_:)), Control(run))
                    mi.isEnabled = c.enabled
                    mi.toolTip = c.help
                    sub.addItem(mi)
                }
            }
            sub.addItem(.separator())
            sub.addItem(action(L("Show in Shellder"), #selector(selectHost(_:)), h.alias))
            it.submenu = sub
            menu.addItem(it)
        }
        if model.hosts.isEmpty { menu.addItem(disabled(L("No Host entries found in ~/.ssh/config"))) }
        menu.addItem(.separator())
        menu.addItem(action(L("Connect All"), #selector(connectAll), nil))
        menu.addItem(action(L("Disconnect All"), #selector(disconnectAll), nil))
        menu.addItem(.separator())
        let settings = action(L("Settings"), #selector(openSettings), nil)
        // macOS 26+ puts a default gear in front of a Settings item. Setting
        // image to nil alone keeps that default; assigning a real image first
        // clears it (Apple engineer, developer.apple.com/forums/thread/800414).
        settings.image = NSImage()
        settings.image = nil
        menu.addItem(settings)
        menu.addItem(NSMenuItem(title: L("Quit Shellder"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
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

    /// A control's closure, carried by its menu item.
    private final class Control {
        let run: () -> Void
        init(_ run: @escaping () -> Void) { self.run = run }
    }

    private func host(_ sender: Any?) -> String? { (sender as? NSMenuItem)?.representedObject as? String }

    @objc private func openWindow() { delegate.showMainWindow() }
    @objc private func openSettings() { delegate.showSettings() }
    @objc private func selectHost(_ sender: Any?) { if let h = host(sender) { delegate.select(h) } }
    @objc private func runControl(_ sender: Any?) { ((sender as? NSMenuItem)?.representedObject as? Control)?.run() }
    @objc private func connectAll() { model.connectAll() }
    @objc private func disconnectAll() { model.disconnectAll() }
}
