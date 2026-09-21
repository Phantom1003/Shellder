import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Start at login", isOn: $model.startAtLogin)
                Toggle("Start silently", isOn: $model.silentLaunch)
                    .disabled(!model.keepInBackground || !model.startAtLogin)
                if let e = model.settingsError { Text(e).foregroundColor(.red) }
            }
            Section("Background") {
                Toggle("Run in background", isOn: $model.keepInBackground)
                Toggle("Show in menu bar", isOn: $model.showMenuBarIcon)
                    .disabled(!model.keepInBackground)
            }
            Section("Appearance") {
                Picker("Language", selection: $model.language) {
                    ForEach(Localization.available, id: \.id) { Text($0.name).tag($0.id) }
                }
                if model.language != Localization.launched {
                    Button("Relaunch to apply") { Relaunch.now() }
                }
            }
            Section("Keychain") {
                LabeledContent("Vault") {
                    if let refusal = model.keychainRefusal {
                        Text(refusal).foregroundColor(.orange).multilineTextAlignment(.trailing)
                    } else {
                        Text("readable").foregroundColor(.secondary)
                    }
                }
                // The keychain asks once, and a dialog dismissed at launch
                // leaves every host looking like it has nothing stored.
                Button("Ask the keychain for access") { model.requestKeychainAccess() }
                Text("Secrets live in one login-keychain item. If the keychain was not allowed to open it, nothing is lost — ask again here.")
                    .font(.caption).foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Section("Reconnect policy") {
                LabeledContent("Health check") { Text("every \(Int(Config.healthInterval))s") }
                LabeledContent("Back-off") { Text("\(Int(Config.backoffMin))s → \(Int(Config.backoffMax))s, ×2") }
                LabeledContent("Give up after") { Text("never") }
                LabeledContent("Connect timeout") { Text("\(Int(Config.connectTimeout))s") }
            }
            Section("Files") {
                pathRow("ssh config", Config.sshConfigFile, reveal: true)
                if model.configFiles.count > 1 {
                    LabeledContent("Included") {
                        Text(model.configFiles.dropFirst().map(Config.abbreviateHome).joined(separator: "\n"))
                            .font(.system(.caption, design: .monospaced)).multilineTextAlignment(.trailing)
                    }
                }
                pathRow("Log", Config.logFile, reveal: true)
                pathRow("askpass socket", Config.socketFile, reveal: false)
                if Launchd.installed, let p = Launchd.installedPath, p != Config.selfPath {
                    Label("LaunchAgent points at another copy: \(Config.abbreviateHome(p))", systemImage: "exclamationmark.triangle")
                        .foregroundColor(.orange)
                }
            }
            UpdateSection(updater: model.updater)
        }
        .formStyle(.grouped)
        .frame(minWidth: 480, minHeight: 320)
    }

    private func pathRow(_ label: LocalizedStringKey, _ path: String, reveal: Bool) -> some View {
        LabeledContent(label) {
            HStack(spacing: 6) {
                Text(Config.abbreviateHome(path)).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                if reveal {
                    Button { Editor.open(path) } label: { Image(systemName: "arrow.up.forward.square") }
                        .buttonStyle(.borderless).help("Open")
                }
            }
        }
    }
}

/// Version, the automatic check switch and one row that tells where the
/// update stands, with the button that moves it on (check, install).
private struct UpdateSection: View {
    @ObservedObject var updater: Updater

    var body: some View {
        Section("Software update") {
            LabeledContent("Version") { Text(Updater.version) }
            Toggle("Check for updates automatically", isOn: $updater.automatic)
            HStack(spacing: 8) {
                switch updater.state {
                case .idle:
                    Text("Not checked yet").foregroundColor(.secondary)
                case .checking:
                    ProgressView().controlSize(.small)
                    Text("Checking…").foregroundColor(.secondary)
                case .upToDate:
                    Text("Up to date").foregroundColor(.secondary)
                case .available(let r):
                    Text("Version \(r.version) is available")
                    Link("Release notes", destination: r.page)
                case .downloading(let r, let fraction):
                    ProgressView(value: fraction).frame(width: 100)
                    Text("Downloading \(r.version)…").foregroundColor(.secondary)
                case .installing:
                    ProgressView().controlSize(.small)
                    Text("Installing…").foregroundColor(.secondary)
                case .failed(let message):
                    Text(message).foregroundColor(.red)
                }
                Spacer()
                switch updater.state {
                case .available:
                    Button("Install and relaunch") { updater.install() }
                case .checking, .downloading, .installing:
                    EmptyView()
                default:
                    Button("Check now") { updater.check(manual: true) }
                }
            }
        }
    }
}
