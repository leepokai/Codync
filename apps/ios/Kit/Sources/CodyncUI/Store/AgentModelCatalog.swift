import CodyncKit
import SwiftUI

/// One agent login's model list as its computer's host has it, for the model pickers (`AgentModelPicker`,
/// `ModelPanel`, `ApprovalPicker`): the list the computer gave last shows at once (`BotStore.models`, which the
/// host's `models` events keep current), and is asked for again when the computer, agent, login profile or link
/// changes. `retry` (the refresh) asks the computer to read it from the agent again.
@Observable @MainActor final class AgentModelCatalog {
    private(set) var loading = false
    private(set) var failure: String?
    var retry = 0
    private var login: AgentLogin?
    @ObservationIgnored private weak var store: BotStore?
    @ObservationIgnored private var asked = 0

    var catalog: AgentModels? { login.flatMap { store?.models(for: $0) } }

    /// What a view's `.task(id:)` loads the list on.
    func key(_ store: BotStore, backend: String, loginProfile: String?) -> String {
        "\(store.computer.id)/\(backend)/\(loginProfile ?? "")/\(store.connection == .online)/\(retry)"
    }

    /// Loads `backend`'s models for `loginProfile`, showing the one it had meanwhile; `refresh` (or a `retry`) has the
    /// computer read them from the agent again. `animation` shows the new list.
    func load(_ store: BotStore, backend: String, loginProfile: String?, refresh: Bool = false, animation: Animation?) async {
        let login = AgentLogin(backend: backend, loginProfile: loginProfile)
        self.store = store
        self.login = login
        failure = nil
        loading = false
        guard backend != "custom" else { return }
        guard store.connection == .online, let client = store.client else {
            if catalog == nil { failure = "Connect to this computer to load its models. You can still enter a model ID." }
            return
        }
        let refresh = refresh || retry != asked
        asked = retry
        loading = true
        do {
            let response = try await client.agentModels(backend, loginProfile: loginProfile, refresh: refresh)
            try Task.checkCancellation()
            withAnimation(animation) { store.keepModels(response, for: login) }
        } catch {
            guard !Task.isCancelled else { return }
            failure = "Couldn't load models: \(error.localizedDescription)"
        }
        loading = false
    }
}
