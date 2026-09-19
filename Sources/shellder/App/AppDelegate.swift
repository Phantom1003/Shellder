import AppKit
import Combine
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()
    let background: Bool
    let loginItem: Bool
    private var mainWindow: NSWindow?
    private var settingsWindow: NSWindow?
    private var promptWindow: NSWindow?
    private var statusMenu: StatusMenu?
    private var sigterm: DispatchSourceSignal?
    private var subs = Set<AnyCancellable>()

    init(background: Bool, loginItem: Bool) {
        self.background = background
        self.loginItem = loginItem
        super.init()
    }

    // MARK: lifecycle

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMainMenu()

        model.onNeedsAttention = { [weak self] in self?.showMainWindow() }
        // Prompts get their own floating window: it works before the main
        // window exists and while it is closed, unlike a SwiftUI sheet.
        model.$currentPrompt.receive(on: DispatchQueue.main)
            .sink { [weak self] p in self?.presentPrompt(p) }
            .store(in: &subs)
        model.onSettingsChanged = { [weak self] in self?.applyMenuBarSetting() }
        model.start()
        applyMenuBarSetting()

        // Closing the last window sends the app to the background: it leaves
        // the Dock and lives in the menu bar (if the icon is on), or it quits
        // when Settings say so.
        NotificationCenter.default.publisher(for: NSWindow.willCloseNotification)
            .compactMap { $0.object as? NSWindow }
            .receive(on: DispatchQueue.main)
            .sink { [weak self] w in self?.windowWillClose(w) }
            .store(in: &subs)

        // "Start silently" applies to the login item only: a double click
        // always opens the window. It also needs "Run in background", as an
        // app that quits with its window must show one.
        if !background && !(loginItem && Prefs.silentLaunch && Prefs.keepInBackground) { showMainWindow() }

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

    /// Dock tile only while one of our windows is open; otherwise the app is
    /// an accessory: no Dock tile, menu bar item only. Not a setting.
    private func applyActivationPolicy(windowVisible: Bool) {
        let wanted: NSApplication.ActivationPolicy = windowVisible ? .regular : .accessory
        guard NSApp.activationPolicy() != wanted else { return }
        NSApp.setActivationPolicy(wanted)
    }

    private func windowWillClose(_ w: NSWindow) {
        guard ownWindows.contains(where: { $0 === w }) else { return }
        guard !anyWindowVisible(except: w) else { return }
        if model.keepInBackground {
            applyActivationPolicy(windowVisible: false)
            // Hand the focus to the next app instead of staying active with
            // no window and no Dock tile (the menu bar item keeps working).
            NSApp.hide(nil)
        } else {
            NSApp.terminate(nil)
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
        if model.menuBarIconShown {
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
            // No title bar: the traffic lights sit on the sidebar and the
            // content runs up to the top edge.
            w.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
            w.titleVisibility = .hidden
            w.titlebarAppearsTransparent = true
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

    /// The menu bar icon: bring the window up, or put it away when it is
    /// already in front. Away means what closing it means (background or
    /// quit, per Settings), except that a click never quits: it hides.
    @objc func toggleMainWindow() {
        if let w = mainWindow, w.isVisible, NSApp.isActive, !NSApp.isHidden {
            if model.keepInBackground { w.close() } else { NSApp.hide(nil) }
        } else {
            showMainWindow()
        }
    }

    @objc func showSettings() {
        if settingsWindow == nil {
            let host = NSHostingController(rootView: SettingsView().environmentObject(model))
            // Only the minimum size comes from SwiftUI: the user sets the
            // height and the grouped form scrolls inside it.
            host.sizingOptions = [.minSize]
            let w = NSWindow(contentViewController: host)
            w.title = L("Shellder Settings")
            w.styleMask = [.titled, .closable, .resizable]
            w.isReleasedWhenClosed = false
            w.setContentSize(NSSize(width: 520, height: 640))
            w.minSize = NSSize(width: 480, height: 320)
            w.center()
            w.setFrameAutosaveName("shellder.settings")
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
            w.title = L("Shellder — ssh is asking")
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
        appMenu.addItem(withTitle: L("About Shellder"), action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: L("Settings…"), action: #selector(showSettings), keyEquivalent: ",").target = self
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: L("Hide Shellder"), action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        let hideOthers = appMenu.addItem(withTitle: L("Hide Others"), action: #selector(NSApplication.hideOtherApplications(_:)), keyEquivalent: "h")
        hideOthers.keyEquivalentModifierMask = [.command, .option]
        appMenu.addItem(withTitle: L("Show All"), action: #selector(NSApplication.unhideAllApplications(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: L("Quit Shellder"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        let fileItem = NSMenuItem()
        let fileMenu = NSMenu(title: L("File"))
        fileMenu.addItem(withTitle: L("Reload ssh config"), action: #selector(reloadConfig), keyEquivalent: "r").target = self
        fileMenu.addItem(withTitle: L("Edit ~/.ssh/config…"), action: #selector(editConfig), keyEquivalent: "e").target = self
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: L("Close Window"), action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: L("Edit"))
        editMenu.addItem(withTitle: L("Undo"), action: Selector(("undo:")), keyEquivalent: "z")
        let redo = editMenu.addItem(withTitle: L("Redo"), action: Selector(("redo:")), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: L("Cut"), action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: L("Copy"), action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: L("Paste"), action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: L("Select All"), action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = editMenu
        main.addItem(editItem)

        let hostsItem = NSMenuItem()
        let hostsMenu = NSMenu(title: L("Hosts"))
        hostsMenu.addItem(withTitle: L("Connect All"), action: #selector(connectAll), keyEquivalent: "").target = self
        hostsMenu.addItem(withTitle: L("Disconnect All"), action: #selector(disconnectAll), keyEquivalent: "").target = self
        hostsMenu.addItem(.separator())
        hostsMenu.addItem(withTitle: L("Reconnect Selected Host"), action: #selector(reconnectSelected), keyEquivalent: "k").target = self
        hostsItem.submenu = hostsMenu
        main.addItem(hostsItem)

        let viewItem = NSMenuItem()
        let viewMenu = NSMenu(title: L("View"))
        viewMenu.addItem(withTitle: L("Toggle Log Panel"), action: #selector(showLogPanel), keyEquivalent: "l").target = self
        viewMenu.addItem(withTitle: L("Open Log File"), action: #selector(openLogFile), keyEquivalent: "").target = self
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: L("Window"))
        windowMenu.addItem(withTitle: L("Shellder"), action: #selector(showMainWindow), keyEquivalent: "0").target = self
        windowMenu.addItem(.separator())
        windowMenu.addItem(withTitle: L("Minimize"), action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: L("Zoom"), action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
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
