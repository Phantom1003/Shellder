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

/// The dashed board on a host's page: one square icon per tool on it, no
/// text. A tool on the board is one thing to do, so the square runs the
/// tool's first action, and its help is the tooltip. Tools act through the
/// master, so the whole board is greyed out while the host is not up.
struct ToolBoard: View {
    let alias: String
    let tools: [any HostTool]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Tools").font(.headline).padding(.horizontal, 12)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: ToolSquare.side, maximum: ToolSquare.side), spacing: 6, alignment: .topLeading)],
                      alignment: .leading, spacing: 6) {
                ForEach(tools, id: \.id) { tool in
                    ToolSquare(tool: tool, alias: alias)
                }
            }
            .padding(6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Color.primary.opacity(0.2), style: StrokeStyle(lineWidth: 1, dash: [6, 4])))
        }
    }
}

/// One tool as a square icon button. Watches the model itself so the button
/// follows the host's state (a context passed in from outside would look
/// unchanged to SwiftUI).
struct ToolSquare: View {
    static let side: CGFloat = 40
    @EnvironmentObject var model: AppModel
    @Environment(\.colorScheme) private var scheme
    let tool: any HostTool
    let alias: String

    var body: some View {
        let ctx = ToolContext(model: model, alias: alias)
        let action = tool.controls(ctx).first { if case .action = $0.kind { return true } else { return false } }
        let enabled = ctx.state.isUp && (action?.enabled ?? false)
        Button {
            if case .action(let run)? = action?.kind { run() }
        } label: {
            Image(systemName: tool.icon)
                .font(.system(size: 17, weight: .medium))
                .frame(width: Self.side, height: Self.side)
                .background(RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(scheme == .dark ? Color.white.opacity(0.06) : Color(nsColor: .controlBackgroundColor)))
                .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08)))
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.4)
        .accessibilityLabel(tool.title)
        .help(ctx.state.isUp ? (action?.help ?? tool.title) : L("\(tool.title): needs the host connected"))
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
