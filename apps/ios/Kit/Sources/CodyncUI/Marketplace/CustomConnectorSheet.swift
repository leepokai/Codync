import AuthenticationServices
import CodyncKit
import SwiftUI

struct CustomConnectorSheet: View {
    private enum Mode: Hashable { case command, url, config }

    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @Environment(\.openURL) private var openURL
    @State private var name = ""
    @State private var mode = Mode.command
    @State private var target = ""
    @State private var envText = ""
    @State private var config = ""
    @State private var saving = false
    @State private var error: String?

    private var ready: Bool {
        mode == .config
            ? !config.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            : !name.trimmingCharacters(in: .whitespaces).isEmpty && !target.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Custom connector") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Add", systemImage: "plus") { save() }
                        .disabled(!ready)
                }
            }
            form
        }
        .background(Palette.background)
    }

    private var footer: String {
        switch mode {
        case .command: "Runs on \(model.hostName) in the bot's project folder. Quotes work like in a shell."
        case .url: "A remote MCP server, streamable HTTP or SSE."
        case .config: "Paste the MCP config from a README or another app (Claude, Cursor, VS Code). Every server in it is added. Saved on \(model.hostName) only."
        }
    }

    private var form: some View {
        CardForm {
            CardSection(footer: footer) {
                SegmentedChoice(selection: $mode, options: [(id: .command, label: "Command"), (id: .url, label: "URL"), (id: .config, label: "Config")])
                if mode == .config {
                    TextField("{ \"mcpServers\": { … } }", text: $config, axis: .vertical)
                        .lineLimit(6...16)
                        .font(.callout.monospaced())
                        .plainTextInput()
                } else {
                    TextField("Name", text: $name)
                    TextField(mode == .url ? "https://example.com/mcp" : "npx -y @scope/server", text: $target)
                        .font(.callout.monospaced())
                        .plainTextInput()
                }
            }
            if mode == .url {
                CardSection("Headers", footer: "Optional, one Name: value per line. Leave empty if the service has you sign in. Saved on \(model.hostName) only.") {
                    TextField("Authorization: Bearer …", text: $envText, axis: .vertical)
                        .lineLimit(2...6)
                        .font(.callout.monospaced())
                        .plainTextInput()
                }
            } else if mode == .command {
                CardSection("Environment", footer: "One KEY=value per line. Saved on \(model.hostName) only.") {
                    TextField("API_KEY=…", text: $envText, axis: .vertical)
                        .lineLimit(2...6)
                        .font(.callout.monospaced())
                        .plainTextInput()
                }
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
        .animation(Motion.layout, value: mode)
    }

    private func save() {
        saving = true
        error = nil
        let remote = mode == .url
        var pairs: [String: String] = [:]
        for line in envText.split(separator: "\n") {
            let parts = line.split(separator: remote ? ":" : "=", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
            if parts.count == 2, !parts[0].isEmpty { pairs[parts[0]] = parts[1] }
        }
        Task {
            do {
                let added = mode == .config
                    ? try await model.importConnectors(config: config)
                    : [try await model.addConnector(
                        name: name, command: remote ? nil : target, url: remote ? target : nil,
                        env: remote ? [:] : pairs, headers: remote ? pairs : [:]
                    )]
                let signIn = ConnectorSignInFlow(model: model, webAuthenticationSession: webAuthenticationSession, openURL: openURL)
                for c in added where c.needsSignIn {
                    try await signIn.signIn(c.id)
                }
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
        }
    }
}
