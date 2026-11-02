import CodyncKit
import OSLog
import SwiftUI

private let log = Logger(subsystem: "com.pokai.Codync", category: "Bots")

/// Permissions: the modes the bot's agent offers, its Auto mode first, then approving everything (`ApprovalChoice`).
struct ApprovalPicker: View {
    @Binding var draft: BotDraft
    /// The model picker's Retry, so a list that failed the first time fills both.
    var reload = 0
    @Environment(BotStore.self) private var store
    @State private var catalog: AgentModels?

    var body: some View {
        let (choices, selected) = ApprovalChoice.choices(permission: draft.permission, mode: draft.mode, catalog: catalog)
        ValueRow("Permissions", detail: [selected.note, L10n.tr(selected.detail)].compactMap { $0 }.joined(separator: " ")) {
            ChoicePicker(selection: Binding(get: { selected.id }, set: { draft.setApproval($0) }),
                         options: choices.map { ($0.id, $0.label) })
        }
        .task(id: "\(store.computer.id)/\(draft.backend)/\(store.connection == .online)/\(reload)") { await load() }
        // Another agent's modes never show; a refresh that fails keeps the ones it has.
        .onChange(of: "\(store.computer.id)/\(draft.backend)") { catalog = nil }
    }

    /// The agent's modes come with its model list (the host shares one asked for from several places at once).
    private func load() async {
        guard draft.backend != "custom", store.connection == .online, let client = store.client else { return }
        do {
            let response = try await client.agentModels(draft.backend)
            try Task.checkCancellation()
            catalog = response
        } catch {
            // The model picker says why; Auto, Ask me and Approve automatically stay on offer meanwhile.
            log.info("no agent modes for \(draft.backend, privacy: .public): \(error.localizedDescription, privacy: .public)")
        }
    }
}
