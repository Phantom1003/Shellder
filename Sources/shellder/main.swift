import AppKit
import Foundation

// 1. SSH_ASKPASS mode: ssh spawned us with the prompt in argv[1].
if ProcessInfo.processInfo.environment["SHELLDER_ASKPASS"] == "1" {
    let prompt = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ""
    let rc = Askpass.run(prompt: prompt)
    fflush(stdout)
    exit(rc)
}

// 2. CLI mode: any argument (except Finder's legacy -psn_ token and our own
//    --background flag, which forces a silent start regardless of the
//    setting) is a subcommand.
var cliArgs = Array(CommandLine.arguments.dropFirst()).filter { !$0.hasPrefix("-psn_") }
let background = cliArgs.contains("--background")
cliArgs.removeAll { $0 == "--background" }
if !cliArgs.isEmpty {
    exit(CLI.run(cliArgs))
}

// 3. GUI app (Dock/menu bar + window).
// Overlay scrollers for this app whatever "Show scroll bars" says: legacy
// scrollers draw an opaque white track that no background colour reaches.
// AppKit reads this key from the app's own domain before the global one.
UserDefaults.standard.set("WhenScrolling", forKey: "AppleShowScrollBars")
guard SingleInstance.acquire() else {
    Log.warn("another shellder instance is running; exiting")
    fputs("shellder is already running\n", stderr)
    exit(0)
}

let app = NSApplication.shared
let delegate = AppDelegate(background: background)
app.delegate = delegate
// Start as an accessory (no Dock tile); the delegate switches to a regular
// app whenever one of its windows is on screen.
app.setActivationPolicy(.accessory)
app.run()
