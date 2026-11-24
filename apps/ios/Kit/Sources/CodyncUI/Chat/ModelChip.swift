import CodyncKit
import SwiftUI

/// What a bot's chat runs on, in its message box (the desktop's `BotRunOptions`): the agent's logo and the model. It
/// opens the agent and model picker, whose rows are the installed agents' logins; each change is saved to the bot a
/// moment after it's made, as its Settings would (a typed model ID isn't saved per keystroke: each save starts a new
/// agent session). The chip, not its sheet, keeps the draft and the save, so closing the sheet right after a pick
/// still saves it.
struct ModelChip: View {
    let bot: Bot
    @Environment(BotStore.self) private var model
    @State private var open = false
    @State private var draft: BotDraft
    @State private var saveError: String?
    /// The save sent last: not cancelled with the next change (the host applies it anyway), and awaited before the
    /// next one is weighed against the bot.
    @State private var saving: Task<Void, Never>?

    init(bot: Bot) {
        self.bot = bot
        _draft = State(initialValue: BotDraft(bot))
    }

    var body: some View {
        Button { open = true } label: {
            HStack(spacing: 5) {
                AgentIcon(registry: model.agentRegistry(bot.backend), size: 14)
                Text(bot.model ?? L10n.tr("Default"))
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, 8)
            .frame(height: 34)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        // Saving needs the computer, as the settings' Save does.
        .disabled(model.connection != .online)
        .accessibilityLabel("Model")
        .help("Model")
        .codyncSheet(isPresented: $open) { ModelSheet(bot: bot, draft: $draft, error: saveError) }
        // A change made elsewhere (a bot that manages the team, another device) replaces each field not edited here.
        .onChange(of: BotDraft(bot)) { base, fresh in
            draft = base.id == fresh.id ? draft.rebased(from: base, onto: fresh) : fresh
        }
        .task(id: draft) { await autosave(draft) }
    }

    private func autosave(_ draft: BotDraft) async {
        try? await Task.sleep(for: .milliseconds(600))
        guard !Task.isCancelled else { return }
        await saving?.value
        guard !Task.isCancelled, let saved = model.bots[bot.id], draft != BotDraft(saved), draft.isValid else { return }
        let save = Task {
            do {
                _ = try await model.save(draft.normalized)
                saveError = nil
            } catch {
                if open { saveError = error.localizedDescription } else { model.lastError = error.localizedDescription }
            }
        }
        saving = save
        await save.value
    }
}

/// The bot's agent, login and model.
private struct ModelSheet: View {
    let bot: Bot
    @Binding var draft: BotDraft
    let error: String?
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Model")
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    AgentModelPicker(backend: draft.backend, loginProfile: draft.loginProfile, selection: $draft.model, options: $draft.modelOptions,
                                     fastMode: Binding(get: { draft.fastMode ?? false }, set: { draft.fastMode = $0 }), bot: bot,
                                     agents: model.readySeats(on: draft.login),
                                     onAgentChange: { draft.setAgent($0) },
                                     restLine: bot.rest(of: draft.login).map { L10n.tr($0.line) }, expanded: true)
                    if let error { Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger) }
                }
                .padding(20)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .background(Palette.background)
    }
}
