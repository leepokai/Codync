import CodyncKit
import SwiftUI

/// The details sheet's pages, as on the desktop inspector.
enum DetailsTab: Hashable {
    case details, computer, library, memory, settings
}

/// What to open the details sheet on.
struct DetailsRequest: Identifiable {
    let id = UUID()
    var tab: DetailsTab = .details
    /// A routine to open in its editor (Details page).
    var routineId: String?
    /// Another bot's details than the chat's: a group member's, from its notice.
    var botId: String?

    /// Where a bot's fallbacks are set (the chat's "Add a fallback"): the Agent card, on Settings.
    static func fallbacks(of botId: String) -> DetailsRequest {
        DetailsRequest(tab: .settings, botId: botId)
    }
}

/// A bot's details (Grok Bot's): who it is, then what it does (Details), its computer, library, memory and how it
/// is set up (Settings). A Threads or Recent row pushes its task's thread chat (`ThreadChatView`) inside the sheet, with
/// Back and the system's swipe back.
/// Settings save as you edit.
struct BotDetailsView: View {
    let botId: String
    let routineId: String?
    /// Shows the card in the bot's chat that a task waits on; nil where the details aren't over that chat.
    let openCard: ((WorkTask) -> Void)?
    /// An artifact picked in the Library, for the chat's viewer.
    let openArtifact: (ArtifactTarget) -> Void
    @State var tab: DetailsTab
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var draft: BotDraft?
    @State private var saveError: String?
    /// Off once the page closes: a save that fails after that shows over the app instead.
    @State private var presence = PagePresence()
    @State private var saving: Task<Void, Never>?
    @State private var template: EditorRequest?
    /// The task whose thread chat is pushed over the pages.
    @State private var thread: WorkTask?

    init(botId: String, request: DetailsRequest, openCard: ((WorkTask) -> Void)? = nil, openArtifact: @escaping (ArtifactTarget) -> Void) {
        self.botId = botId
        self.openCard = openCard
        self.openArtifact = openArtifact
        routineId = request.routineId
        _tab = State(initialValue: request.tab)
    }

    private var bot: Bot? { model.bots[botId] }

    var body: some View {
        NavigationStack {
            pages
                .hidesSystemNavigationBar()
                .navigationDestination(item: $thread) { ThreadChatView(botId: botId, task: $0, showsDetails: false) }
        }
        // What the computer refused here (a message to a thread, a row's action) shows over this sheet, not under it
        // at the root, where the same error clears with this one.
        .codyncDialog("Something went wrong", isPresented: Binding(
            get: { model.lastError != nil }, set: { if !$0 { model.lastError = nil } }
        ), message: model.lastError, cancel: "OK", inPlace: true) { [] }
    }

    private var pages: some View {
        ScrollView {
            VStack(spacing: 0) {
                if let bot { identity(bot) }
                TabChoice(selection: $tab, options: [(.details, "Details"), (.computer, "Computer")]
                    + (model.hasLibrary ? [(.library, "Library")] : []) + [(.memory, "Memory"), (.settings, "Settings")])
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
        .onAppear { presence.shown = true }
        .onDisappear { presence.shown = false }
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
            if let bot {
                WorkTimeline(bot: bot, openCard: openCard, openThread: { thread = $0 }) {
                    RoutinesView(botId: botId, initialId: routineId)
                    if !bot.devServers.isEmpty { DevServersSection(bot: bot) }
                }
            }
        case .computer:
            BotComputerPage(botId: botId)
        case .library:
            LibraryView(botId: botId, openArtifact: openArtifact)
        case .memory:
            MemoryCard(botId: botId)
        case .settings:
            if let draft = Binding($draft) {
                BotSettingsForm(draft: draft, error: saveError, scrolls: false, inDetails: true)
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
                if presence.shown { saveError = error.localizedDescription } else { model.lastError = error.localizedDescription }
            }
        }
    }
}

/// Whether a page is on screen, for work that outlives it.
@MainActor private final class PagePresence {
    var shown = true
}
