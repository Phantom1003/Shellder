import AppKit
import Foundation

// 1. SSH_ASKPASS mode. ssh execs the program named in SSH_ASKPASS with the
//    prompt as its only argument, nothing else, so the mode comes from the
//    name we were called by: the shellder-askpass link in the bundle. There
//    is deliberately no subcommand for this: the helper is for ssh, not for
//    a terminal, and it answers nothing unless the app vouches for the ssh
//    that started it.
if (CommandLine.arguments[0] as NSString).lastPathComponent == Config.askpassName {
    let prompt = CommandLine.arguments.dropFirst().first ?? ""
    let rc = Askpass.run(prompt: prompt)
    fflush(stdout)
    exit(rc)
}

// 2. CLI mode: any argument is a subcommand, except Finder's legacy -psn_
//    token and our own flags: --background forces a start without a window,
//    --login marks a launch by the LaunchAgent (the only one that honours the
//    "start silently" setting).
var cliArgs = Array(CommandLine.arguments.dropFirst()).filter { !$0.hasPrefix("-psn_") }
let background = cliArgs.contains("--background")
let loginItem = cliArgs.contains(Launchd.loginFlag)
cliArgs.removeAll { $0 == "--background" || $0 == Launchd.loginFlag }
if !cliArgs.isEmpty {
    exit(CLI.run(cliArgs))
}

// 3. GUI app (Dock/menu bar + window).
// Overlay scrollers for this app whatever "Show scroll bars" says: legacy
// scrollers draw an opaque white track that no background colour reaches.
// AppKit reads this key from the app's own domain before the global one.
UserDefaults.standard.set("WhenScrolling", forKey: "AppleShowScrollBars")
// The interface language comes from Settings, English by default. The same
// key System Settings writes for a per-app language: AppKit, the open panel
// and our own strings all read it at launch, so a change needs a relaunch.
UserDefaults.standard.set([Prefs.language], forKey: "AppleLanguages")
Localization.launched = Prefs.language
guard SingleInstance.acquire() else {
    Log.warn("another shellder instance is running; exiting")
    fputs("shellder is already running\n", stderr)
    exit(0)
}

Log.startFresh()

let app = NSApplication.shared
let delegate = AppDelegate(background: background, loginItem: loginItem)
app.delegate = delegate
// Start as an accessory (no Dock tile); the delegate switches to a regular
// app whenever one of its windows is on screen.
app.setActivationPolicy(.accessory)
app.run()
