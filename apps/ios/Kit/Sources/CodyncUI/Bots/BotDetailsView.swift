import CodyncKit
import SwiftUI

/// The details sheet's pages, as on the desktop inspector.
enum DetailsTab: Hashable {
    case details, library, memory, settings
}

/// What to open the details sheet on.
struct DetailsRequest: Identifiable {
    let id = UUID()
    var tab: DetailsTab = .details
    /// A routine to open in its editor (Details page).
    var routineId: String?
    /// Another bot's details than the chat's: a group member's, from its notice.
    var botId: String?
}

/// A bot's details (Grok Bot's): who it is, then its details, library, memory and settings. Settings save
/// as you edit.
struct BotDetailsView: View {
    let botId: String
    let routineId: String?
    /// An artifact picked in the Library, for the chat's viewer.
    let openArtifact: (ArtifactTarget) -> Void
    @State var tab: DetailsTab
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var draft: BotDraft?
    @State private var saveError: String?
    @State private var saving: Task<Void, Never>?
    @State private var template: EditorRequest?

    init(botId: String, request: DetailsRequest, openArtifact: @escaping (ArtifactTarget) -> Void) {
        self.botId = botId
        self.openArtifact = openArtifact
        routineId = request.routineId
        _tab = State(initialValue: request.tab)
    }

    private var bot: Bot? { model.bots[botId] }

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                if let bot { identity(bot) }
                TabChoice(selection: $tab, options: [(.details, "Details")] + (model.hasLibrary ? [(.library, "Library")] : [])
                    + [(.memory, "Memory"), (.settings, "Settings")])
                    .padding(.top, 18)
                page.padding(.top, 20)
                    .environment(\.cardStyle, .outlined)
                    .environment(\.noteLines, 2)
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 24)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .overlay(alignment: .topTrailing) { actions }
        .tracked(.screenViewed(.botDetails))
        .task(id: botId) { draft = bot.map(BotDraft.init) }
        // A change made elsewhere (a bot that manages the team, another device) replaces each field not edited here.
        .onChange(of: bot.map(BotDraft.init)) { base, fresh in
            if let base, let fresh, base.id == fresh.id { draft = draft?.rebased(from: base, onto: fresh) }
        }
        .onChange(of: draft) { _, draft in autosave(draft) }
        .codyncSheet(item: $template) { BotTemplateView(draft: $0.draft) }
    }

    private var actions: some View {
        HStack(spacing: 8) {
            if let bot {
                cornerButton("Create template", "square.and.arrow.up") { template = EditorRequest(BotDraft(bot)) }
            }
            cornerButton("Close details", "xmark") { dismiss() }
        }
        .padding(.top, 14)
        .padding(.trailing, 14)
    }

    /// Grok Bot's corner buttons: the page color, ringed by a soft shadow.
    private func cornerButton(_ title: String, _ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(Palette.text)
                .frame(width: 36, height: 36)
                .background(Palette.background, in: Circle())
                .shadow(color: .black.opacity(0.12), radius: 0.5)
                .shadow(color: .black.opacity(0.04), radius: 2, y: 1)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .help(title)
    }

    /// The bot's avatar and name, and what runs it.
    private func identity(_ bot: Bot) -> some View {
        VStack(spacing: 4) {
            LeadAgentAvatar(bot: bot)
                .padding(.bottom, 10)
            Text(bot.name)
                .font(.title3.weight(.semibold))
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.center)
            HStack(spacing: 5) {
                AgentIcon(registry: model.agentRegistry(bot.backend), size: 13)
                Text(bot.model.map { "\(model.backendName(bot.backend)) · \($0)" } ?? model.backendName(bot.backend))
            }
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 64)
    }

    @ViewBuilder private var page: some View {
        switch tab {
        case .details:
            VStack(alignment: .leading, spacing: 28) {
                if let draft = Binding($draft) {
                    BotSettingsForm(draft: draft, error: saveError, scrolls: false, part: .often)
                }
                if let screen = model.screen, let bot {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.tr("Computer"))
                            .font(.system(size: 15, weight: .semibold))
                            .foregroundStyle(Palette.text)
                            .padding(.leading, 2)
                        SwitchRow("Use the computer",
                                  detail: screen.controlReady == true ? nil : "On that computer, open Codync → Settings → Computer access to set it up first.",
                                  isOn: Binding(get: { bot.computer }, set: { model.setComputer(bot, $0) }))
                            .disabled(model.isOffline)
                            .outlinedCard()
                    }
                }
                RoutinesView(botId: botId, initialId: routineId) { dismiss() }
            }
        case .library:
            LibraryView(botId: botId, openArtifact: openArtifact)
        case .memory:
            MemoryCard(botId: botId)
        case .settings:
            if let draft = Binding($draft) {
                BotSettingsForm(draft: draft, error: saveError, scrolls: false, part: .rarely)
            }
        }
    }

    /// Saves a moment after the last change, like the desktop inspector.
    private func autosave(_ draft: BotDraft?) {
        saving?.cancel()
        guard let draft, let bot, draft != BotDraft(bot), draft.isValid else { return }
        saving = Task {
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }
            do {
                _ = try await model.save(draft.normalized)
                saveError = nil
            } catch {
                saveError = error.localizedDescription
            }
        }
    }
}
