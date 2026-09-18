import Foundation

/// "Start at login" via a per-user LaunchAgent. KeepAlive only restarts the
/// app after a crash (SuccessfulExit=false), so Quit from the menu sticks.
/// Whether the window opens at login follows the "start silently" setting,
/// like a manual launch, so the agent passes no flag.
enum Launchd {
    static var installed: Bool { FileManager.default.fileExists(atPath: Config.plistFile) }

    /// The executable path the installed agent points at, if any.
    static var installedPath: String? {
        guard let data = FileManager.default.contents(atPath: Config.plistFile),
              let plist = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any],
              let args = plist["ProgramArguments"] as? [String] else { return nil }
        return args.first
    }

    @discardableResult
    private static func launchctl(_ args: [String]) -> Int32 {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        p.arguments = args
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { return -1 }
        p.waitUntilExit()
        return p.terminationStatus
    }

    static var domain: String { "gui/\(getuid())" }

    /// Write the plist. `bootstrap` also starts the agent right away — only do
    /// that when no instance of the app is running.
    static func install(bootstrap: Bool) throws {
        let plist: [String: Any] = [
            "Label": Config.label,
            "ProgramArguments": [Config.selfPath],
            // Without this System Settings > Login Items lists the agent by
            // its executable name with a generic icon; with it the entry
            // carries the app's name and icon (the bundle id is the label).
            "AssociatedBundleIdentifiers": [Config.label],
            "RunAtLoad": true,
            "KeepAlive": ["SuccessfulExit": false],
            "ProcessType": "Interactive",
            "StandardOutPath": Config.logFile,
            "StandardErrorPath": Config.logFile,
            "EnvironmentVariables": [
                "PATH": "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin",
                "HOME": Config.home,
            ],
        ]
        let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
        try FileManager.default.createDirectory(atPath: (Config.plistFile as NSString).deletingLastPathComponent,
                                                withIntermediateDirectories: true)
        try data.write(to: URL(fileURLWithPath: Config.plistFile))
        Log.info("launch agent written to \(Config.plistFile) -> \(Config.selfPath)")
        if bootstrap {
            launchctl(["bootout", "\(domain)/\(Config.label)"])
            let rc = launchctl(["bootstrap", domain, Config.plistFile])
            if rc != 0 { throw SSHError("launchctl bootstrap failed (rc \(rc))") }
        }
    }

    /// Remove the plist. The currently loaded job (if any) is left alone so
    /// that disabling "start at login" does not kill the running app.
    static func uninstall(bootout: Bool) {
        if bootout { launchctl(["bootout", "\(domain)/\(Config.label)"]) }
        try? FileManager.default.removeItem(atPath: Config.plistFile)
        Log.info("launch agent removed")
    }
}
