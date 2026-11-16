import CodyncKit
import SwiftUI

// The Agent card's fallbacks (docs/features/agent-fallback.md): while the bot runs on one, a banner to switch back;
// after Model, its fallbacks in order, moved, edited and removed from each row's menu and added with +. Edits change the
// draft, which saves like the rest of the form.

/// The Agent card's first row while the bot runs on one of its fallbacks (`step`).
struct FallbackBanner: View {
    let bot: Bot
    let step: AgentStep
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            AgentIcon(registry: model.agentRegistry(step.agent), size: 18)
            Text(bot.stepBanner(name: name) ?? "")
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

/// The fallbacks (a header with ⓘ and +, a row per fallback, the caption) and whether the bot may change them.
struct FallbackList: View {
    @Binding var draft: BotDraft
    /// The saved bot: the fallback it's on and the agents resting.
    let bot: Bot?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The step the sheet adds (at the end) or edits in place.
    @State private var sheet: SheetStep?
    @State private var explaining = false

    /// A step as the sheet opened on it: its place, and its own (nil for a new one).
    private struct SheetStep: Identifiable {
        let id: Int
        var initial: BotFallback?
    }

    private var fallbacks: [BotFallback] { draft.fallbacks ?? [] }

    /// Each login is in the chain once, the bot's own included: an agent comes again only on another of its login profiles.
    /// The step being edited (`i`) keeps its own login.
    private func taken(but i: Int) -> [AgentLogin] {
        [draft.login] + fallbacks.enumerated().filter { $0.offset != i }.map(\.element.login)
    }

    /// Agents step `i` can use: the ones with a login the rest of the chain doesn't have, and its `own` (its login may
    /// have signed out since).
    private func choices(but i: Int, own: String? = nil) -> [Backend] {
        (model.hello?.backends ?? []).filter { $0.id == own || !$0.freeLogins(taken: taken(but: i)).isEmpty }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if fallbacks.isEmpty {
                Text(L10n.tr("No fallbacks yet.")).foregroundStyle(Palette.secondary).transition(.opacity)
            }
            ForEach(Array(fallbacks.enumerated()), id: \.element.login) { i, fallback in
                row(fallback, at: i).transition(.opacity)
            }
            Text(L10n.tr("If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.").replacingOccurrences(of: "{agent}", with: model.backendName(draft.backend)))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .lineSpacing(2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .codyncSheet(item: $sheet) { step in
            FallbackSheet(choices: choices(but: step.id, own: step.initial?.backend), taken: taken(but: step.id), initial: step.initial) { new in
                change { list in if step.initial != nil, list.indices.contains(step.id) { list[step.id] = new } else { list.append(new) } }
            }
        }
        .codyncSheet(isPresented: $explaining) { FallbackInfoSheet() }
        SwitchRow("Let the bot change its fallbacks", detail: "It can set this list itself, for example when you ask it to.",
                  isOn: Binding(get: { draft.selfFallbacks ?? true }, set: { draft.selfFallbacks = $0 }))
    }

    private var header: some View {
        HStack(spacing: 4) {
            Text(L10n.tr("Fallbacks")).foregroundStyle(Palette.text)
            IconButton("About fallbacks", systemImage: "info.circle") { explaining = true }
            Spacer(minLength: 8)
            if fallbacks.count < BotFallback.limit, !choices(but: fallbacks.count).isEmpty {
                IconButton("Add fallback", systemImage: "plus") { sheet = SheetStep(id: fallbacks.count) }
            }
        }
    }

    private func row(_ fallback: BotFallback, at i: Int) -> some View {
        let backend = model.hello?.backends.first { $0.id == fallback.backend }
        let agent = backend.map(model.agentLabel) ?? model.backendName(fallback.backend)
        let loginProfile = fallback.loginProfile.flatMap { id in backend?.loginProfiles?.first { $0.id == id } }
        let name = loginProfile.map { "\(L10n.tr(agent)) (\($0.name ?? L10n.tr("New login profile")))" } ?? L10n.tr(agent)
        return HStack(spacing: 10) {
            Text(L10n.tr("\(i + 1)")).font(.subheadline.monospacedDigit()).foregroundStyle(Palette.secondary)
            AgentIcon(registry: model.agentRegistry(fallback.backend), size: 18)
            VStack(alignment: .leading, spacing: 3) {
                Text("\(name) · \(L10n.tr(fallback.model ?? "Default"))").foregroundStyle(Palette.text).lineLimit(1)
                if let rest = bot?.rest(of: fallback.login) {
                    Text(rest.line).font(.caption).foregroundStyle(Palette.secondary)
                }
            }
            Spacer(minLength: 8)
            if bot?.stepLogin == fallback.login {
                Text(L10n.tr("In use"))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Palette.bubbleUser, in: Capsule())
            }
            DropdownMenu { menu(at: i) } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(Palette.secondary)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .accessibilityLabel("\(name) fallback actions")
        }
    }

    private func menu(at i: Int) -> [MenuItem] {
        var items: [MenuItem] = []
        if i > 0 { items.append(MenuItem("Move up", icon: "arrow.up") { change { $0.swapAt(i, i - 1) } }) }
        if i < fallbacks.count - 1 { items.append(MenuItem("Move down", icon: "arrow.down") { change { $0.swapAt(i, i + 1) } }) }
        items.append(MenuItem("Edit", icon: "pencil") { sheet = SheetStep(id: i, initial: fallbacks[i]) })
        items.append(MenuItem("Remove", icon: "trash", destructive: true, divider: true) { change { $0.remove(at: i) } })
        return items
    }

    private func change(_ edit: (inout [BotFallback]) -> Void) {
        var list = fallbacks
        edit(&list)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { draft.fallbacks = list }
    }
}

/// + (or a row's Edit, with the step's own as `initial`): a fallback's agent (or the bot's own on another of its login
/// profiles) and model, from the same pickers as the bot's own.
struct FallbackSheet: View {
    let choices: [Backend]
    let taken: [AgentLogin]
    let initial: BotFallback?
    let save: (BotFallback) -> Void
    @State private var fallback: BotFallback
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss

    private var agent: Backend? { choices.first { $0.id == fallback.backend } }
    /// The step's own while its agent stays the same.
    private var own: BotFallback? { initial.flatMap { $0.backend == fallback.backend ? $0 : nil } }
    /// A step keeps its own login, even one that has signed out since.
    private var free: [String?] {
        let open = agent?.freeLogins(taken: taken) ?? []
        guard let own, !open.contains(own.loginProfile) else { return open }
        return open + [own.loginProfile]
    }
    /// The picked login, or the first the chain doesn't have yet.
    private var loginProfile: String? { free.contains(fallback.loginProfile) ? fallback.loginProfile : free.first ?? nil }

    init(choices: [Backend], taken: [AgentLogin], initial: BotFallback?, save: @escaping (BotFallback) -> Void) {
        self.choices = choices
        self.taken = taken
        self.initial = initial
        self.save = save
        _fallback = State(initialValue: initial ?? BotFallback(backend: (choices.first(where: \.available) ?? choices.first)?.id ?? ""))
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(initial != nil ? "Edit fallback" : "Add fallback") {
                IconButton(initial != nil ? "Save" : "Add", systemImage: "checkmark") {
                    var new = fallback
                    new.loginProfile = loginProfile
                    save(new)
                    dismiss()
                }
                .disabled(fallback.backend.isEmpty)
            }
            CardForm {
                CardSection {
                    AgentModelPicker(backend: fallback.backend, selection: $fallback.model, options: $fallback.modelOptions,
                                     agents: choices.map { ($0.id, model.agentLabel($0)) },
                                     // Back on the step's own agent, its model and login come back with it.
                                     onAgentChange: { id in fallback = initial.flatMap { $0.backend == id ? $0 : nil } ?? BotFallback(backend: id) },
                                     collapsible: false)
                    if let agent, agent.loginProfiles != nil {
                        LoginProfileRow(backend: agent, loginProfile: Binding(get: { loginProfile }, set: { fallback.loginProfile = $0 }), free: free)
                    }
                }
            }
        }
        .background(Palette.background)
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
