import CodyncKit
import SwiftUI

// The Agent card's fallbacks (docs/features/agent-fallback.md): while the bot runs on one, a banner to switch back;
// after Model, its fallbacks in order, each picked in place, moved and removed from its row's menu and added with +.
// Edits change the draft, which saves like the rest of the form.

/// The Agent card's first row while the bot runs on one of its fallbacks (`step`).
struct FallbackBanner: View {
    let bot: Bot
    let step: AgentStep
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            AgentIcon(registry: model.agentRegistry(step.agent), size: 18)
            // The phrase is translated before its names go in: a login profile named "Work while traveling" stays whole.
            Text(bot.stepBannerPhrase(name: name).map { L10n.fill(L10n.tr($0.key), $0.values) } ?? "")
                .font(.subheadline)
                .foregroundStyle(Palette.text)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 8)
            Button(L10n.tr("Switch back")) { model.switchBack(bot.id) }
                .buttonStyle(.secondary)
                .disabled(model.isOffline)
        }
    }

    /// "Claude Code (Work)": a login on a login profile, and the computer's own when it's asked for by name.
    private func name(_ login: AgentLogin, own: Bool) -> String {
        let backend = model.hello?.backends.first { $0.id == login.backend }
        let agent = model.backendName(login.backend)
        let loginProfile = login.loginProfile.map { id in backend?.loginProfiles?.first { $0.id == id }?.name ?? L10n.tr("New login profile") }
            ?? (own ? backend?.defaultLoginProfile?.name ?? L10n.tr("Default login profile") : nil)
        return loginProfile.map { "\(agent) (\($0))" } ?? agent
    }
}

/// The fallbacks (a header with ⓘ and +, a row per fallback, the caption). Each fallback is picked in place like the
/// bot's own agent, and moved or removed from its menu.
struct FallbackList: View {
    @Binding var draft: BotDraft
    /// The saved bot: the fallback it's on and the agents resting.
    let bot: Bot?
    /// Advisor mode's, after the threads' own agent.
    var threads = false
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The step + just added: it opens on its pickers.
    @State private var added: Int?
    /// Bumped when a move or removal renumbers the rows, so each starts afresh on the step now in its place.
    @State private var order = 0
    @State private var explaining = false

    private var key: WritableKeyPath<BotDraft, [BotFallback]?> { threads ? \.threadFallbacks : \.fallbacks }
    private var fallbacks: [BotFallback] { draft[keyPath: key] ?? [] }
    /// The chain's first agent: the threads' own in Advisor mode, else the bot's.
    private var head: AgentLogin { (threads ? draft.threadAgent?.login : nil) ?? draft.login }

    /// Each login is in the chain once, its first agent's included: an agent comes again only on another of its login
    /// profiles. Step `i` keeps its own login.
    private func taken(but i: Int) -> [AgentLogin] {
        [head] + fallbacks.enumerated().filter { $0.offset != i }.map(\.element.login)
    }

    /// The agents a new step can use.
    private var choices: [Backend] { (model.hello?.backends ?? []).usable(taken: taken(but: fallbacks.count)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if fallbacks.isEmpty {
                Text(L10n.tr("No fallbacks yet.")).foregroundStyle(Palette.secondary).transition(.opacity)
            }
            // By place, not login: picking another agent keeps the row (and its open picker) where it is.
            ForEach(Array(fallbacks.indices), id: \.self) { i in
                // The chat's step; Advisor mode's threads each run their own.
                FallbackRow(fallback: step(i), number: i + 1, taken: taken(but: i), bot: bot, inUse: threads ? nil : bot?.stepLogin,
                            expanded: added == i) { menu(at: i) }
                    .id(order)
                    .transition(.opacity)
            }
            Text(L10n.tr(threads
                    ? "If {agent} runs out of usage or can't answer, threads carry on with the next one."
                    : "If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.")
                .replacingOccurrences(of: "{agent}", with: model.backendName(head.backend)))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .lineSpacing(2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .codyncSheet(isPresented: $explaining) { FallbackInfoSheet() }
    }

    private var header: some View {
        HStack(spacing: 4) {
            Text(L10n.tr(threads ? "Threads' fallbacks" : "Fallbacks")).foregroundStyle(Palette.text)
            IconButton("About fallbacks", systemImage: "info.circle") { explaining = true }
            Spacer(minLength: 8)
            if fallbacks.count < BotFallback.limit, !choices.isEmpty {
                IconButton("Add fallback", systemImage: "plus") { add() }
            }
        }
    }

    /// + : a step on the first installed agent with a free login, at the end, open on its pickers.
    private func add() {
        guard let agent = choices.first(where: \.available) ?? choices.first else { return }
        let step = BotFallback.on(agent, taken: taken(but: fallbacks.count))
        added = fallbacks.count
        change { $0.append(step) }
    }

    /// Step `i` in the draft, for its row.
    private func step(_ i: Int) -> Binding<BotFallback> {
        Binding(get: { fallbacks.indices.contains(i) ? fallbacks[i] : BotFallback(backend: "") },
                set: { new in if draft[keyPath: key]?.indices.contains(i) == true { draft[keyPath: key]?[i] = new } })
    }

    private func menu(at i: Int) -> [MenuItem] {
        var items: [MenuItem] = []
        if i > 0 { items.append(MenuItem("Move up", icon: "arrow.up") { renumber { $0.swapAt(i, i - 1) } }) }
        if i < fallbacks.count - 1 { items.append(MenuItem("Move down", icon: "arrow.down") { renumber { $0.swapAt(i, i + 1) } }) }
        items.append(MenuItem("Remove", icon: "trash", destructive: true, divider: true) { renumber { $0.remove(at: i) } })
        return items
    }

    private func renumber(_ edit: (inout [BotFallback]) -> Void) {
        added = nil
        order += 1
        change(edit)
    }

    private func change(_ edit: (inout [BotFallback]) -> Void) {
        var list = fallbacks
        edit(&list)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { draft[keyPath: key] = list }
    }
}

/// One fallback: its place, its agent, model and login picked in place like the bot's own, whether it's resting or in
/// use, and its menu.
private struct FallbackRow: View {
    @Binding var fallback: BotFallback
    let number: Int
    /// The rest of the chain's logins.
    let taken: [AgentLogin]
    let bot: Bot?
    /// The step the bot runs on right now, which shows "In use"; nil where none does.
    let inUse: AgentLogin?
    let expanded: Bool
    let menu: () -> [MenuItem]
    @Environment(BotStore.self) private var model
    /// The step as it was on its own agent while another is picked: picking that agent again brings back its model and login.
    @State private var own: BotFallback?

    var body: some View {
        let choices = (model.hello?.backends ?? []).usable(taken: taken, keeping: [fallback.login] + (own.map { [$0.login] } ?? []))
        let agent = choices.first { $0.id == fallback.backend }
        let loginProfile = fallback.loginProfile.flatMap { id in agent?.loginProfiles?.first { $0.id == id } }
        // The step's login shows beside its agent: "Claude Code (Work)".
        let agents = choices.map { backend in
            let label = L10n.tr(model.agentLabel(backend))
            return (backend.id, backend.id == fallback.backend ? loginProfile.map { "\(label) (\($0.name ?? L10n.tr("New login profile")))" } ?? label : label)
        }
        let name = agents.first { $0.0 == fallback.backend }?.1 ?? model.backendName(fallback.backend)
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .top, spacing: 10) {
                Text(L10n.tr("\(number)")).font(.subheadline.monospacedDigit()).foregroundStyle(Palette.secondary)
                AgentModelPicker(backend: fallback.backend, selection: $fallback.model, options: $fallback.modelOptions, agents: agents,
                                 onAgentChange: { id in
                                     guard let next = choices.first(where: { $0.id == id }) else { return }
                                     let step = BotFallback.on(next, taken: taken, own: own)
                                     own = step == own ? nil : own ?? fallback
                                     fallback = step
                                 },
                                 expanded: expanded) {
                    if let agent, agent.loginProfiles != nil {
                        LoginProfileRow(backend: agent, loginProfile: $fallback.loginProfile, free: agent.logins(for: fallback.login, taken: taken))
                    }
                }
                if inUse == fallback.login {
                    Text(L10n.tr("In use"))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Palette.text)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .background(Palette.bubbleUser, in: Capsule())
                }
                DropdownMenu { menu() } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                // The 36pt target centered on the summary's first line.
                .padding(.vertical, -8)
                .accessibilityLabel("\(name) fallback actions")
            }
            if let rest = bot?.rest(of: fallback.login) {
                Text(L10n.tr(rest.line)).font(.caption).foregroundStyle(Palette.secondary)
            }
        }
    }
}

/// ⓘ beside Fallbacks: what they do, and what they cost.
private struct FallbackInfoSheet: View {
    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Fallbacks")
            CardForm {
                Text(L10n.tr("When this bot's agent runs out of usage, is rate limited, signed out or won't start, the next agent here carries on in the same chat, with the same memory and workspace. The bot goes back to its own agent when it recovers. A fallback that uses an API key costs money. To fall back between one agent's models, set that in the agent itself (Claude Code's fallbackModel)."))
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .background(Palette.background)
    }
}
