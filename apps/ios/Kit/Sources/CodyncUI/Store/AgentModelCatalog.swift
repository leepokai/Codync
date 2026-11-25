import CodyncKit
import SwiftUI

/// One agent login's model list as its computer's host has it, for the model pickers (`AgentModelPicker`,
/// `ModelPanel`): loaded again when the computer, agent, login profile or link changes, or on `retry`.
@Observable @MainActor final class AgentModelCatalog {
    private(set) var catalog: AgentModels?
    private(set) var loading = false
    private(set) var failure: String?
    var retry = 0
    @ObservationIgnored private var scope: String?

    /// What a view's `.task(id:)` loads the list on.
    func key(_ store: BotStore, backend: String, loginProfile: String?) -> String {
        "\(store.computer.id)/\(backend)/\(loginProfile ?? "")/\(store.connection == .online)/\(retry)"
    }

    /// Loads `backend`'s models for `loginProfile`; a list of another login is dropped first, the same one kept while
    /// it loads again. `animation` shows the new list.
    func load(_ store: BotStore, backend: String, loginProfile: String?, animation: Animation?) async {
        let scope = "\(store.computer.id)/\(backend)/\(loginProfile ?? "")"
        if self.scope != scope {
            catalog = nil
            self.scope = scope
        }
        failure = nil
        loading = false
        guard backend != "custom" else { return }
        guard store.connection == .online, let client = store.client else {
            failure = "Connect to this computer to load its models. You can still enter a model ID."
            return
        }
        loading = true
        do {
            let response = try await client.agentModels(backend, loginProfile: loginProfile)
            try Task.checkCancellation()
            withAnimation(animation) { catalog = response }
        } catch {
            guard !Task.isCancelled else { return }
            failure = "Couldn't load models: \(error.localizedDescription)"
        }
        loading = false
    }
}
