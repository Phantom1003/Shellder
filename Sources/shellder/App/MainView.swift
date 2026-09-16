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
        .alert(item: $model.testResult) { r in
            Alert(title: Text(r.ok ? "\(r.host): login OK" : "\(r.host): login failed"),
                  message: Text(r.output.isEmpty ? "ssh returned no output." : r.output),
                  dismissButton: .default(Text("OK")))
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
struct RevealableSecretField: View {
    let placeholder: String
    @Binding var text: String
    var monospaced = false
    @State private var visible = false

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
            Button {
                visible.toggle()
            } label: {
                Image(systemName: visible ? "eye.slash" : "eye")
            }
            .buttonStyle(.borderless)
            .help(visible ? "Hide" : "Show what you typed")
        }
    }
}

struct EmptyDetailView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "network").font(.system(size: 44)).foregroundColor(.secondary)
            if model.hosts.isEmpty {
                Text("No Host entries in ~/.ssh/config").font(.title3)
                Text("Shellder lists every `Host` block from your ssh configuration. Add hosts there and they appear here — shellder never edits that file.")
                    .foregroundColor(.secondary).multilineTextAlignment(.center).frame(maxWidth: 420)
            } else {
                Text("Select a host").font(.title3).foregroundColor(.secondary)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - sidebar

struct SidebarView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        List(selection: $model.selection) {
            Section {
                ForEach(model.hosts) { h in
                    HostRow(entry: h).tag(h.alias)
                }
            } header: {
                HStack {
                    Text("Hosts")
                    Spacer()
                    Text(summary).foregroundColor(.secondary)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationSplitViewColumnWidth(min: 240, ideal: 280, max: 420)
        .safeAreaInset(edge: .bottom, spacing: 0) { bottomBar }
    }

    private var summary: String {
        let enabled = model.hosts.filter { model.isEnabled($0.alias) }
        let up = enabled.filter { model.statuses[$0.alias]?.state.isUp == true }.count
        return enabled.isEmpty ? "\(model.hosts.count)" : "\(up)/\(enabled.count) up"
    }

    private var bottomBar: some View {
        HStack(spacing: 4) {
            Button { model.reloadCatalog(force: true) } label: { Image(systemName: "arrow.clockwise") }
                .accessibilityLabel("Reload ssh config").help("Reload ~/.ssh/config")
            Button { NSWorkspace.shared.open(URL(fileURLWithPath: Config.sshConfigFile)) } label: { Image(systemName: "doc.text") }
                .accessibilityLabel("Edit ssh config").help("Edit ~/.ssh/config in your editor")
            Spacer()
            if model.reloading { ProgressView().controlSize(.small) }
            Button { model.showLog.toggle() } label: { Image(systemName: model.showLog ? "terminal.fill" : "terminal") }
                .accessibilityLabel("Log panel").help("Toggle the log panel (⌘L)")
            Button { (NSApp.delegate as? AppDelegate)?.showSettings() } label: { Image(systemName: "gearshape") }
                .accessibilityLabel("Settings").help("Settings (⌘,)")
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.bar)
        .overlay(Divider(), alignment: .top)
    }
}

struct HostRow: View {
    @EnvironmentObject var model: AppModel
    let entry: HostEntry

    private var state: HostState { model.statuses[entry.alias]?.state ?? .off }

    var body: some View {
        HStack(spacing: 8) {
            StatusDot(state: state)
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.alias).fontWeight(.medium)
                Text(subtitle).font(.caption).foregroundColor(.secondary).lineLimit(1)
            }
            Spacer(minLength: 4)
            Toggle("", isOn: Binding(get: { model.isEnabled(entry.alias) },
                                     set: { model.setEnabled(entry.alias, $0) }))
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
                .help("Keep a ControlMaster connection to \(entry.alias) open")
        }
        .padding(.vertical, 2)
    }

    private var subtitle: String {
        if let r = model.resolved[entry.alias] {
            let target = r.user.isEmpty ? r.hostname : "\(r.user)@\(r.hostname)"
            if state == .off, let e = model.statuses[entry.alias]?.lastError {
                return "\(target) · failed: \(e)"
            }
            return "\(target) · \(state.label)"
        }
        if let e = model.resolveErrors[entry.alias] { return "config error: \(e)" }
        return "resolving…"
    }
}

struct StatusDot: View {
    let state: HostState
    var body: some View {
        Circle().fill(color).frame(width: 9, height: 9)
            .overlay(Circle().strokeBorder(Color.primary.opacity(0.15)))
            .help(state.label)
    }
    var color: Color {
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

    var body: some View {
        Form {
            header
            if resolved != nil && resolved?.controlPath == nil { noControlPath }
            if let err = model.resolveErrors[alias] {
                Section {
                    Label(err, systemImage: "exclamationmark.triangle.fill").foregroundColor(.red)
                } header: { Text("ssh -G \(alias) failed") }
            }
            statusSection
            credentialsSection
            connectionSection
        }
        .formStyle(.grouped)
        .onDisappear { model.hideAllRevealed() }
        .sheet(item: $model.secretEdit) { e in SecretEditorSheet(edit: e) }
        .confirmationDialog("Remove every stored credential for \(alias)?", isPresented: $confirmRemoveAll) {
            Button("Remove password, passphrase and TOTP secret", role: .destructive) {
                for k in SecretKind.allCases { model.removeSecret(alias, k) }
            }
        }
    }

    // header + actions
    private var header: some View {
        Section {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(alias).font(.title2.weight(.semibold))
                    if let r = resolved {
                        Text(targetLine(r)).font(.callout).foregroundColor(.secondary)
                            .textSelection(.enabled)
                    }
                    HStack(spacing: 6) {
                        StatusDot(state: state)
                        Text(state.label).font(.callout).foregroundColor(.secondary)
                    }
                    if !entry.aliases.isEmpty {
                        Text("also: " + entry.aliases.joined(separator: ", ")).font(.caption).foregroundColor(.secondary)
                    }
                }
                Spacer()
                Toggle("Connect", isOn: Binding(get: { enabled }, set: { model.setEnabled(alias, $0) }))
                    .toggleStyle(.switch)
                    .help("On: connect once, then keep the master alive and reconnect after drops. Off: close it. A failed first attempt turns the switch back off.")
            }
            .padding(.vertical, 4)
            HStack(spacing: 8) {
                if state.isRunning {
                    Button("Disconnect") { model.disconnect(alias) }
                    Button("Reconnect") { model.reconnect(alias) }
                } else if case .foreign = state {
                    Button("Take over") { model.reconnect(alias) }
                        .help("Close the external master and start one owned by shellder")
                } else if enabled {
                    Button("Retry now") { model.reconnect(alias) }
                        .help("Try again immediately instead of waiting for the back-off")
                } else {
                    Button("Connect") { model.connect(alias) }
                        .disabled(resolved == nil || resolved?.controlPath == nil)
                        .help("Same as the switch: one attempt, kept alive if it succeeds")
                }
                Button {
                    model.testLogin(alias)
                } label: {
                    if model.testing.contains(alias) { ProgressView().controlSize(.small) } else { Text("Test login…") }
                }
                .disabled(model.testing.contains(alias))
                .help("Run a one-off `ssh \(alias) echo` with the stored credentials, bypassing the socket")
                if state.isUp {
                    Button("Close socket") { model.closeSocket(alias) }
                        .help("ssh -O exit: shut the master down and remove \(resolved?.controlPath.map(Config.abbreviateHome) ?? "the socket"). A kept host stays paused until you press Connect.")
                }
                Spacer()
                if let e = status?.lastError {
                    Label(e, systemImage: "info.circle").font(.caption).foregroundColor(.orange).lineLimit(1)
                        .help(e)
                }
            }
        }
    }

    private func targetLine(_ r: ResolvedHost) -> String {
        var s = r.user.isEmpty ? r.hostname : "\(r.user)@\(r.hostname)"
        if r.port != "22" { s += ":\(r.port)" }
        if let pj = r.proxyJump { s += "  via \(pj)" }
        return s
    }

    private var noControlPath: some View {
        Section {
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
                    Button("Edit config…") { NSWorkspace.shared.open(URL(fileURLWithPath: Config.sshConfigFile)) }
                }
            }
        }
    }

    private var snippet: String {
        "Host \(alias)\n    ControlMaster auto\n    ControlPath ~/.ssh/cm-%C\n    ControlPersist 5m"
    }

    private var statusSection: some View {
        Section("Status") {
            row("State", state.label)
            if case .up(let pid) = state { row("Master PID", "\(pid)") }
            if let since = status?.since {
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    row(state.isUp ? "Up for" : "Connecting for", Fmt.duration(ctx.date.timeIntervalSince(since)))
                }
            }
            if let cp = resolved?.controlPath {
                row("Socket", Config.abbreviateHome(cp), mono: true)
            }
            if let q = status?.quickFailures, q > 0 {
                row("Consecutive failures", "\(q)")
            }
            Picker("Keep-alive mode", selection: Binding(
                get: { status?.idleMode ?? .none },
                set: { model.setIdleMode(alias, $0) })) {
                ForEach(SSH.IdleMode.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .help("How the master stays connected. -N is cleanest; servers that close session-less connections get an idle login shell (looks like an open terminal in `w`), and cat as a last resort. shellder escalates automatically and remembers the result.")
            if let e = status?.lastError { row("Last error", e) }
            if state == .off, status?.lastError != nil {
                Text("The switch was turned off because the attempt failed. Fix the cause (credentials, network, host key) and switch it on again.")
                    .font(.caption).foregroundColor(.secondary)
            }
        }
    }

    private var credentialsSection: some View {
        Section {
            ForEach(SecretKind.allCases, id: \.self) { kind in
                credentialRow(kind)
            }
            HStack {
                Text("Secrets live in your login keychain as “shellder \(alias)” items. When a prompt comes that no secret answers, shellder asks you in a dialog and can save the answer here.")
                    .font(.caption).foregroundColor(.secondary)
                Spacer()
                Button("Remove all…", role: .destructive) { confirmRemoveAll = true }
                    .disabled(model.secretPresence[alias]?.isEmpty ?? true)
            }
        } header: {
            Text("Credentials")
        }
    }

    @ViewBuilder
    private func credentialRow(_ kind: SecretKind) -> some View {
        let stored = model.hasSecret(alias, kind)
        let shown = model.revealed[model.revealKey(alias, kind)]
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(kind.title)
                    Text(kindHint(kind)).font(.caption).foregroundColor(.secondary)
                }
                Spacer()
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
                .disabled(!stored)
                .help(shown == nil ? "Show the stored \(kind.title.lowercased())" : "Hide")
                Button(stored ? "Change…" : "Set…") { model.secretEdit = SecretEdit(host: alias, kind: kind) }
                Button("Remove") { model.removeSecret(alias, kind) }.disabled(!stored)
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

    private func kindHint(_ kind: SecretKind) -> String {
        switch kind {
        case .password: return "answers “Password:” prompts (password or keyboard-interactive auth)"
        case .passphrase: return "unlocks the private key when it is not in ssh-agent"
        case .totp: return "generates verification codes from a base32 secret / otpauth:// URI"
        }
    }

    private var connectionSection: some View {
        Section {
            if let r = resolved {
                row("HostName", r.hostname)
                row("User", r.user)
                row("Port", r.port)
                if let pj = r.proxyJump {
                    row("ProxyJump", pj + (r.jumpAlias != nil ? "  (managed host)" : ""))
                }
                if let pc = r.proxyCommand { row("ProxyCommand", pc, mono: true) }
                row("IdentityFile", r.identityFiles.map(Config.abbreviateHome).joined(separator: "\n"), mono: true)
                row("ControlMaster", r.controlMaster)
                row("ControlPath", r.controlPath.map(Config.abbreviateHome) ?? "none", mono: true)
                row("ControlPersist", r.controlPersist)
                row("ServerAliveInterval", r.serverAliveInterval == 0 ? "0 (shellder adds 15s for its master)" : "\(r.serverAliveInterval)")
            } else if model.resolveErrors[alias] == nil {
                Text("resolving…").foregroundColor(.secondary)
            }
            row("Defined in", Config.abbreviateHome(entry.source), mono: true)
        } header: {
            HStack {
                Text("Effective configuration (ssh -G \(alias))")
                Spacer()
                Button("Edit ~/.ssh/config…") { NSWorkspace.shared.open(URL(fileURLWithPath: Config.sshConfigFile)) }
                    .buttonStyle(.link).font(.caption)
            }
        }
    }

    /// One key/value line of the detail form. The value sits on the right and
    /// wraps onto further lines when it is long (a ProxyCommand, several
    /// identity files, an error). A plain HStack rather than LabeledContent:
    /// the grouped Form does not grow a LabeledContent row whose trailing
    /// text wraps, so the text spilled over the rows around it.
    private func row(_ label: String, _ value: String, mono: Bool = false) -> some View {
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
            Text("\(edit.kind.title) for \(edit.host)").font(.headline)
            Text(hint).font(.callout).foregroundColor(.secondary).fixedSize(horizontal: false, vertical: true)
            if edit.kind == .totp {
                TextField("base32 secret or otpauth://totp/… URI", text: $value)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))
            } else {
                RevealableSecretField(placeholder: edit.kind == .password ? "password" : "passphrase", text: $value)
                Text("\(value.count) characters typed").font(.caption).foregroundColor(.secondary)
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

    private var hint: String {
        switch edit.kind {
        case .password: return "Stored in your login keychain and used whenever ssh asks for the password of this host."
        case .passphrase: return "Passphrase of the private key ssh uses for this host. Not needed if the key is loaded in ssh-agent."
        case .totp: return "The secret from your authenticator setup. shellder will answer verification-code prompts with a fresh code."
        }
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
