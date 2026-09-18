import AppKit

/// Opens an interactive `ssh <alias>` in the user's terminal.
///
/// Done through an `ssh://` URL: macOS hands it to whichever application
/// claims that scheme (Terminal unless the user picked iTerm2 or another),
/// which runs `ssh <alias>`. The alias goes through the same ~/.ssh/config
/// as the master, so the session reuses shellder's socket when one is up.
enum Shell {
    static func open(_ alias: String) {
        var parts = URLComponents()
        parts.scheme = "ssh"
        parts.host = alias
        guard let url = parts.url else {
            Log.warn("\(alias): cannot form an ssh:// URL from this alias")
            return
        }
        NSWorkspace.shared.open(url, configuration: NSWorkspace.OpenConfiguration()) { _, error in
            if let error {
                Log.warn("\(alias): could not open a shell: \(error.localizedDescription)")
            } else {
                Log.info("\(alias): shell opened in the ssh:// handler")
            }
        }
    }

    static func command(_ alias: String) -> String { "ssh \(alias)" }
}
