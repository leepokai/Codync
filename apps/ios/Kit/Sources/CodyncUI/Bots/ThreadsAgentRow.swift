import CodyncKit
import SwiftUI

/// Advisor mode's agent for the bot's threads: its login and model picked in place, as the bot's own.
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
        Field("Threads' agent") {
            AgentModelPicker(backend: agent.wrappedValue.backend, loginProfile: agent.wrappedValue.loginProfile, selection: agent.model, options: agent.modelOptions,
                             agents: model.loginSeats(backends, on: agent.wrappedValue.login),
                             onAgentChange: { login in
                                 draft.setThreadAgent(login.backend)
                                 draft.threadAgent?.loginProfile = login.loginProfile
                             },
                             addsLoginTo: backends.first { $0.id == agent.wrappedValue.backend && $0.loginProfiles != nil },
                             restLine: bot?.rest(of: agent.wrappedValue.login).map { L10n.tr($0.line) })
        }
    }
}
