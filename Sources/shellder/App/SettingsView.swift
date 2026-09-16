import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Start shellder at login (LaunchAgent)", isOn: $model.startAtLogin)
                Toggle("Open the window when launched manually", isOn: $model.openWindowAtLaunch)
                if let e = model.settingsError { Text(e).font(.caption).foregroundColor(.red) }
                Text("The LaunchAgent starts this copy of the app in the background at login and restarts it after a crash. Quitting from the menu stays quit.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Appearance") {
                Toggle("Show icon in the Dock", isOn: $model.showDockIcon)
                Toggle("Show icon in the menu bar", isOn: $model.showMenuBarIcon)
                    .disabled(!model.showDockIcon && model.showMenuBarIcon)
                Text("Keep at least one of them, or the app becomes hard to reach.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Reconnect policy") {
                LabeledContent("Health check", value: "ssh -O check every \(Int(Config.healthInterval))s")
                LabeledContent("Back-off", value: "\(Int(Config.backoffMin))s → \(Int(Config.backoffMax))s, ×2 per quick failure")
                LabeledContent("Give up after", value: "never; \(Config.maxQuickFailures) quick failures jump to the maximum wait")
                LabeledContent("Connect timeout", value: "\(Int(Config.connectTimeout))s")
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

    private func pathRow(_ label: String, _ path: String, reveal: Bool) -> some View {
        LabeledContent(label) {
            HStack(spacing: 6) {
                Text(Config.abbreviateHome(path)).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                if reveal {
                    Button { NSWorkspace.shared.open(URL(fileURLWithPath: path)) } label: { Image(systemName: "arrow.up.forward.square") }
                        .buttonStyle(.borderless).help("Open")
                }
            }
        }
    }
}
