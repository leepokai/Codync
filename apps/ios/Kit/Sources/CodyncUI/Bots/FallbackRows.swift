import CodyncKit
import SwiftUI

// The Agent card's fallbacks (docs/features/agent-fallback.md): while the bot runs on one, a banner to switch back;
// after Model, its fallbacks in order, moved and removed from each row's menu and added with +. Edits change the
// draft, which saves like the rest of the form.

/// The Agent card's first row while the bot runs on one of its fallbacks.
struct FallbackBanner: View {
    let bot: Bot
    let step: AgentStep
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            AgentIcon(registry: model.agentRegistry(step.agent), size: 18)
            Text(text)
                .font(.subheadline)
                .foregroundStyle(Palette.text)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 8)
            Button(L10n.tr("Switch back")) { model.switchBack(bot.id) }
                .buttonStyle(.secondary)
                .disabled(model.isOffline)
        }
    }

    private var text: String {
        let to = model.backendName(step.agent), own = model.backendName(bot.backend)
        guard let rest = bot.rest(of: bot.backend) else { return "Using \(to) while \(own) can't answer" }
        return "Using \(to) until \(own) resets at \(RelativeTime.clock(Date(milliseconds: rest.until)))"
    }
}

/// The fallbacks (a header with ⓘ and +, a row per fallback, the caption) and whether the bot may change them.
struct FallbackList: View {
    @Binding var draft: BotDraft
    /// The saved bot: the fallback it's on and the agents resting.
    let bot: Bot?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var adding = false
    @State private var explaining = false

    private var fallbacks: [BotFallback] { draft.fallbacks ?? [] }

    /// Agents a new fallback can use: each agent appears once, the bot's own included.
    private var choices: [Backend] {
        (model.hello?.backends ?? []).filter { b in b.id != draft.backend && !fallbacks.contains { $0.backend == b.id } }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if fallbacks.isEmpty {
                Text(L10n.tr("No fallbacks yet.")).foregroundStyle(Palette.secondary).transition(.opacity)
            }
            ForEach(Array(fallbacks.enumerated()), id: \.element.backend) { i, fallback in
                row(fallback, at: i).transition(.opacity)
            }
            Text(L10n.tr("If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.").replacingOccurrences(of: "{agent}", with: model.backendName(draft.backend)))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .lineSpacing(2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .codyncSheet(isPresented: $adding) { AddFallbackSheet(choices: choices) { new in change { $0.append(new) } } }
        .codyncSheet(isPresented: $explaining) { FallbackInfoSheet() }
        SwitchRow("Let the bot change its fallbacks", detail: "It can set this list itself, for example when you ask it to.",
                  isOn: Binding(get: { draft.selfFallbacks ?? true }, set: { draft.selfFallbacks = $0 }))
    }

    private var header: some View {
        HStack(spacing: 4) {
            Text(L10n.tr("Fallbacks")).foregroundStyle(Palette.text)
            IconButton("About fallbacks", systemImage: "info.circle") { explaining = true }
            Spacer(minLength: 8)
            if fallbacks.count < BotFallback.limit, !choices.isEmpty {
                IconButton("Add fallback", systemImage: "plus") { adding = true }
            }
        }
    }

    private func row(_ fallback: BotFallback, at i: Int) -> some View {
        let name = model.hello?.backends.first { $0.id == fallback.backend }.map(model.agentLabel) ?? model.backendName(fallback.backend)
        return HStack(spacing: 10) {
            Text(L10n.tr("\(i + 1)")).font(.subheadline.monospacedDigit()).foregroundStyle(Palette.secondary)
            AgentIcon(registry: model.agentRegistry(fallback.backend), size: 18)
            VStack(alignment: .leading, spacing: 3) {
                Text("\(L10n.tr(name)) · \(L10n.tr(fallback.model ?? "Default"))").foregroundStyle(Palette.text).lineLimit(1)
                if let rest = bot?.rest(of: fallback.backend) {
                    Text(rest.line).font(.caption).foregroundStyle(Palette.secondary)
                }
            }
            Spacer(minLength: 8)
            if bot?.agentStep?.agent == fallback.backend {
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
        items.append(MenuItem("Remove", icon: "trash", destructive: true, divider: !items.isEmpty) { change { $0.remove(at: i) } })
        return items
    }

    private func change(_ edit: (inout [BotFallback]) -> Void) {
        var list = fallbacks
        edit(&list)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { draft.fallbacks = list }
    }
}

/// +: a new fallback's agent and model, from the same pickers as the bot's own.
struct AddFallbackSheet: View {
    let choices: [Backend]
    let add: (BotFallback) -> Void
    @State private var fallback: BotFallback
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss

    init(choices: [Backend], add: @escaping (BotFallback) -> Void) {
        self.choices = choices
        self.add = add
        _fallback = State(initialValue: BotFallback(backend: (choices.first(where: \.available) ?? choices.first)?.id ?? ""))
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Add fallback") {
                IconButton("Add", systemImage: "checkmark") {
                    add(fallback)
                    dismiss()
                }
                .disabled(fallback.backend.isEmpty)
            }
            CardForm {
                CardSection {
                    AgentModelPicker(backend: fallback.backend, selection: $fallback.model, options: $fallback.modelOptions,
                                     agents: choices.map { ($0.id, model.agentLabel($0)) },
                                     onAgentChange: { fallback.backend = $0; fallback.model = nil; fallback.modelOptions = [:] })
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
