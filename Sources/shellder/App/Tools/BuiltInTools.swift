import AppKit

/// The master connection itself: the lock and whatever fits the state.
/// The Connect switch is in the host list, so it is not repeated here.
struct MasterTool: HostTool {
    let id = "master"
    let title = "Master"
    let icon = "network"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        let m = ctx.model, alias = ctx.alias
        var out: [ToolControl] = [
            .toggle("lock", ctx.locked ? "Unlock" : "Lock", icon: ctx.locked ? "lock.fill" : "lock.open", isOn: ctx.locked,
                    help: ctx.locked
                        ? "Locked: shellder reconnects this host after a drop and switches it on again when it starts. Switching it off, or a failed attempt, unlocks it."
                        : "Lock: switch the host on if it is off, then keep it connected. A drop is reconnected with back-off and the host comes back at the next launch. The lock goes with the switch: off, or a failed attempt, unlocks it.") {
                m.setLocked(alias, $0)
            },
        ]
        if ctx.state.isRunning {
            let behind = ctx.status?.neededBy ?? []
            out.append(.action("disconnect", "Disconnect", icon: "stop.fill",
                               help: behind.isEmpty
                                   ? "Disconnect: same as switching off. Closes the master and unlocks it."
                                   : "Disconnect: same as switching off. Closes the master, unlocks it, and switches off the hosts that jump through it (\(behind.joined(separator: ", ")))") {
                m.disconnect(alias)
            })
            out.append(.action("reconnect", "Reconnect", icon: "arrow.clockwise",
                               help: "Reconnect: close the master and connect again. The switch and the lock stay as they are.") {
                m.reconnect(alias)
            })
        } else if case .foreign = ctx.state {
            out.append(.action("takeover", "Take over", icon: "hand.raised.fill",
                               help: "Take over: close the external master and start one owned by shellder. Switches the host on.") {
                m.reconnect(alias)
            })
        } else if ctx.enabled {
            out.append(.action("retry", "Retry now", icon: "arrow.clockwise",
                               help: "Retry now: try again immediately instead of waiting for the back-off") {
                m.reconnect(alias)
            })
        } else {
            out.append(.action("connect", "Connect", icon: "play.fill",
                               help: "Connect: same as the switch, one attempt, kept up if it succeeds",
                               enabled: ctx.manageable) {
                m.connect(alias)
            })
        }
        if ctx.state.isUp {
            let socket = ctx.resolved?.controlPath.map(Config.abbreviateHome) ?? "the socket"
            out.append(.action("close", "Close socket", icon: "xmark.circle",
                               help: "Close socket: ssh -O exit, shuts the master down and removes \(socket). The switch goes off and the lock with it, and hosts jumping through this one go down as well.") {
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
    let title = "Keep-alive"
    let icon = "heart.text.square"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        let current = ctx.status?.idleMode ?? .none
        return [
            .choice("mode", "Keep-alive mode", icon: "heart.text.square",
                    options: SSH.IdleMode.allCases.map { .init(id: $0.rawValue, title: $0.title) },
                    selected: current.rawValue,
                    help: "Keep-alive mode: -N is cleanest. Servers that close session-less connections get an idle login shell, then cat. shellder escalates by itself and remembers the result.") { raw in
                if let mode = SSH.IdleMode(rawValue: raw) { ctx.model.setIdleMode(ctx.alias, mode) }
            },
        ]
    }

}

/// An interactive shell on the host, in the user's terminal.
struct ShellTool: HostTool {
    let id = "shell"
    let title = "Shell"
    let icon = "terminal"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        [
            .action("open", "Shell", icon: "terminal",
                    help: "Shell: run `\(Shell.command(ctx.alias))` in the application that handles ssh:// links (Terminal unless you chose another). Goes through the master, so no prompts.") {
                Shell.open(ctx.alias)
            },
        ]
    }
}

/// A one-off login with the stored credentials, bypassing the socket. Not
/// shown by default.
struct TestLoginTool: HostTool {
    let id = "testlogin"
    let title = "Test login"
    let icon = "checkmark.shield"

    func controls(_ ctx: ToolContext) -> [ToolControl] {
        let busy = ctx.model.testing.contains(ctx.alias)
        return [
            .action("test", "Test login", icon: "checkmark.shield",
                    help: "Test login: run a one-off `ssh \(ctx.alias) echo` with the stored credentials, bypassing the socket",
                    enabled: !busy) {
                ctx.model.testLogin(ctx.alias)
            },
        ]
    }

    func note(_ ctx: ToolContext) -> String? {
        ctx.model.testing.contains(ctx.alias) ? "Logging in…" : nil
    }
}
