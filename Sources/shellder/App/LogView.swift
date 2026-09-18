import SwiftUI

/// Tail of ~/Library/Logs/shellder.log, refreshed while visible.
struct LogView: View {
    @EnvironmentObject var model: AppModel
    @State private var filter = ""
    @State private var follow = true

    private var lines: [String] {
        let f = filter.trimmingCharacters(in: .whitespaces)
        if f.isEmpty { return model.logLines }
        return model.logLines.filter { $0.localizedCaseInsensitiveContains(f) }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text("Log").font(.caption.weight(.semibold))
                TextField("filter (host name, ERROR, …)", text: $filter)
                    .textFieldStyle(.roundedBorder).controlSize(.small).frame(maxWidth: 260)
                Toggle("Follow", isOn: $follow).controlSize(.small)
                Spacer()
                Button("Open file") {
                    _ = Log.appendHandle()
                    NSWorkspace.shared.open(URL(fileURLWithPath: Config.logFile))
                }.controlSize(.small)
                Button { model.showLog = false } label: { Image(systemName: "xmark") }
                    .buttonStyle(.borderless).controlSize(.small)
            }
            .padding(.horizontal, 10).padding(.vertical, 5)
            .background(.bar)
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        ForEach(Array(lines.enumerated()), id: \.offset) { i, line in
                            Text(line)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(color(line))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .id(i)
                        }
                    }
                    .padding(8)
                }
                .background(OverlayScrollers())
                .onChange(of: model.logLines.count) { _ in
                    if follow, let last = lines.indices.last { proxy.scrollTo(last, anchor: .bottom) }
                }
                .onAppear {
                    if let last = lines.indices.last { proxy.scrollTo(last, anchor: .bottom) }
                }
            }
        }
    }

    private func color(_ line: String) -> Color {
        if line.contains(" ERROR ") { return .red }
        if line.contains(" WARNING ") { return .orange }
        if line.contains(" PROMPT ") { return .blue }
        if line.contains(" SSH ") { return .purple }
        return .primary
    }
}
