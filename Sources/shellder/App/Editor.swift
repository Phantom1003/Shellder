import AppKit
import UniformTypeIdentifiers

/// Opens text files in the user's editor.
///
/// `NSWorkspace.open(url)` picks the application by the file's LaunchServices
/// type. `~/.ssh/config` has no extension, so no application claims it, and a
/// config with the execute bit set is even classified as a Unix executable and
/// run instead of edited. Open such files explicitly with the application the
/// user chose for plain text (TextEdit unless changed in Finder's Get Info).
enum Editor {
    static func open(_ path: String) {
        let file = URL(fileURLWithPath: path)
        let ws = NSWorkspace.shared
        if !FileManager.default.fileExists(atPath: path) {
            // Editors cannot open a missing file. An empty 0600 config is harmless.
            FileManager.default.createFile(atPath: path, contents: nil, attributes: [.posixPermissions: 0o600])
        }
        guard let app = ws.urlForApplication(toOpen: .plainText)
                ?? ws.urlForApplication(withBundleIdentifier: "com.apple.TextEdit") else {
            ws.open(file)
            return
        }
        ws.open([file], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration()) { _, error in
            if let error {
                Log.warn("could not open \(Config.abbreviateHome(path)) in \(app.lastPathComponent): \(error.localizedDescription)")
            }
        }
    }
}
