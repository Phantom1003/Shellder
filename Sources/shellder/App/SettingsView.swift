import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Start at login", isOn: $model.startAtLogin)
                Toggle("Start silently", isOn: $model.silentLaunch)
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
                    Button("Relaunch to apply") { Localization.relaunch() }
                }
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
