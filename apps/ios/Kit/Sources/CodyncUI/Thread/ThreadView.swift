import CodyncKit
import SwiftUI
import UIKit

/// One endless conversation with a bot or a group chat. Only deliberate messages show
/// here; tool calls and thinking live in the "Full conversation" sheet. Any message can be
/// quoted in a reply.
public struct ThreadView: View {
    let botId: String

    public init(botId: String) { self.botId = botId }
    @Environment(BotStore.self) var model
    @State var showTrace = false
    /// The message the composer's next send quotes.
    @State var replyingTo: Entry?
    @State var botChat: BotChatTarget?
    @State var openArtifact: ArtifactTarget?
    @State var openCall: Entry?
    @State var editingGroup = false
    @State var details: DetailsRequest?
    /// A picture in the chat, full size.
    @State var viewingImage: ImageViewing?
    @State var calling = false
    @State var callSpeaking = false
    @State var interruptCall: (() -> Void)?
    /// The chat stays on the newest message until the reader scrolls away.
    @State var following = true
    @State var loadingEarlier = false
    /// Find in the conversation; nil while its bar is closed.
    @State var find: ConversationFind?
    /// Bumped to bring the shown match's message into view (a new text, a step).
    @State var findReveal = 0
    @Environment(\.accessibilityReduceMotion) var reduceMotion

    var bot: Bot? { model.bots[botId] }

    /// The messages, the composer and the find bar, without the platform's top chrome.
    private var chat: some View {
        let items = chatItems
        let matches = find.map { ConversationFind.matches(in: Self.findable(items), query: $0.query) } ?? []
        let shown = find?.index(in: matches).map { matches[$0] }
        return transcript(items, shown: shown)
        .background(Palette.background)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 6) {
                if let mismatch = model.mismatch {
                    // Nothing sent now could be read on the other side.
                    UpdateNeededCard(store: model, mismatch: mismatch)
                        .padding(.horizontal, 16)
                        .padding(.bottom, 8)
                        .transition(.opacity)
                } else {
                    Composer(botId: botId, replyingTo: $replyingTo,
                             queued: model.chat(botId).hidingCalls().queue(working: busy),
                             onCall: !calling && bot != nil ? { calling = true } : nil,
                             onInterrupt: callSpeaking ? interruptCall : nil)
                }
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: model.mismatch)
        }
        // Grok Bot's call: a bar floating over the chat, which stays readable and usable.
        .overlay(alignment: .top) {
            VStack(spacing: 8) {
                if calling {
                    CallView(botId: botId, isSpeaking: $callSpeaking, interrupt: $interruptCall) {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { calling = false }
                    }
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
                if let open = find {
                    // Not `Binding($find)`: its force unwrap crashes when the closing bar's binding updates once more.
                    let binding = Binding { find ?? open } set: { if find != nil { find = $0 } }
                    FindBar(find: binding, matches: matches, step: { stepFind($0, in: matches) }, close: { closeFind(in: matches) })
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .padding(.top, 4)
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: calling)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: find == nil)
        // A new text shows its first match; it counts as searched once typing pauses.
        .onChange(of: find?.query) { findReveal += 1 }
        // The first match found is held, so earlier messages with matches loading above don't move it.
        .onChange(of: find?.shown == nil && !matches.isEmpty) { find?.pin(matches) }
        .task(id: find?.query) {
            try? await Task.sleep(for: .milliseconds(700))
            if !Task.isCancelled { reportFind(found: !matches.isEmpty) }
        }
    }

    // MARK: find

    /// The text find searches: each message's own words, not separators, notices or exchanges.
    static func findable(_ items: [ChatItem]) -> [(id: String, text: String)] {
        items.compactMap { item in
            guard case let .entry(e, _) = item.kind, e.kind == "user" || e.kind == "agent", let text = e.data.text else { return nil }
            return (item.id, text)
        }
    }

    func openFind() {
        guard find == nil else { return }
        Analytics.shared.capture(.conversationFindOpened)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { find = ConversationFind() }
    }

    private func stepFind(_ forward: Bool, in matches: [ConversationFind.Match]) {
        find?.step(forward: forward, in: matches)
        findReveal += 1
        reportFind(found: !matches.isEmpty)
    }

    /// Closing clears the highlight with the bar.
    private func closeFind(in matches: [ConversationFind.Match]) {
        reportFind(found: !matches.isEmpty)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { find = nil }
    }

    private func reportFind(found: Bool) {
        if let event = find?.searched(found: found) { Analytics.shared.capture(event) }
    }

    var conversation: some View {
        chrome(chat)
            .readingConversation(botId)
            .onChange(of: model.voiceCallRequest, initial: true) { _, _ in
                if model.takeVoiceCallRequest(for: botId) { startShortcutCall() }
            }
            // The details sheet's Library and the picture viewer present their own downloads.
            .fileExportSheet(model, botId: botId, active: details == nil && viewingImage == nil)
            .imageViewer($viewingImage)
            .codyncSheet(isPresented: $showTrace) {
                TraceView(botId: botId)
            }
            .codyncSheet(item: $botChat) { BotConversationView(botId: botId, peerId: $0.id) }
            .codyncSheet(item: $openArtifact) { ArtifactView(target: $0) }
            .codyncSheet(item: $openCall) { CallConversationView(line: $0) }
            .codyncSheet(item: $details) { request in
                BotDetailsView(botId: request.botId ?? botId, request: request) { artifact in
                    details = nil
                    openArtifact = artifact
                }
            }
            .codyncSheet(isPresented: $editingGroup) {
                if let bot { GroupEditorView(group: bot) }
            }
    }

    /// Open Live Mode from Shortcuts: closes whatever covers the chat, then starts the call.
    private func startShortcutCall() {
        showTrace = false
        botChat = nil
        openArtifact = nil
        editingGroup = false
        details = nil
        viewingImage = nil
        if model.fileDownloads.export?.botId == botId { model.fileDownloads.dismissExport() }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { calling = true }
    }

    // MARK: rows

    @ViewBuilder func row(_ item: ChatItem) -> some View {
        switch item.kind {
        case .separator(let date):
            TimeSeparator(date: date)
        case .exchanges(let group):
            BotMessageRow(group: group) { botChat = BotChatTarget(id: group.peerId) }
        case .entry(let e, let groupStart):
            if e.kind == "notice", let id = e.data.routineId {
                RoutineNoticeRow(entry: e) { openRoutine(id) }
            } else {
                ChatRow(entry: e, groupStart: groupStart, chat: bot, openTrace: { showTrace = true },
                        // An update card takes the composer's place, so there's nothing to reply with.
                        reply: e.isReplyable && model.mismatch == nil ? { reply(to: e) } : nil,
                        // In a group, the bot whose agent failed.
                        addFallback: { details = DetailsRequest(botId: e.data.author ?? e.botId) },
                        openArtifact: { openArtifact = ArtifactTarget(botId: e.botId, id: $0.id, title: $0.title) },
                        openCall: { openCall = e })
                    .equatable()
            }
        }
    }

    private func reply(to entry: Entry) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { replyingTo = entry }
    }

    // MARK: chrome

    var connectionSubtitle: some View {
        HStack(spacing: 6) {
            Group {
                if model.shownConnection == .connecting {
                    Spinner(size: 8)
                } else {
                    Image(systemName: connectionSymbol).font(.system(size: 8, weight: .medium))
                }
            }
            .frame(width: 12)
            .accessibilityHidden(true)
            HStack(spacing: 3) {
                Text(L10n.tr(model.connectionLabel))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let devServerLabel {
                    Text("·")
                    if devServerSummary == .starting { Spinner(size: 8) }
                    Text(devServerLabel)
                        .monospacedDigit()
                        .lineLimit(1)
                        .foregroundStyle(devServerSummary == .failed ? Palette.danger : Palette.secondary)
                }
            }
        }
        .font(.system(size: 10, weight: .medium))
        .foregroundStyle(Palette.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([model.hostName, L10n.tr(connectionDescription), devServerLabel].compactMap(\.self).joined(separator: ", "))
        .help("\(model.hostName) · \(L10n.tr(connectionDescription))")
    }

    private var devServerSummary: DevServerSummary { DevServerSummary(bot?.devServers ?? []) }

    private var devServerLabel: String? {
        switch devServerSummary {
        case .none: nil
        case .starting: L10n.tr("Starting")
        case .failed: L10n.tr("Failed to start")
        case let .running(text): text
        }
    }

    private var connectionSymbol: String {
        guard model.connection == .online else { return "wifi.slash" }
        switch model.hostRoute {
        case .relay: return "cloud"
        case .direct: return "wifi"
        case nil: return "network"
        }
    }

    private var connectionDescription: String {
        if model.isAsleep { return "Sleeping · wakes when you message" }
        switch model.shownConnection {
        case .connecting: return "Connecting"
        case .computerOffline, .offline: return "Offline"
        case .unauthorized: return "No access"
        case .unpaired: return "Not paired"
        case .online: break
        }
        if model.isCloudUpdating { return "Updating…" }
        if model.mismatch != nil { return "Needs update" }
        switch model.hostRoute {
        case .relay: return RelayCopy.connected(cloud: model.computer.cloud)
        case .direct: return "Connected over Wi-Fi or Tailscale"
        case nil: return "Connected"
        }
    }

    var title: some View {
        HStack(spacing: 7) {
            if let bot {
                let size: CGFloat = 22
                if bot.isGroup { GroupAvatar(members: model.members(of: bot), size: size) } else { CharacterAvatar(bot: bot, size: size) }
            }
            Text(bot?.name ?? "")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
        }
    }

    func openRoutine(_ id: String) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
            details = DetailsRequest(tab: .details, routineId: id)
        }
    }
}

// MARK: - Chat model

struct ChatItem: Identifiable {
    enum Kind {
        case separator(Date)
        case entry(Entry, groupStart: Bool)
        /// Main chat only: consecutive bot-to-bot exchanges with one peer.
        case exchanges(BotExchangeGroup)
    }

    let id: String
    let kind: Kind

    /// Chat-visible entries plus time separators (gaps > 1 h) and author grouping. A bot's
    /// messages arrive whole (Grok Bot's `send_message`); what it writes along the way is trace. Runs of
    /// bot exchanges with one peer fold into one item.
    static func build(_ entries: [Entry]) -> [ChatItem] {
        var out: [ChatItem] = []
        var lastDate: Date?
        var lastAuthor: String?
        for slot in BotExchangeGroup.collapse(entries) {
            let date = slot.date
            // Bot-originated messages carry an empty nonce. Their entry IDs
            // distinguish them; only real nonces identify optimistic echoes.
            let id = if case .entry(let e) = slot, e.kind == "user", let nonce = e.data.clientNonce, !nonce.isEmpty { "user-\(nonce)" } else { slot.id }
            if lastDate.map({ date.timeIntervalSince($0) > 3600 }) ?? true {
                out.append(ChatItem(id: "sep-\(id)", kind: .separator(date)))
                lastAuthor = nil
            }
            lastDate = date
            switch slot {
            case .exchanges(let group):
                out.append(ChatItem(id: id, kind: .exchanges(group)))
                lastAuthor = nil
            case .entry(let e):
                // In a group each bot is its own author.
                let author = e.kind == "user" ? "user" : e.kind == "agent" ? "agent:\(e.data.author ?? "")" : e.kind
                out.append(ChatItem(id: id, kind: .entry(e, groupStart: author != lastAuthor)))
                lastAuthor = (author == "user" || e.kind == "agent") ? author : nil
            }
        }
        return out
    }

    var isUserMessage: Bool {
        if case let .entry(entry, _) = kind { entry.kind == "user" } else { false }
    }

}

/// The start of an empty group chat: who's in it and how the room works.
struct GroupIntroCard: View {
    let group: Bot
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            GroupAvatar(members: model.members(of: group), size: 72)
            Text(group.name).font(.title2.weight(.semibold)).foregroundStyle(Palette.text)
            let names = model.members(of: group).map(\.name)
            if group.name != names.joined(separator: ", ") {
                Text(names.joined(separator: " · "))
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
            }
            if !group.description.isEmpty {
                Text(group.description)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
            }
            Text(L10n.tr("Everyone answers in turn. @mention a bot to ask just that one."))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
    }
}

struct IntroCard: View {
    let bot: Bot
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            CharacterAvatar(bot: bot, size: 72)
            Text(bot.name).font(.title2.weight(.semibold)).foregroundStyle(Palette.text)
            Text(bot.managedWorkspace
                 ? "\(model.backendName(bot.backend)) · \(L10n.tr("Personal workspace"))"
                 : L10n.fill(L10n.tr("{agent} in {path}"), ["agent": model.backendName(bot.backend), "path": bot.cwd]))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            if !bot.description.isEmpty {
                Text(bot.description)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
            }
            Text(L10n.tr("Tell it what you need. You'll get a notification when it's done or needs you."))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
    }
}

extension ThreadView {
    public var body: some View {
        conversation
            .tracked(.screenViewed(.chat))
    }

    private var working: Bool { bot?.isWorking(in: botId) == true && !model.isOffline }
    /// The bot works anywhere (a room, a routine): the user's messages here wait above the composer.
    private var busy: Bool { bot?.isWorking == true }

    /// iPhone's messages: all this device has, earlier ones from the computer as the reader
    /// scrolls up; the chat follows the newest message until they scroll away, and a reply is
    /// revealed steadily as it's written.
    /// What voice calls carried shows in each call's conversation, not in the chat.
    /// Messages waiting behind the bot's work show above the composer instead.
    var chatItems: [ChatItem] {
        ChatItem.build(model.chat(botId).hidingCalls().hidingQueue(working: busy))
    }

    /// `shown`: the find's current match, whose message is highlighted and brought into view.
    func transcript(_ items: [ChatItem], shown: ConversationFind.Match?) -> some View {
        var rows: [ConversationRow] = []
        if moreOnComputer {
            rows.append(ConversationRow("earlier") {
                Spinner(size: 14)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .accessibilityLabel("Loading earlier messages")
                    // A chat shorter than the screen never scrolls, so showing the row is the trigger.
                    .onAppear { showEarlier() }
            })
        }
        if items.isEmpty, let bot {
            rows.append(ConversationRow("intro") {
                if bot.isGroup { GroupIntroCard(group: bot).padding(.top, 40) } else { IntroCard(bot: bot).padding(.top, 40) }
            })
        }
        rows += items.map { item in
            ConversationRow(item.id, isUserMessage: item.isUserMessage) {
                row(item)
                    .background(shown?.id == item.id ? Palette.attention.opacity(0.18) : .clear)
                    .accessibilityAddTraits(shown?.id == item.id ? .isSelected : [])
            }
        }
        // Offline, "working" is only what the computer last said; don't show it as live.
        if let bot, working {
            rows.append(ConversationRow("working") {
                // Each turn starts folded.
                WorkingIndicator(bot: bot, steps: Entry.workingSteps(model.chat(botId)))
                    .id(bot.startedAt)
                    .padding(.top, 6)
            })
        }
        // Pull to refresh reconnects, like Reconnect in the computer menu, until the link is back.
        let reveal = shown.map { ConversationReveal(id: $0.id, step: findReveal) }
        return ConversationList(rows: rows, following: $following, reveal: reveal, nearTop: { showEarlier() }, refresh: {
            model.restartStream()
            _ = try? await model.ready()
        })
    }

    private var moreOnComputer: Bool {
        model.canLoadOlder(botId)
    }

    /// The reader scrolled up to the top: fetch earlier messages from the computer.
    private func showEarlier() {
        guard moreOnComputer, !loadingEarlier else { return }
        loadingEarlier = true
        Task {
            await model.loadOlder(botId)
            loadingEarlier = false
        }
    }

    /// The system navigation bar: the title opens the bot's details; the corner is the computer.
    func chrome(_ content: some View) -> some View {
        content
            .navigationBarTitleDisplayMode(.inline)
            .toolbar(.visible, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 0) {
                        header
                        // The port shows here, so a tap on it opens the details too.
                        Button(action: openDetails) { connectionSubtitle }
                            .buttonStyle(.plain)
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button(L10n.tr("Find in conversation"), systemImage: "magnifyingglass", action: openFind)
                }
                if model.screen != nil {
                    ToolbarItem(placement: .topBarTrailing) { computerButton }
                }
            }
    }

    /// Like Grok Bot: the bot at the top opens its details (a group's editor for a group).
    var header: some View {
        Button(action: openDetails) { title }
            .accessibilityLabel(L10n.tr("{name}, view details").replacingOccurrences(of: "{name}", with: bot?.name ?? ""))
    }

    private func openDetails() {
        if bot?.isGroup == true { editingGroup = true } else { details = DetailsRequest() }
    }

    /// Opens the computer's screen; pulses while this bot is operating it (watch, then take over).
    private var computerButton: some View {
        let operating = model.screen?.agentBot == botId
        return Button(L10n.tr("Computer"), systemImage: "desktopcomputer") {
            model.screenRequest = operating ? ScreenRequest(watching: botId) : ScreenRequest()
        }
        .symbolEffect(.pulse, options: .repeating, isActive: operating)
    }
}
