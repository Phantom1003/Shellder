import Foundation

/// Paths and tunables shared by the app, the askpass helper and the CLI.
/// shellder never writes ssh configuration: the only files it owns are its
/// state directory, its log and (optionally) a LaunchAgent plist.
enum Config {
    static let app = "shellder"
    static let label = "local.shellder"

    static let home = FileManager.default.homeDirectoryForCurrentUser.path
    static let sshDir = home + "/.ssh"
    private static let env = ProcessInfo.processInfo.environment
    /// Test hooks: point shellder at another ssh config / state dir without
    /// touching the real ones (SHELLDER_SSH_CONFIG, SHELLDER_STATE_DIR, SHELLDER_PREFS_SUITE).
    static let sshConfigFile = env["SHELLDER_SSH_CONFIG"] ?? sshDir + "/config"
    static let configOverridden = env["SHELLDER_SSH_CONFIG"] != nil
    static let stateDir = env["SHELLDER_STATE_DIR"] ?? home + "/.local/state/shellder"
    static let prefsSuite = env["SHELLDER_PREFS_SUITE"] ?? label + ".prefs"
    static let socketFile = stateDir + "/askpass.sock"
    static let logFile = home + "/Library/Logs/shellder.log"
    static let plistFile = home + "/Library/LaunchAgents/" + label + ".plist"
    /// GitHub repository whose Releases the updater watches, and its latest
    /// release endpoint. SHELLDER_UPDATE_API points a test at a local copy
    /// of that JSON.
    static let releaseRepo = "Phantom1003/Shellder"
    static let updateAPI = URL(string: env["SHELLDER_UPDATE_API"] ?? "https://api.github.com/repos/\(releaseRepo)/releases/latest")!

    /// Path of this very executable. It doubles as the SSH_ASKPASS program.
    static let selfPath: String = {
        if let p = Bundle.main.executablePath { return p }
        return URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL.path
    }()

    // Reconnect back-off (seconds)
    static let backoffMin: TimeInterval = 5
    static let backoffMax: TimeInterval = 600
    /// A master that dies this soon after spawning counts as a failed attempt.
    static let shortLife: TimeInterval = 45
    /// After this many consecutive quick failures jump straight to backoffMax
    /// (a wrong password hammering the server gets you fail2ban'd).
    static let maxQuickFailures = 3
    static let pollInterval: TimeInterval = 2
    /// Re-verify an established master with `ssh -O check` this often.
    static let healthInterval: TimeInterval = 60
    /// Time a fresh master gets to authenticate (TOTP window waits, PAM delays,
    /// the user typing into a prompt).
    static let connectTimeout: TimeInterval = 300

    static func ensureDirs() {
        let fm = FileManager.default
        for d in [stateDir, (logFile as NSString).deletingLastPathComponent] {
            try? fm.createDirectory(atPath: d, withIntermediateDirectories: true,
                                    attributes: [.posixPermissions: 0o700])
        }
    }

    static func expandTilde(_ p: String) -> String {
        if p == "~" { return home }
        if p.hasPrefix("~/") { return home + String(p.dropFirst()) }
        return p
    }

    static func abbreviateHome(_ p: String) -> String {
        p.hasPrefix(home + "/") ? "~" + p.dropFirst(home.count) : p
    }
}
