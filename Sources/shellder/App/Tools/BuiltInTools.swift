import AppKit

/// The master connection itself: the lock and whatever fits the state.
/// The Connect switch is in the host list, so it is not repeated here.
struct MasterTool: HostTool {
    let id = "master"
    let title = L("Master")
    let icon = "network"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        let m = ctx.model, alias = ctx.alias
        var out: [ToolControl] = [
            .toggle("lock", ctx.locked ? L("Unlock") : L("Lock"), icon: ctx.locked ? "lock.fill" : "lock.open", isOn: ctx.locked,
                    help: ctx.locked
                        ? L("Locked: shellder reconnects this host after a drop and switches it on again when it starts. Switching it off, or a failed attempt, unlocks it.")
                        : L("Lock: switch the host on if it is off, then keep it connected. A drop is reconnected with back-off and the host comes back at the next launch. The lock goes with the switch: off, or a failed attempt, unlocks it.")) {
                m.setLocked(alias, $0)
            },
        ]
        if ctx.state.isRunning {
            let behind = ctx.status?.neededBy ?? []
            out.append(.action("disconnect", L("Disconnect"), icon: "stop.fill",
                               help: behind.isEmpty
                                   ? L("Disconnect: same as switching off. Closes the master and unlocks it.")
                                   : L("Disconnect: same as switching off. Closes the master, unlocks it, and switches off the hosts that jump through it (\(behind.joined(separator: ", ")))")) {
                m.disconnect(alias)
            })
            out.append(.action("reconnect", L("Reconnect"), icon: "arrow.clockwise",
                               help: L("Reconnect: close the master and connect again. The switch and the lock stay as they are.")) {
                m.reconnect(alias)
            })
        } else if case .foreign = ctx.state {
            out.append(.action("takeover", L("Take over"), icon: "hand.raised.fill",
                               help: L("Take over: close the external master and start one owned by shellder. Switches the host on.")) {
                m.reconnect(alias)
            })
        } else if ctx.enabled {
            out.append(.action("retry", L("Retry now"), icon: "arrow.clockwise",
                               help: L("Retry now: try again immediately instead of waiting for the back-off")) {
                m.reconnect(alias)
            })
        } else {
            out.append(.action("connect", L("Connect"), icon: "play.fill",
                               help: L("Connect: same as the switch, one attempt, kept up if it succeeds"),
                               enabled: ctx.manageable) {
                m.connect(alias)
            })
        }
        if ctx.state.isUp {
            let socket = ctx.resolved?.controlPath.map(Config.abbreviateHome) ?? L("the socket")
            out.append(.action("close", L("Close socket"), icon: "xmark.circle",
                               help: L("Close socket: ssh -O exit, shuts the master down and removes \(socket). The switch goes off and the lock with it, and hosts jumping through this one go down as well.")) {
                m.closeSocket(alias)
            })
        }
        return out
    }
}

/// How the master stays connected on servers that drop session-less
/// connections: -N, an idle shell, or cat.
struct KeepAliveTool: HostTool {
    let id = "keepalive"
    let title = L("Keep-alive")
    let icon = "heart.text.square"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        let current = ctx.status?.idleMode ?? .none
        return [
            .choice("mode", L("Keep-alive mode"), icon: "heart.text.square",
                    options: SSH.IdleMode.allCases.map { .init(id: $0.rawValue, title: $0.localizedTitle) },
                    selected: current.rawValue,
                    help: L("Keep-alive mode: -N is cleanest. If the server closes a session-less connection right after login, pick an idle login shell or cat here. The choice is remembered per host.")) { raw in
                if let mode = SSH.IdleMode(rawValue: raw) { ctx.model.setIdleMode(ctx.alias, mode) }
            },
        ]
    }

}

/// An interactive shell on the host, in the user's terminal.
struct ShellTool: HostTool {
    let id = "shell"
    let title = L("Shell")
    let icon = "terminal"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        [
            .action("open", L("Shell"), icon: "terminal",
                    help: L("Shell: run `\(Shell.command(ctx.alias))` in the application that handles ssh:// links (Terminal unless you chose another). Goes through the master, so no prompts.")) {
                Shell.open(ctx.alias)
            },
        ]
    }
}

/// Files and folders picked in an open panel, copied into the host's home
/// directory with scp through the master.
struct CopyTool: HostTool {
    let id = "copy"
    let title = L("Copy files")
    let icon = "square.and.arrow.up"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        let busy = ctx.model.copying.contains(ctx.alias)
        return [
            .action("copy", L("Copy files"), icon: "square.and.arrow.up",
                    help: busy ? L("Copy files: an upload to \(ctx.alias) is still running")
                               : L("Copy files: pick files and folders, then scp them into the home directory on \(ctx.alias) through the master. Failures show in an alert, everything else in the log."),
                    enabled: !busy) {
                ctx.model.copyFiles(ctx.alias)
            },
        ]
    }
}
