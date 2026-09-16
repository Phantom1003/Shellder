import Foundation

/// App preferences (which hosts to keep connected, UI options). Stored in the
/// app's own defaults domain, never in ssh configuration.
enum Prefs {
    static let defaults = UserDefaults(suiteName: Config.prefsSuite) ?? .standard

    private enum Key {
        static let enabled = "autoConnectHosts"
        static let dock = "showDockIcon"
        static let menuBar = "showMenuBarIcon"
        static let openWindow = "openWindowAtLaunch"
        static let logPanel = "showLogPanel"
    }

    static var enabledHosts: [String] {
        get { defaults.stringArray(forKey: Key.enabled) ?? [] }
        set { defaults.set(newValue, forKey: Key.enabled) }
    }

    static func setEnabled(_ host: String, _ on: Bool) {
        var list = enabledHosts.filter { $0 != host }
        if on { list.append(host) }
        enabledHosts = list
    }

    /// How the master keeps its connection alive on servers that drop
    /// session-less (-N) connections; remembered per host across runs.
    /// Values: "none" (-N), "shell" (idle login shell on a pty), "cat".
    static var idleModes: [String: String] {
        get { defaults.dictionary(forKey: "idleModes") as? [String: String] ?? [:] }
        set { defaults.set(newValue, forKey: "idleModes") }
    }

    static func idleMode(_ host: String) -> String? { idleModes[host] }

    static func setIdleMode(_ host: String, _ mode: String?) {
        var m = idleModes
        m[host] = mode
        idleModes = m
    }

    /// Code identity (teamid:… or cdhash:…) that last wrote the vault item.
    static var vaultOwner: String? {
        get { defaults.string(forKey: "vaultOwner") }
        set { defaults.set(newValue, forKey: "vaultOwner") }
    }

    private static func bool(_ key: String, default d: Bool) -> Bool {
        defaults.object(forKey: key) == nil ? d : defaults.bool(forKey: key)
    }

    static var showDockIcon: Bool {
        get { bool(Key.dock, default: true) }
        set { defaults.set(newValue, forKey: Key.dock) }
    }
    static var showMenuBarIcon: Bool {
        get { bool(Key.menuBar, default: true) }
        set { defaults.set(newValue, forKey: Key.menuBar) }
    }
    static var openWindowAtLaunch: Bool {
        get { bool(Key.openWindow, default: true) }
        set { defaults.set(newValue, forKey: Key.openWindow) }
    }
    static var showLogPanel: Bool {
        get { bool(Key.logPanel, default: false) }
        set { defaults.set(newValue, forKey: Key.logPanel) }
    }
}
