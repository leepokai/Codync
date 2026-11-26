import CodyncKit
import SwiftUI

/// Permissions: the modes the bot's agent offers, its Auto mode first, then approving everything (`ApprovalChoice`).
struct ApprovalPicker: View {
    @Binding var draft: BotDraft
    /// The model picker's Retry, so a list that failed the first time fills both.
    var reload = 0
    @Environment(BotStore.self) private var store
    @State private var models = AgentModelCatalog()

    var body: some View {
        let (choices, selected) = ApprovalChoice.choices(permission: draft.permission, mode: draft.mode, catalog: models.catalog)
        ValueRow("Permissions", detail: [selected.note, L10n.tr(selected.detail)].compactMap { $0 }.joined(separator: " ")) {
            ChoicePicker(selection: Binding(get: { selected.id }, set: { draft.setApproval($0) }),
                         options: choices.map { ($0.id, $0.label) })
        }
        // The agent's modes come with its model list. The model picker says why one failed; Auto, Ask me and Approve
        // automatically stay on offer meanwhile.
        .task(id: models.key(store, backend: draft.backend, loginProfile: draft.loginProfile)) {
            await models.load(store, backend: draft.backend, loginProfile: draft.loginProfile, animation: nil)
        }
        .onChange(of: reload) { models.retry += 1 }
    }
}
