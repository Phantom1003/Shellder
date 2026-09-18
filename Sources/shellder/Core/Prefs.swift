import Foundation

/// App preferences (which hosts are locked, UI options). Stored in the app's
/// own defaults domain, never in ssh configuration.
enum Prefs {
    static let defaults = UserDefaults(suiteName: Config.prefsSuite) ?? .standard

    private enum Key {
        static let locked = "autoConnectHosts"
        static let background = "keepInBackground"
        static let menuBar = "showMenuBarIcon"
        static let silent = "silentLaunch"
        static let logPanel = "showLogPanel"
        static let language = "language"
    }

    /// Language of the interface, a BCP 47 name with a translation in the
    /// bundle ("en", "zh-Hans"). English unless chosen otherwise, whatever
    /// the system language. Applied at launch, see main.swift.
    static var language: String {
        get { defaults.string(forKey: Key.language) ?? "en" }
        set { defaults.set(newValue, forKey: Key.language) }
    }

    /// Locked hosts: connected again at launch and reconnected after drops.
    /// The Connect switch itself is not remembered.
    static var lockedHosts: [String] {
        get { defaults.stringArray(forKey: Key.locked) ?? [] }
        set { defaults.set(newValue, forKey: Key.locked) }
    }

    static func setLocked(_ host: String, _ on: Bool) {
        var list = lockedHosts.filter { $0 != host }
        if on { list.append(host) }
        lockedHosts = list
    }

    /// How the master keeps its connection alive on servers that drop
    /// session-less (-N) connections; remembered per host across runs.
    /// Values: "none" (-N), "shell" (idle login shell on a pty), "cat".
    private static var idleModes: [String: String] {
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

    /// Closing the last window leaves the app running (masters kept up,
    /// reachable from the Dock or the menu bar); off means it quits instead.
    static var keepInBackground: Bool {
        get { bool(Key.background, default: true) }
        set { defaults.set(newValue, forKey: Key.background) }
    }
    /// Menu bar item; only meaningful while the app keeps running in the
    /// background, see keepInBackground.
    static var showMenuBarIcon: Bool {
        get { bool(Key.menuBar, default: true) }
        set { defaults.set(newValue, forKey: Key.menuBar) }
    }
    /// Start without a window (menu bar item only), whether launched by hand
    /// or by the LaunchAgent at login.
    static var silentLaunch: Bool {
        get { bool(Key.silent, default: false) }
        set { defaults.set(newValue, forKey: Key.silent) }
    }
    static var showLogPanel: Bool {
        get { bool(Key.logPanel, default: false) }
        set { defaults.set(newValue, forKey: Key.logPanel) }
    }
}
