import SwiftUI

/// Content of the floating prompt window: the head of the prompt queue.
struct PromptHostView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        if let p = model.currentPrompt {
            PromptSheet(request: p).id(p.id)
        } else {
            Text("No pending prompt").foregroundColor(.secondary).padding(40)
        }
    }
}

/// A question from ssh that the stored credentials could not answer.
struct PromptSheet: View {
    @EnvironmentObject var model: AppModel
    let request: PromptRequest
    @State private var answer = ""
    @State private var save = true

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: icon).font(.system(size: 30)).foregroundColor(iconColor).frame(width: 36)
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.headline)
                    Text("ssh → \(request.host)").foregroundColor(.secondary)
                }
                Spacer()
                if model.pendingPrompts > 1 {
                    Text("+\(model.pendingPrompts - 1) more").font(.caption).foregroundColor(.secondary)
                }
            }
            // The prompt text itself (fingerprint, key path, …). A plain Text
            // sizes the window correctly; the ScrollView only kicks in for
            // unusually long prompts.
            ScrollView(.vertical) {
                Text(request.prompt.trimmingCharacters(in: .whitespacesAndNewlines))
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
            }
            .frame(minHeight: request.kind == .confirm ? 130 : 44, maxHeight: request.kind == .confirm ? 260 : 120)
            .background(Color.secondary.opacity(0.08))
            .cornerRadius(6)

            if request.kind == .confirm {
                Text("Compare the fingerprint with what the server's administrator gave you. Answering yes adds the key to ~/.ssh/known_hosts.")
                    .font(.callout).foregroundColor(.secondary).fixedSize(horizontal: false, vertical: true)
            } else {
                if request.kind == .totp || request.kind == .other {
                    TextField(placeholder, text: $answer).textFieldStyle(.roundedBorder)
                        .font(.system(.body, design: .monospaced))
                } else {
                    RevealableSecretField(placeholder: placeholder, text: $answer)
                }
                if request.allowSave {
                    Toggle("Save in Keychain so shellder can answer this itself next time", isOn: $save)
                } else if request.kind == .totp {
                    Text("Store the TOTP secret under Credentials and shellder will generate these codes for you.")
                        .font(.caption).foregroundColor(.secondary)
                }
            }

            HStack {
                Spacer()
                Button(request.kind == .confirm ? "Don't connect" : "Cancel") {
                    model.answerPrompt(request.id, PromptResponse(answer: request.kind == .confirm ? "no" : nil))
                }
                .keyboardShortcut(.cancelAction)
                Button(primaryTitle) {
                    let r = request.kind == .confirm
                        ? PromptResponse(answer: "yes")
                        : PromptResponse(answer: answer, save: request.allowSave && save)
                    model.answerPrompt(request.id, r)
                }
                .keyboardShortcut(.defaultAction)
                .disabled(request.kind != .confirm && answer.isEmpty)
            }
        }
        .padding(20)
        .frame(width: 480)
    }

    private var title: String {
        switch request.kind {
        case .password: return "Password needed"
        case .passphrase: return "Key passphrase needed"
        case .totp: return "Verification code needed"
        case .confirm: return "Confirm host key"
        case .other: return "ssh is asking"
        }
    }

    private var placeholder: String {
        switch request.kind {
        case .password: return "password"
        case .passphrase: return "passphrase"
        case .totp: return "code from your authenticator"
        default: return "answer"
        }
    }

    private var primaryTitle: String { request.kind == .confirm ? "Yes, connect" : "Continue" }

    private var icon: String {
        switch request.kind {
        case .password, .passphrase: return "key.fill"
        case .totp: return "number.circle.fill"
        case .confirm: return "shield.lefthalf.filled"
        case .other: return "questionmark.circle.fill"
        }
    }

    private var iconColor: Color { request.kind == .confirm ? .orange : .accentColor }
}
