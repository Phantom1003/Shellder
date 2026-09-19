import SwiftUI

struct MainView: View {
    @EnvironmentObject var model: AppModel
    @AppStorage("logPanelHeight", store: Prefs.defaults) private var logHeight = 200.0

    var body: some View {
        VStack(spacing: 0) {
            NavigationSplitView {
                SidebarView()
            } detail: {
                if let h = model.selection, let entry = model.hosts.first(where: { $0.alias == h }) {
                    HostDetailView(entry: entry)
                        .id(entry.alias)
                } else {
                    EmptyDetailView()
                }
            }
            if model.showLog {
                ResizeHandle(height: $logHeight, range: 80...700)
                LogView()
                    .frame(height: logHeight)
            }
        }
        .frame(minWidth: 720, minHeight: 420)
        .alert(item: $model.actionResult) { r in
            Alert(title: Text(r.title), message: Text(r.output), dismissButton: .default(Text("OK")))
        }
    }
}

/// Horizontal divider that drags the panel below it taller or shorter.
struct ResizeHandle: View {
    @Binding var height: Double
    let range: ClosedRange<Double>
    @State private var startHeight: Double?

    var body: some View {
        Rectangle()
            .fill(Color.clear)
            .frame(height: 7)
            .overlay(Divider(), alignment: .center)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside { NSCursor.resizeUpDown.push() } else { NSCursor.pop() }
            }
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { v in
                        if startHeight == nil { startHeight = height }
                        let base = startHeight ?? height
                        height = min(range.upperBound, max(range.lowerBound, base - v.translation.height))
                    }
                    .onEnded { _ in startHeight = nil }
            )
            .help("Drag to resize the log panel")
    }
}

/// Password field with an eye button to show what was typed.
/// While it has focus the keyboard is kept on the ASCII layout so an input
/// method cannot swallow keys, see `ASCIIInputSource`.
struct RevealableSecretField: View {
    let placeholder: String
    @Binding var text: String
    var monospaced = false
    @State private var visible = false
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 6) {
            Group {
                if visible {
                    TextField(placeholder, text: $text)
                } else {
                    SecureField(placeholder, text: $text)
                }
            }
            .textFieldStyle(.roundedBorder)
            .font(monospaced ? .system(.body, design: .monospaced) : .body)
            .autocorrectionDisabled(true)
            .focused($focused)
            Button {
                visible.toggle()
                DispatchQueue.main.async { focused = true }
            } label: {
                Image(systemName: visible ? "eye.slash" : "eye")
            }
            .buttonStyle(.borderless)
            .help(visible ? L("Hide") : L("Show what you typed"))
        }
        .onAppear { DispatchQueue.main.async { focused = true } }
        .onChange(of: focused) { on in
            if on { ASCIIInputSource.enter() } else { ASCIIInputSource.leave() }
        }
        .onDisappear { ASCIIInputSource.leave() }
    }
}

struct EmptyDetailView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        VStack(spacing: 10) {
            emblem.foregroundColor(.secondary)
            if model.hosts.isEmpty {
                Text("No Host entries in ~/.ssh/config").font(.title3)
            } else {
                Text("Select a host").font(.title3).foregroundColor(.secondary)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// The menu-bar picture (menubar-large.png, produced by build.sh) at 44 pt.
    @ViewBuilder private var emblem: some View {
        if let pic = Bundle.main.image(forResource: "menubar-large") {
            Image(nsImage: pic).renderingMode(.template).resizable().frame(width: 44, height: 44)
        } else {
            Image(systemName: "network").font(.system(size: 44))
        }
    }
}

// MARK: - sidebar

struct SidebarView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        List(selection: $model.selection) {
            ForEach(model.hosts) { h in
                HostRow(entry: h).tag(h.alias)
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .background(OverlayScrollers())
        .background(QuietSelection())
        .navigationSplitViewColumnWidth(min: 240, ideal: 280, max: 420)
        .safeAreaInset(edge: .bottom, spacing: 0) { bottomBar }
    }

    /// The three buttons that lead somewhere (the config, twice, and the
    /// settings) on the left, the log panel's toggle alone on the right. The
    /// reload button shows the spinner itself while a reload runs, so nothing
    /// else moves.
    private var bottomBar: some View {
        HStack(spacing: 2) {
            Button { model.reloadCatalog(force: true) } label: {
                if model.reloading {
                    ProgressView().controlSize(.small).frame(width: 22, height: 20)
                } else {
                    Image(systemName: "arrow.clockwise").frame(width: 22, height: 20)
                }
            }
            .disabled(model.reloading)
            .accessibilityLabel("Reload ssh config").help("Reload ~/.ssh/config (⌘R)")
            Button { Editor.open(Config.sshConfigFile) } label: { Image(systemName: "doc.text").frame(width: 22, height: 20) }
                .accessibilityLabel("Edit ssh config").help("Edit ~/.ssh/config in your editor (⌘E)")
            Button { (NSApp.delegate as? AppDelegate)?.showSettings() } label: { Image(systemName: "gearshape").frame(width: 22, height: 20) }
                .accessibilityLabel("Settings").help("Settings (⌘,)")
            if let r = model.updater.available {
                Button { (NSApp.delegate as? AppDelegate)?.showSettings() } label: {
                    Image(systemName: "arrow.down.circle.fill").foregroundStyle(.tint).frame(width: 22, height: 20)
                }
                .accessibilityLabel("Update available").help("Version \(r.version) is available")
            }
            Spacer(minLength: 0)
            Button { model.showLog.toggle() } label: { Image(systemName: model.showLog ? "terminal.fill" : "terminal").frame(width: 22, height: 20) }
                .accessibilityLabel("Log panel").help("Toggle the log panel (⌘L)")
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(.bar)
        .overlay(Divider(), alignment: .top)
    }
}

/// Turns off the table view's own selection highlight in the window, so the
/// sidebar can draw a light one under the selected row instead of the solid
/// accent fill, which hides the Connect switch's colour. Applied like
/// `OverlayScrollers`, as a background of the List.
struct QuietSelection: NSViewRepresentable {
    func makeNSView(context: Context) -> Probe { Probe() }
    func updateNSView(_ view: Probe, context: Context) { view.apply() }

    final class Probe: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            apply()
        }

        func apply() {
            guard let root = window?.contentView else { return }
            Self.sweep(root)
            DispatchQueue.main.async { [weak self] in
                if let root = self?.window?.contentView { Self.sweep(root) }
            }
        }

        private static func sweep(_ view: NSView) {
            if let table = view as? NSTableView, table.selectionHighlightStyle != .none {
                table.selectionHighlightStyle = .none
            }
            for sub in view.subviews { sweep(sub) }
        }
    }
}

struct HostRow: View {
    @EnvironmentObject var model: AppModel
    let entry: HostEntry

    private var state: HostState { model.statuses[entry.alias]?.state ?? .off }

    var body: some View {
        HStack(spacing: 8) {
            StatusDot(state: state, failed: model.statuses[entry.alias]?.failed ?? false)
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.alias).fontWeight(.medium)
                Text(subtitle).font(.caption).foregroundColor(.secondary).lineLimit(1)
            }
            Spacer(minLength: 4)
            ConnectSwitch(alias: entry.alias, label: "")
                .controlSize(.mini)
                .labelsHidden()
        }
        .padding(.vertical, 2)
        .listRowBackground(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(Color.accentColor.opacity(model.selection == entry.alias ? 0.16 : 0))
                .padding(.horizontal, 6))
    }

    /// Where the host is. The state is the dot, the details are on the page.
    private var subtitle: String {
        if let r = model.resolved[entry.alias] {
            return r.user.isEmpty ? r.hostname : "\(r.user)@\(r.hostname)"
        }
        if let e = model.resolveErrors[entry.alias] { return L("config error: \(e)") }
        return L("resolving…")
    }
}

/// The Connect switch: one attempt, kept up while it lasts, off again when
/// it fails. Green once the master is up.
struct ConnectSwitch: View {
    @EnvironmentObject var model: AppModel
    let alias: String
    let label: String

    var body: some View {
        let up = model.statuses[alias]?.state.isUp ?? false
        Toggle(label, isOn: Binding(get: { model.isEnabled(alias) },
                                    set: { model.setEnabled(alias, $0) }))
            .toggleStyle(.switch)
            .tint(up ? .green : nil)
            .help("Connect: one attempt. If it succeeds the master stays up until it drops or you switch it off. If it fails the switch goes back off. Jump hosts from this list are switched on with it, and switching a jump host off takes the hosts behind it down.")
    }
}

struct StatusDot: View {
    let state: HostState
    var failed = false
    var body: some View {
        Circle().fill(color).frame(width: 9, height: 9)
            .overlay(Circle().strokeBorder(Color.primary.opacity(0.15)))
            .help(failed ? L("Failed") : state.label)
    }
    var color: Color {
        if failed { return .red }
        switch state {
        case .up: return .green
        case .foreign: return .teal
        case .connecting, .waitingForJump: return .yellow
        case .waiting: return .orange
        case .error: return .red
        case .off: return Color.secondary.opacity(0.25)
        }
    }
}

// MARK: - detail

struct HostDetailView: View {
    @EnvironmentObject var model: AppModel
    let entry: HostEntry
    @State private var confirmRemoveAll = false

    private var alias: String { entry.alias }
    private var status: HostStatus? { model.statuses[alias] }
    private var state: HostState { status?.state ?? .off }
    private var resolved: ResolvedHost? { model.resolved[alias] }
    private var enabled: Bool { model.isEnabled(alias) }
    private var summary: String { status?.summary ?? state.label }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                header
                if resolved != nil && resolved?.controlPath == nil { noControlPath }
                if let err = model.resolveErrors[alias] {
                    DetailCard("ssh -G \(alias) failed") {
                        Label(err, systemImage: "exclamationmark.triangle.fill").foregroundColor(.red)
                    }
                }
                if !ToolRegistry.onBoard.isEmpty {
                    ToolBoard(alias: alias, tools: ToolRegistry.tools(ToolRegistry.onBoard))
                }
                credentialsSection
                connectionSection
            }
            .frame(maxWidth: 720)
            .padding(20)
            .frame(maxWidth: .infinity)
        }
        .scrollContentBackground(.hidden)
        .background(OverlayScrollers())
        .background(Color(nsColor: .windowBackgroundColor))
        .onDisappear { model.hideAllRevealed() }
        .sheet(item: $model.secretEdit) { e in SecretEditorSheet(edit: e) }
        .confirmationDialog("Remove every stored credential for \(alias)?", isPresented: $confirmRemoveAll) {
            Button("Remove", role: .destructive) {
                for k in SecretKind.allCases { model.removeSecret(alias, k) }
            }
        }
    }

    // The first card: what the connection is, how it is doing, and the tools
    // that belong right next to that (the master itself, the keep-alive mode).
    private var header: some View {
        DetailCard {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(alias).font(.title2.weight(.semibold))
                    HStack(spacing: 6) {
                        StatusDot(state: state, failed: status?.failed ?? false)
                        if let since = status?.since {
                            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                                Text("\(summary) · \(Fmt.duration(ctx.date.timeIntervalSince(since)))")
                            }
                        } else if status?.failed == true {
                            Text("\(summary) (details in log)")
                        } else {
                            Text(summary)
                        }
                    }
                    .lineLimit(1)
                    .font(.callout).foregroundColor(.secondary)
                    if let r = resolved {
                        Text(targetLine(r)).font(.callout).foregroundColor(.secondary)
                            .textSelection(.enabled)
                    }
                    if !entry.aliases.isEmpty {
                        Text("also: \(entry.aliases.joined(separator: ", "))").font(.caption).foregroundColor(.secondary)
                    }
                }
                Spacer(minLength: 0)
                ForEach(ToolRegistry.tools(ToolRegistry.inHeader), id: \.id) { tool in
                    ToolControls(tool: tool, alias: alias)
                }
            }
            .padding(.vertical, 4)
            ForEach(ToolRegistry.tools(ToolRegistry.inCard), id: \.id) { tool in
                ToolRow(tool: tool, alias: alias)
            }
            // While off the reason is in the status line, while retrying it
            // is worth a row of its own.
            if status?.failed != true, let e = status?.lastError { row("Last error", e) }
        }
    }

    private func targetLine(_ r: ResolvedHost) -> String {
        var s = r.user.isEmpty ? r.hostname : "\(r.user)@\(r.hostname)"
        if r.port != "22" { s += ":\(r.port)" }
        if let pj = r.proxyJump { s = L("\(s)  via \(pj)") }
        return s
    }

    private var noControlPath: some View {
        DetailCard {
            VStack(alignment: .leading, spacing: 8) {
                Label("No ControlPath for this host", systemImage: "exclamationmark.triangle.fill")
                    .foregroundColor(.orange).font(.headline)
                Text("A ControlMaster needs a socket path. shellder never edits ~/.ssh/config, so add something like this yourself (a `Host *` block covers every host):")
                    .foregroundColor(.secondary)
                HStack(alignment: .top) {
                    Text(snippet).font(.system(.body, design: .monospaced)).textSelection(.enabled)
                        .padding(8).background(Color.secondary.opacity(0.1)).cornerRadius(6)
                    Button("Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(snippet, forType: .string)
                    }
                    Button("Edit config…") { Editor.open(Config.sshConfigFile) }
                }
            }
        }
    }

    private var snippet: String {
        "Host \(alias)\n    ControlMaster auto\n    ControlPath ~/.ssh/cm-%C\n    ControlPersist 5m"
    }

    /// Only the kinds that have a secret get a row. The header adds the
    /// missing ones on demand ("+") and empties the whole card (trash).
    private var credentialsSection: some View {
        let stored = SecretKind.allCases.filter { model.hasSecret(alias, $0) }
        let missing = SecretKind.allCases.filter { !model.hasSecret(alias, $0) }
        return DetailCard {
            if stored.isEmpty {
                Text("No credentials stored. Press + to add a password, key passphrase or TOTP secret.")
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(stored, id: \.self) { kind in
                credentialRow(kind)
            }
        } header: {
            HStack(spacing: 6) {
                Text("Credentials")
                Spacer()
                Menu {
                    ForEach(missing, id: \.self) { kind in
                        Button(kind.localizedTitle) { model.secretEdit = SecretEdit(host: alias, kind: kind) }
                    }
                } label: {
                    Image(systemName: "plus")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .modifier(ToolChip(active: false, pressed: false))
                .disabled(missing.isEmpty)
                .accessibilityLabel(L("Add credential"))
                .help(missing.isEmpty ? L("Every kind of credential is stored") : L("Add a credential"))
                Button(role: .destructive) { confirmRemoveAll = true } label: {
                    Image(systemName: "trash")
                }
                .buttonStyle(ToolButtonStyle())
                .disabled(stored.isEmpty)
                .accessibilityLabel(L("Remove all credentials"))
                .help(L("Remove every stored credential of this host from the keychain"))
            }
        }
    }

    @ViewBuilder
    private func credentialRow(_ kind: SecretKind) -> some View {
        let stored = model.hasSecret(alias, kind)
        let shown = model.revealed[model.revealKey(alias, kind)]
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(kind.localizedTitle)
                Spacer(minLength: 12)
                HStack(spacing: 6) {
                    if kind == .totp && stored {
                        TOTPCodeView(host: alias)
                    }
                    Image(systemName: stored ? "checkmark.circle.fill" : "circle.dashed")
                        .foregroundColor(stored ? .green : .secondary)
                    Button {
                        model.setRevealed(alias, kind, shown == nil)
                    } label: {
                        Image(systemName: shown == nil ? "eye" : "eye.slash")
                    }
                    .buttonStyle(ToolButtonStyle(active: shown != nil))
                    .disabled(!stored)
                    .accessibilityLabel(shown == nil ? L("Show") : L("Hide"))
                    .help(shown == nil ? L("Show the stored \(kind.noun)") : L("Hide"))
                    Button { model.secretEdit = SecretEdit(host: alias, kind: kind) } label: {
                        Image(systemName: "pencil")
                    }
                    .buttonStyle(ToolButtonStyle())
                    .accessibilityLabel(stored ? L("Change \(kind.noun)") : L("Set \(kind.noun)"))
                    .help(stored ? L("Change the stored \(kind.noun)") : L("Set a \(kind.noun)"))
                    Button(role: .destructive) { model.removeSecret(alias, kind) } label: {
                        Image(systemName: "trash")
                    }
                    .buttonStyle(ToolButtonStyle())
                    .disabled(!stored)
                    .accessibilityLabel(L("Remove \(kind.noun)"))
                    .help(L("Remove the stored \(kind.noun) from the keychain"))
                }
                .layoutPriority(1)
            }
            if let v = shown {
                HStack(spacing: 8) {
                    Text(v)
                        .font(.system(.body, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(.horizontal, 8).padding(.vertical, 4)
                        .background(Color.secondary.opacity(0.1)).cornerRadius(5)
                    Button("Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(v, forType: .string)
                    }
                    .controlSize(.small)
                    Text("\(v.count) characters").font(.caption).foregroundColor(.secondary)
                }
            }
        }
    }

    private var connectionSection: some View {
        DetailCard {
            if let r = resolved {
                row("HostName", r.hostname)
                row("User", r.user)
                row("Port", r.port)
                if let pj = r.proxyJump {
                    row("ProxyJump", pj + (r.jumpAlias != nil ? L("  (a host from this list: its master is brought up first and kept alive with this one)") : ""))
                }
                if let pc = r.proxyCommand { row("ProxyCommand", pc, mono: true) }
                row("IdentityFile", r.identityFiles.map(Config.abbreviateHome).joined(separator: "\n"), mono: true)
                row("ControlMaster", r.controlMaster)
                row("ControlPath", r.controlPath.map(Config.abbreviateHome) ?? L("none"), mono: true)
                row("ControlPersist", r.controlPersist)
                row("ServerAliveInterval", r.serverAliveInterval == 0 ? L("0 (shellder adds 15s for its master)") : "\(r.serverAliveInterval)")
            } else if model.resolveErrors[alias] == nil {
                Text("resolving…").foregroundColor(.secondary)
            }
            row("Defined in", Config.abbreviateHome(entry.source), mono: true)
        } header: {
            HStack {
                Text("Effective configuration")
                Spacer()
                Button("Edit ~/.ssh/config…") { Editor.open(Config.sshConfigFile) }
                    .buttonStyle(.link).font(.caption)
            }
        }
    }

    /// One key/value line of the detail page. The value sits on the right and
    /// wraps onto further lines when it is long (a ProxyCommand, several
    /// identity files, an error).
    private func row(_ label: LocalizedStringKey, _ value: String, mono: Bool = false) -> some View {
        HStack(alignment: .top, spacing: 16) {
            Text(label)
                .frame(width: 150, alignment: .leading)
            Text(value)
                .font(mono ? .system(.body, design: .monospaced) : .body)
                .foregroundColor(.secondary)
                .textSelection(.enabled)
                .multilineTextAlignment(.trailing)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
    }
}

/// A section of the detail page drawn as a rounded card: the grouped-Form
/// look done by hand. SwiftUI's grouped Form on macOS 26 lays its rows out
/// from estimated heights, so a host with wrapping or extra lines of text
/// got cramped credential rows and a different gap above the first card
/// than its neighbours. A VStack gives every row its real size.
struct DetailCard<Header: View, Content: View>: View {
    @Environment(\.colorScheme) private var scheme
    private let header: Header
    private let content: Content

    init(@ViewBuilder content: () -> Content, @ViewBuilder header: () -> Header) {
        self.content = content()
        self.header = header()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            header.font(.headline).padding(.horizontal, 12)
            rows
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(scheme == .dark ? Color.white.opacity(0.06) : Color(nsColor: .controlBackgroundColor)))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08)))
        }
    }

    /// The rows, one under the other with a divider between neighbours.
    @ViewBuilder private var rows: some View {
        if #available(macOS 15, *) {
            VStack(spacing: 0) {
                Group(subviews: content) { subviews in
                    ForEach(subviews.indices, id: \.self) { i in
                        if i > 0 { Divider().padding(.leading, 12) }
                        subviews[i].modifier(CardRow())
                    }
                }
            }
        } else {
            VStack(spacing: 0) { Group { content }.modifier(CardRow()) }
        }
    }
}

extension DetailCard where Header == EmptyView {
    init(@ViewBuilder content: () -> Content) {
        self.init(content: content, header: { EmptyView() })
    }
}

extension DetailCard where Header == Text {
    init(_ title: LocalizedStringKey, @ViewBuilder content: () -> Content) {
        self.init(content: content, header: { Text(title) })
    }
}

private struct CardRow: ViewModifier {
    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct TOTPCodeView: View {
    @EnvironmentObject var model: AppModel
    let host: String
    @State private var shown = false

    var body: some View {
        if shown, let t = model.totpCache[host] {
            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                HStack(spacing: 6) {
                    Text(t.code(at: ctx.date.timeIntervalSince1970))
                        .font(.system(.body, design: .monospaced).weight(.semibold))
                        .textSelection(.enabled)
                    Text("\(t.period - Int(ctx.date.timeIntervalSince1970) % t.period)s")
                        .font(.caption).foregroundColor(.secondary).frame(width: 28, alignment: .leading)
                }
            }
        } else {
            Button("Show code") { shown = model.loadTOTP(host) }
                .help("Compare with your authenticator app")
        }
    }
}

// MARK: - secret editor

struct SecretEditorSheet: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let edit: SecretEdit
    @State private var value = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("\(edit.kind.localizedTitle) for \(edit.host)").font(.headline)
            if edit.kind == .totp {
                TextField("base32 secret or otpauth://totp/… URI", text: $value)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))
            } else {
                RevealableSecretField(placeholder: edit.kind == .password ? L("password") : L("passphrase"), text: $value)
            }
            if let e = error { Text(e).font(.caption).foregroundColor(.red) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Save") { save() }.keyboardShortcut(.defaultAction).disabled(value.isEmpty)
            }
        }
        .padding(20)
        .frame(width: 440)
    }

    private func save() {
        do {
            try model.setSecret(edit.host, edit.kind, value)
            dismiss()
        } catch {
            self.error = "\(error)"
        }
    }
}
