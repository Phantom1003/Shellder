import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Start shellder at login (LaunchAgent)", isOn: $model.startAtLogin)
                Toggle("Start silently (no window)", isOn: $model.silentLaunch)
                if let e = model.settingsError { Text(e).font(.caption).foregroundColor(.red) }
                Text("The LaunchAgent starts this copy of the app at login and restarts it after a crash; quitting from the menu stays quit. “Start silently” applies to every launch, at login or by hand: connections come up in the background and the window stays closed until you open it from the menu bar.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Background") {
                Toggle("Keep running after the last window closes", isOn: $model.keepInBackground)
                Toggle("Show an icon in the menu bar while running in the background", isOn: $model.showMenuBarIcon)
                    .disabled(!model.keepInBackground)
                Text("Closing the last window keeps the connections up: shellder leaves the Dock and stays reachable from the menu bar icon, or by opening the app again if the icon is off. With this off, closing the last window quits shellder and closes its connections.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Appearance") {
                Picker("Language", selection: $model.language) {
                    ForEach(Localization.available, id: \.id) { Text($0.name).tag($0.id) }
                }
                if model.language != Localization.launched {
                    HStack {
                        Text("The new language shows after a relaunch.").font(.caption).foregroundColor(.secondary)
                        Spacer()
                        Button("Relaunch") { Localization.relaunch() }.controlSize(.small)
                    }
                }
            }
            Section("Reconnect policy") {
                LabeledContent("Health check") { Text("ssh -O check every \(Int(Config.healthInterval))s") }
                LabeledContent("Back-off") { Text("\(Int(Config.backoffMin))s → \(Int(Config.backoffMax))s, ×2 per quick failure") }
                LabeledContent("Give up after") { Text("never, \(Config.maxQuickFailures) quick failures jump to the maximum wait") }
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
                pathRow("Executable", Config.selfPath, reveal: false)
                if Launchd.installed, let p = Launchd.installedPath, p != Config.selfPath {
                    Label("The LaunchAgent points at a different copy: \(Config.abbreviateHome(p)). Toggle “Start at login” off and on to update it.",
                          systemImage: "exclamationmark.triangle").font(.caption).foregroundColor(.orange)
                }
                Text("Shellder reads ~/.ssh/config and never modifies it. Its own state is limited to the files above, app preferences and keychain items named “shellder …”.")
                    .font(.caption).foregroundColor(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: 520)
        .fixedSize(horizontal: false, vertical: true)
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
