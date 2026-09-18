import SwiftUI

/// A tool is a small plug-in of a host's page: a title, an icon and a
/// handful of controls that act on that host. Tools are stateless, the
/// page asks them for their controls on every render with a fresh
/// `ToolContext`, and every control draws as the same kind of icon chip. On
/// the Tools board a tool is one square icon that runs its first action.
///
/// To add a tool: conform to `HostTool`, register it in `ToolRegistry`.
protocol HostTool {
    /// Stable identifier, used for ordering and (later) the user's pick.
    var id: String { get }
    var title: String { get }
    /// SF Symbol shown next to the title.
    var icon: String { get }
    /// The controls to show for this host right now.
    func controls(_ ctx: ToolContext) -> [ToolControl]
    /// One short line next to the controls in a card row, or nil.
    func note(_ ctx: ToolContext) -> String?
}

extension HostTool {
    func note(_ ctx: ToolContext) -> String? { nil }
}

/// What a tool sees of the host it acts on, plus the model to act through.
/// Built by the page on every render, never kept by a tool.
struct ToolContext {
    let model: AppModel
    let alias: String

    var status: HostStatus? { model.statuses[alias] }
    var state: HostState { status?.state ?? .off }
    var resolved: ResolvedHost? { model.resolved[alias] }
    var enabled: Bool { model.isEnabled(alias) }
    var locked: Bool { model.isLocked(alias) }
    /// A master can be started at all (resolved, with a ControlPath).
    var manageable: Bool { resolved?.controlPath != nil }
}

/// One control of a tool. An action and a toggle draw as icon buttons (the
/// toggle lights up while on), a choice draws as a pop-up menu showing the
/// selected option.
struct ToolControl: Identifiable {
    enum Kind {
        case action(() -> Void)
        case toggle(isOn: Bool, set: (Bool) -> Void)
        case choice(options: [Option], selected: String, set: (String) -> Void)
    }

    struct Option: Identifiable {
        let id: String
        let title: String
    }

    let id: String
    /// Accessibility label, and the tooltip when `help` is nil.
    let name: String
    /// SF Symbol (a choice shows its selected option instead).
    let icon: String
    let kind: Kind
    var help: String? = nil
    var enabled = true

    static func action(_ id: String, _ name: String, icon: String, help: String? = nil,
                       enabled: Bool = true, _ run: @escaping () -> Void) -> ToolControl {
        ToolControl(id: id, name: name, icon: icon, kind: .action(run), help: help, enabled: enabled)
    }

    static func toggle(_ id: String, _ name: String, icon: String, isOn: Bool, help: String? = nil,
                       enabled: Bool = true, _ set: @escaping (Bool) -> Void) -> ToolControl {
        ToolControl(id: id, name: name, icon: icon, kind: .toggle(isOn: isOn, set: set), help: help, enabled: enabled)
    }

    static func choice(_ id: String, _ name: String, icon: String, options: [Option], selected: String,
                       help: String? = nil, enabled: Bool = true, _ set: @escaping (String) -> Void) -> ToolControl {
        ToolControl(id: id, name: name, icon: icon, kind: .choice(options: options, selected: selected, set: set),
                    help: help, enabled: enabled)
    }
}

/// The tools that exist and where they show: next to the host's name at the
/// top of its first card (controls only, no title), as rows of that card, or
/// as square icons on the dashed board below it. Today the lists are fixed,
/// the registry is where a per-user pick plugs in.
enum ToolRegistry {
    static let all: [any HostTool] = [MasterTool(), KeepAliveTool(), ShellTool(), TestLoginTool()]
    static let inHeader = ["master"]
    static let inCard = ["keepalive"]
    static let onBoard = ["shell"]

    static func tools(_ ids: [String]) -> [any HostTool] {
        ids.compactMap { id in all.first { $0.id == id } }
    }
}
