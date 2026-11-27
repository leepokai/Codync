import CodyncKit
import SwiftUI

/// The apps given MCP access on a computer: each one's level and expiry, and Revoke. Apps are added and their
/// access set on the computer itself (the host takes those calls from loopback only). The computer is the
/// `BotStore` in the environment.
public struct McpAccessSheet: View {
    @Environment(BotStore.self) private var store
    @State private var apps: [McpApp]?
    @State private var error: String?
    @State private var revoking: McpApp?

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader { Text(L10n.tr("MCP access")) }
            CardForm {
                CardSection(footer: "Approvals are never answered through MCP: every request still comes to your devices.") {
                    if let error {
                        Text(error).font(.subheadline).foregroundStyle(Palette.danger)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if let apps {
                        if apps.isEmpty {
                            Text(L10n.tr("No apps yet. They're added on the computer."))
                                .font(.subheadline).foregroundStyle(Palette.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        ForEach(apps) { app in row(app) }
                    } else if error == nil {
                        Spinner().frame(maxWidth: .infinity)
                    }
                }
            }
            .animation(Motion.layout, value: apps)
        }
        .background(Palette.background)
        .task { await load() }
        .codyncDialog(L10n.fill(L10n.tr("Revoke {name}?"), ["name": revoking?.label ?? ""]),
                      isPresented: Binding(get: { revoking != nil }, set: { if !$0 { revoking = nil } }),
                      message: "Its token stops working right away.") {
            guard let target = revoking else { return [] }
            return [DialogAction("Revoke", destructive: true) { Task { await revoke(target) } }]
        }
    }

    private func row(_ app: McpApp) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(app.label).font(.body).foregroundStyle(Palette.text)
                Text(app.detail()).font(.footnote).foregroundStyle(Palette.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            IconButton("Revoke access", systemImage: "xmark.circle") { revoking = app }
                .accessibilityIdentifier("mcp.revoke.\(app.id)")
        }
    }

    private func load() async {
        guard let client = store.client else { return }
        do {
            apps = try await client.mcpApps().active
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func revoke(_ app: McpApp) async {
        guard let client = store.client else { return }
        do {
            try await client.revokeMcpApp(id: app.id)
            await load()
        } catch {
            self.error = error.localizedDescription
        }
    }
}
