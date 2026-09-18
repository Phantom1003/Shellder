import SwiftUI

/// A tool's controls alone, in a line: for the top of a card, where the
/// host's name already says what they act on.
struct ToolControls: View {
    @EnvironmentObject var model: AppModel
    let tool: any HostTool
    let alias: String

    var body: some View {
        let ctx = ToolContext(model: model, alias: alias)
        HStack(spacing: 6) {
            ForEach(tool.controls(ctx)) { ToolControlView(control: $0) }
        }
        .help(tool.title)
    }
}

/// A tool as one row of a card: its title on the left, its note and its
/// controls on the right, in the look of the card's other rows.
struct ToolRow: View {
    @EnvironmentObject var model: AppModel
    let tool: any HostTool
    let alias: String

    var body: some View {
        let ctx = ToolContext(model: model, alias: alias)
        HStack(alignment: .center, spacing: 16) {
            Text(tool.title).frame(width: 150, alignment: .leading)
            Spacer(minLength: 0)
            if let note = tool.note(ctx) {
                Text(note).foregroundColor(.secondary).lineLimit(1)
            }
            HStack(spacing: 6) {
                ForEach(tool.controls(ctx)) { ToolControlView(control: $0) }
            }
        }
    }
}

/// The dashed board on a host's page: one square tile per tool on it.
struct ToolBoard: View {
    let alias: String
    let tools: [any HostTool]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Tools").font(.headline).padding(.horizontal, 12)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: ToolTile.side, maximum: ToolTile.side), spacing: 10, alignment: .topLeading)],
                      alignment: .leading, spacing: 10) {
                ForEach(tools, id: \.id) { tool in
                    ToolTile(tool: tool, alias: alias)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Color.primary.opacity(0.2), style: StrokeStyle(lineWidth: 1, dash: [6, 4])))
        }
    }
}

/// One tool: title, its controls in a grid, an optional note. Square. The
/// tile watches the model itself so the controls follow the host's state
/// (a context passed in from outside would look unchanged to SwiftUI).
struct ToolTile: View {
    static let side: CGFloat = 116
    @EnvironmentObject var model: AppModel
    @Environment(\.colorScheme) private var scheme
    let tool: any HostTool
    let alias: String

    var body: some View {
        let ctx = ToolContext(model: model, alias: alias)
        VStack(alignment: .leading, spacing: 8) {
            Label(tool.title, systemImage: tool.icon)
                .font(.caption.weight(.semibold))
                .foregroundColor(.secondary)
                .lineLimit(1)
            let controls = tool.controls(ctx)
            let chips = controls.filter { if case .choice = $0.kind { return false } else { return true } }
            let choices = controls.filter { if case .choice = $0.kind { return true } else { return false } }
            if !chips.isEmpty {
                LazyVGrid(columns: Array(repeating: GridItem(.fixed(ToolButtonStyle.width), spacing: 6), count: 3),
                          alignment: .leading, spacing: 6) {
                    ForEach(chips) { ToolControlView(control: $0) }
                }
            }
            ForEach(choices) { ToolControlView(control: $0).controlSize(.mini) }
            if let note = tool.note(ctx) {
                Text(note).font(.caption2).foregroundColor(.secondary).lineLimit(2)
            }
            Spacer(minLength: 0)
        }
        .padding(10)
        .frame(width: Self.side, height: Self.side, alignment: .topLeading)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(scheme == .dark ? Color.white.opacity(0.06) : Color(nsColor: .controlBackgroundColor)))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
            .strokeBorder(Color.primary.opacity(0.08)))
    }
}

/// A control as it draws: an icon chip for an action or a toggle, a pop-up
/// for a choice. The name is the accessibility label and, unless the control
/// has its own help, the tooltip.
struct ToolControlView: View {
    let control: ToolControl

    var body: some View {
        Group {
            switch control.kind {
            case .action(let run):
                Button(action: run) { icon }
                    .buttonStyle(ToolButtonStyle())
            case .toggle(let on, let set):
                Button { set(!on) } label: { icon }
                    .buttonStyle(ToolButtonStyle(active: on))
            case .choice(let options, let selected, let set):
                Picker(control.name, selection: Binding(get: { selected }, set: set)) {
                    ForEach(options) { Text($0.title).tag($0.id) }
                }
                .pickerStyle(.menu)
                .labelsHidden()
                .fixedSize()
            }
        }
        .disabled(!control.enabled)
        .accessibilityLabel(control.name)
        .help(control.help ?? control.name)
    }

    private var icon: some View {
        Image(systemName: control.icon)
    }
}

/// The one look every icon button shares, on tools and elsewhere on the
/// page: a small rounded chip, lit while a toggle is on, darker while
/// pressed, dimmed while disabled.
struct ToolChip: ViewModifier {
    @Environment(\.isEnabled) private var isEnabled
    let active: Bool
    let pressed: Bool

    func body(content: Content) -> some View {
        content
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(active ? .accentColor : .primary)
            .frame(width: ToolButtonStyle.width, height: ToolButtonStyle.height)
            .background(RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(pressed ? Color.primary.opacity(0.16)
                      : active ? Color.accentColor.opacity(0.16)
                      : Color.primary.opacity(0.07)))
            .opacity(isEnabled ? 1 : 0.4)
            .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
    }
}

struct ToolButtonStyle: ButtonStyle {
    static let width: CGFloat = 28
    static let height: CGFloat = 24
    var active = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label.modifier(ToolChip(active: active, pressed: configuration.isPressed))
    }
}
