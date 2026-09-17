import AppKit
import Combine
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()
    let background: Bool
    private var mainWindow: NSWindow?
    private var settingsWindow: NSWindow?
    private var promptWindow: NSWindow?
    private var statusMenu: StatusMenu?
    private var sigterm: DispatchSourceSignal?
    private var subs = Set<AnyCancellable>()

    init(background: Bool) {
        self.background = background
        super.init()
    }

    // MARK: lifecycle

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMainMenu()
        applyActivationPolicy()

        model.onNeedsAttention = { [weak self] in self?.showMainWindow() }
        // Prompts get their own floating window: it works before the main
        // window exists and while it is closed, unlike a SwiftUI sheet.
        model.$currentPrompt.receive(on: DispatchQueue.main)
            .sink { [weak self] p in self?.presentPrompt(p) }
            .store(in: &subs)
        model.onSettingsChanged = { [weak self] in
            self?.applyActivationPolicy()
            self?.applyMenuBarSetting()
        }
        model.start()
        applyMenuBarSetting()

        // Closing the last window sends the app to the background: it leaves
        // the Dock (accessory policy) and keeps running from the menu bar.
        NotificationCenter.default.publisher(for: NSWindow.willCloseNotification)
            .compactMap { $0.object as? NSWindow }
            .receive(on: DispatchQueue.main)
            .sink { [weak self] w in self?.windowWillClose(w) }
            .store(in: &subs)

        if !background && !Prefs.silentLaunch { showMainWindow() }

        // launchctl bootout / logout send SIGTERM: shut the masters down cleanly.
        signal(SIGTERM, SIG_IGN)
        let src = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        src.setEventHandler { NSApp.terminate(nil) }
        src.resume()
        sigterm = src
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showMainWindow()
        return false
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    func applicationWillTerminate(_ notification: Notification) {
        model.shutdown()
    }

    // MARK: policies

    /// Our own windows (the About panel and similar system panels do not count).
    private var ownWindows: [NSWindow] { [mainWindow, settingsWindow, promptWindow].compactMap { $0 } }

    private func anyWindowVisible(except closing: NSWindow? = nil) -> Bool {
        ownWindows.contains { $0 !== closing && $0.isVisible }
    }

    /// Dock icon only while a window is open (and the setting allows it);
    /// otherwise the app is an accessory: no Dock tile, menu bar item only.
    private func applyActivationPolicy(windowVisible: Bool? = nil) {
        let visible = windowVisible ?? anyWindowVisible()
        let wanted: NSApplication.ActivationPolicy = model.showDockIcon && visible ? .regular : .accessory
        guard NSApp.activationPolicy() != wanted else { return }
        NSApp.setActivationPolicy(wanted)
        if wanted == .regular && visible { NSApp.activate(ignoringOtherApps: true) }
    }

    private func windowWillClose(_ w: NSWindow) {
        guard ownWindows.contains(where: { $0 === w }) else { return }
        if !anyWindowVisible(except: w) {
            applyActivationPolicy(windowVisible: false)
            // Hand the focus to the next app instead of staying active with
            // no window and no Dock tile (the menu bar item keeps working).
            NSApp.hide(nil)
        }
    }

    /// Bring a window on screen: first restore the Dock tile (so the app can
    /// become active and own the main menu), then order the window front.
    private func present(_ w: NSWindow) {
        applyActivationPolicy(windowVisible: true)
        NSApp.unhide(nil)
        w.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func applyMenuBarSetting() {
        if model.showMenuBarIcon {
            if statusMenu == nil { statusMenu = StatusMenu(model: model, delegate: self) }
        } else {
            statusMenu = nil
        }
    }

    // MARK: windows

    @objc func showMainWindow() {
        if mainWindow == nil {
            let host = NSHostingController(rootView: MainView().environmentObject(model))
            let w = NSWindow(contentViewController: host)
            w.title = "Shellder"
            w.styleMask = [.titled, .closable, .miniaturizable, .resizable]
            w.isReleasedWhenClosed = false
            w.setContentSize(NSSize(width: 900, height: 560))
            w.minSize = NSSize(width: 720, height: 420)
            w.center()
            w.setFrameAutosaveName("shellder.main")
            w.tabbingMode = .disallowed
            mainWindow = w
        }
        if let w = mainWindow { present(w) }
    }

    @objc func showSettings() {
        if settingsWindow == nil {
            let host = NSHostingController(rootView: SettingsView().environmentObject(model))
            let w = NSWindow(contentViewController: host)
            w.title = "Shellder Settings"
            w.styleMask = [.titled, .closable]
            w.isReleasedWhenClosed = false
            w.setFrameAutosaveName("shellder.settings")
            w.center()
            settingsWindow = w
        }
        if let w = settingsWindow { present(w) }
    }

    private func presentPrompt(_ prompt: PromptRequest?) {
        guard prompt != nil else {
            if let w = promptWindow, w.isVisible {
                w.orderOut(nil)
                if !anyWindowVisible() { applyActivationPolicy(windowVisible: false) }
            }
            return
        }
        if promptWindow == nil {
            let host = NSHostingController(rootView: PromptHostView().environmentObject(model))
            let w = NSWindow(contentViewController: host)
            w.title = "Shellder — ssh is asking"
            w.styleMask = [.titled]
            w.level = .floating
            w.isReleasedWhenClosed = false
            w.collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary]
            promptWindow = w
        }
        guard let w = promptWindow else { return }
        w.layoutIfNeeded()
        w.center()
        present(w)
    }

    @objc func showLogPanel() { model.showLog.toggle() }

    @objc func reloadConfig() { model.reloadCatalog(force: true) }

    @objc func editConfig() {
        Editor.open(Config.sshConfigFile)
    }

    @objc func openLogFile() {
        _ = Log.appendHandle()
        NSWorkspace.shared.open(URL(fileURLWithPath: Config.logFile))
    }

    @objc func connectAll() { model.connectAll() }
    @objc func disconnectAll() { model.disconnectAll() }

    @objc func reconnectSelected() {
        if let h = model.selection { model.reconnect(h) }
    }

    func select(_ host: String) {
        model.selection = host
        showMainWindow()
    }

    // MARK: main menu

    private func buildMainMenu() {
        let main = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "About Shellder", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Settings…", action: #selector(showSettings), keyEquivalent: ",").target = self
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Hide Shellder", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        let hideOthers = appMenu.addItem(withTitle: "Hide Others", action: #selector(NSApplication.hideOtherApplications(_:)), keyEquivalent: "h")
        hideOthers.keyEquivalentModifierMask = [.command, .option]
        appMenu.addItem(withTitle: "Show All", action: #selector(NSApplication.unhideAllApplications(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit Shellder", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        let fileItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        fileMenu.addItem(withTitle: "Reload ssh config", action: #selector(reloadConfig), keyEquivalent: "r").target = self
        fileMenu.addItem(withTitle: "Edit ~/.ssh/config…", action: #selector(editConfig), keyEquivalent: "e").target = self
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Close Window", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redo = editMenu.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = editMenu
        main.addItem(editItem)

        let hostsItem = NSMenuItem()
        let hostsMenu = NSMenu(title: "Hosts")
        hostsMenu.addItem(withTitle: "Reconnect All", action: #selector(connectAll), keyEquivalent: "").target = self
        hostsMenu.addItem(withTitle: "Disconnect All", action: #selector(disconnectAll), keyEquivalent: "").target = self
        hostsMenu.addItem(.separator())
        hostsMenu.addItem(withTitle: "Reconnect Selected Host", action: #selector(reconnectSelected), keyEquivalent: "k").target = self
        hostsItem.submenu = hostsMenu
        main.addItem(hostsItem)

        let viewItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")
        viewMenu.addItem(withTitle: "Toggle Log Panel", action: #selector(showLogPanel), keyEquivalent: "l").target = self
        viewMenu.addItem(withTitle: "Open Log File", action: #selector(openLogFile), keyEquivalent: "").target = self
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Shellder", action: #selector(showMainWindow), keyEquivalent: "0").target = self
        windowMenu.addItem(.separator())
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        windowItem.submenu = windowMenu
        main.addItem(windowItem)
        NSApp.windowsMenu = windowMenu

        NSApp.mainMenu = main
    }
}

/// flock-based guard so launchd and a manual launch never run two daemons.
enum SingleInstance {
    private static var fd: Int32 = -1

    static func acquire() -> Bool {
        Config.ensureDirs()
        let path = Config.stateDir + "/app.lock"
        fd = open(path, O_CREAT | O_RDWR, 0o600)
        if fd < 0 { return true }   // cannot lock: do not block the user
        return flock(fd, LOCK_EX | LOCK_NB) == 0
    }
}
