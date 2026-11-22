import CodyncKit
import SwiftUI

/// Advisor mode's agent for the bot's threads: picked in place with its model and login, as the bot's own.
struct ThreadsAgentRow: View {
    @Binding var draft: BotDraft
    /// The saved bot: the logins resting.
    let bot: Bot?
    @Environment(BotStore.self) private var model

    /// The threads' agent in the draft; nothing is set once Advisor mode is off.
    private var agent: Binding<BotFallback> {
        Binding(get: { draft.threadAgent ?? BotFallback(backend: "") },
                set: { new in if draft.threadAgent != nil { draft.threadAgent = new } })
    }

    var body: some View {
        let agent = agent
        let backends = model.hello?.backends ?? []
        let backend = backends.first { $0.id == agent.wrappedValue.backend }
        Field("Threads' agent") {
            AgentModelPicker(backend: agent.wrappedValue.backend, selection: agent.model, options: agent.modelOptions,
                             agents: backends.map { ($0.id, model.agentLabel($0)) },
                             onAgentChange: { draft.setThreadAgent($0) },
                             restLine: bot?.rest(of: agent.wrappedValue.login).map { L10n.tr($0.line) }) {
                if let backend, backend.loginProfiles != nil {
                    LoginProfileRow(backend: backend, loginProfile: agent.loginProfile)
                }
            }
        }
    }
}
