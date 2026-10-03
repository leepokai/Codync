import CodyncKit
import SwiftUI
#if os(iOS)
    import UIKit
#endif

/// One endless conversation with a bot or a group chat. Only deliberate messages show
/// here; tool calls and thinking live in the "Full conversation" sheet. Any message can
/// start a thread, which opens beside the chat (Mac) or over it (iPhone).
public struct ThreadView: View {
    let botId: String

    public init(botId: String) { self.botId = botId }
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var showTrace = false
    @State private var openThread: ThreadTarget?
    @State private var editingGroup = false
    @State private var editing: EditorRequest?
    @State private var templateRequest: EditorRequest?
    @State private var confirmNewSession = false
    @State private var confirmDelete: Bot?
    /// Desktop: the bot's settings as an inspector beside the chat.
    @State private var showSettings = true
    @State private var editingDetails = false
    @State private var availableWidth: CGFloat = 800
    @State private var compactDetails = false
    @State private var calling = false
    @State private var callSpeaking = false
    @State private var interruptCall: (() -> Void)?
    @State private var showRoutines = false
    @State private var routineId: String?
    @State private var routineRequest = UUID()
    @State private var isAtBottom = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    #if os(macOS)
        @Environment(\.conversationTypography) private var typography
    #endif

    private var bot: Bot? { model.bots[botId] }

    public var body: some View {
        #if os(macOS)
            GeometryReader { geometry in
                HStack(spacing: 0) {
                    conversation.frame(maxWidth: .infinity)
                    if let openThread, geometry.size.width >= 680 {
                        HStack(spacing: 0) {
                            Rectangle().fill(Palette.border).frame(width: 1)
                            RepliesView(botId: botId, rootId: openThread.id) { closeThread() }
                                .frame(width: min(420, geometry.size.width * 0.45))
                        }
                        .id(openThread.id)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                    } else if showSettings && geometry.size.width >= 680 {
                        HStack(spacing: 0) {
                            Rectangle().fill(Palette.border).frame(width: 1)
                            detailsPanel.frame(width: 292)
                        }
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                    }
                }
                .clipped()
                .onGeometryChange(for: CGFloat.self) {
                    $0.size.width
                } action: {
                    availableWidth = $0
                }
            }
            .ignoresSafeArea(.container, edges: .top)
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: openThread)
            .codyncSheet(isPresented: $compactDetails) {
                detailsPanel.frame(width: 340, height: 600)
            }
            .codyncSheet(isPresented: Binding(get: { openThread != nil && availableWidth < 680 }, set: { if !$0 { openThread = nil } })) {
                if let openThread {
                    RepliesView(botId: botId, rootId: openThread.id) { closeThread() }.frame(width: 440, height: 600)
                }
            }
        #else
            conversation
                .codyncSheet(item: $openThread) { target in
                    RepliesView(botId: botId, rootId: target.id) { closeThread() }
                }
        #endif
    }

    private func closeThread() {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { openThread = nil }
    }

    @ViewBuilder private var conversation: some View {
        let thread = model.chat(botId)
        let items = ChatItem.build(thread, streaming: bot?.isWorking(in: botId, thread: nil) == true && !model.isOffline)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if !model.historyComplete.contains(botId), thread.count >= 50 {
                        Button("Load earlier messages") { Task { await model.loadOlder(botId) } }
                            .buttonStyle(.plain)
                            .appFont(.footnote)
                            .foregroundStyle(Palette.secondary)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                    }
                    if items.isEmpty, let bot {
                        if bot.isGroup { GroupIntroCard(group: bot).padding(.top, 40) } else { IntroCard(bot: bot).padding(.top, 40) }
                    }
                    ForEach(items) { item in
                        row(item)
                            .id(item.id)
                            .transition(item.id == items.last?.id
                                ? .asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity)
                                : .identity)
                    }
                    // Offline, "working" is only what the computer last said; don't show it as live.
                    if let bot, bot.isWorking(in: botId, thread: nil), !model.isOffline {
                        WorkingIndicator(bot: bot, thinking: model.currentThinking(botId, thread: nil))
                            .padding(.top, 6)
                            .id("working")
                    }
                    Color.clear.frame(height: 8).id("bottom")
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .animation(Motion.reduced(Motion.conversation, reduceMotion), value: items.last?.id)
                #if os(macOS)
                    .frame(maxWidth: 820)
                    .frame(maxWidth: .infinity)
                #endif
            }
            .conversationInitialBottomAnchor()
            .scrollDismissesKeyboard(.interactively)
            .conversationScrollEdges()
            .conversationBottomObserver($isAtBottom)
            #if os(iOS)
                .simultaneousGesture(TapGesture().onEnded { dismissChatKeyboard() })
            #endif
            .onChange(of: items.last?.id) { _, _ in
                guard isAtBottom || items.last?.isUserMessage == true else { return }
                withAnimation(Motion.reduced(Motion.conversation, reduceMotion)) {
                    proxy.scrollTo("bottom", anchor: .bottom)
                }
            }
            .onChange(of: items.last?.textContent) { _, _ in
                guard isAtBottom else { return }
                withAnimation(Motion.reduced(Motion.conversation, reduceMotion)) {
                    proxy.scrollTo("bottom", anchor: .bottom)
                }
            }
            .onChange(of: bot?.isWorking) { _, _ in
                guard isAtBottom else { return }
                withAnimation(Motion.reduced(Motion.conversation, reduceMotion)) {
                    proxy.scrollTo("bottom", anchor: .bottom)
                }
            }
            #if os(macOS)
                .onChange(of: typography.pointSize) { _, _ in
                    guard isAtBottom else { return }
                    proxy.scrollTo("bottom", anchor: .bottom)
                }
            #endif
        }
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
                    #if os(iOS)
                        Composer(botId: botId,
                                 onCall: !calling && bot?.isGroup == false ? { calling = true } : nil,
                                 onInterrupt: callSpeaking ? interruptCall : nil)
                    #else
                        Composer(botId: botId)
                    #endif
                }
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: model.mismatch)
        }
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar(.visible, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 0) {
                        header
                        connectionSubtitle
                    }
                }
                if model.screen != nil {
                    ToolbarItem(placement: .topBarTrailing) { computerButton }
                }
            }
            // Grok Bot's call: a bar floating over the chat, which stays readable and usable.
            .overlay(alignment: .top) {
                if calling {
                    CallView(botId: botId, isSpeaking: $callSpeaking, interrupt: $interruptCall) {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { calling = false }
                    }
                        .padding(.top, 4)
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: calling)
        #endif
        #if os(macOS)
            .safeAreaInset(edge: .top, spacing: 0) {
                VStack(spacing: 0) {
                    HStack {
                        header
                        connectionSubtitle
                        Spacer(minLength: 8)
                        if let bot, !bot.isGroup {
                            IconButton("Create template", systemImage: "square.and.arrow.up") {
                                templateRequest = EditorRequest(BotDraft(bot))
                            }
                        }
                        if !showSettings || availableWidth < 680 {
                            IconButton("Conversation details", systemImage: "chevron.right.2") { toggleDetails() }
                                .keyboardShortcut("i", modifiers: [.command, .option])
                                .transition(.opacity)
                        }
                    }
                    .padding(.horizontal, 16)
                    .frame(height: 44)
                    Rectangle().fill(Palette.border).frame(height: 0.5)
                }
                .background(Palette.background)
            }
        #endif
        .readingConversation(botId)
        .codyncSheet(isPresented: $showTrace) {
            TraceView(botId: botId)
                #if os(macOS)
                    .frame(width: 620, height: 560)
                #endif
        }
        .codyncSheet(item: $templateRequest) { request in
            BotTemplateView(draft: request.draft)
        }
        .codyncSheet(item: $editing) { request in
            BotEditorView(draft: request.draft)
        }
        .codyncSheet(isPresented: $editingGroup) {
            GroupEditorView(group: bot)
                #if os(macOS)
                    .frame(width: 420, height: 560)
                #endif
        }
        .codyncDialog("Start a new session?", isPresented: $confirmNewSession,
                      message: "The conversation stays here, but the agent starts with a fresh context.") {
            [DialogAction("New session") { model.newSession(botId) }]
        }
        .codyncSheet(isPresented: $showRoutines) {
            ScrollView {
                RoutinesView(botId: botId, initialId: routineId) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showRoutines = false }
                }.padding(20)
            }
            #if os(macOS)
                .frame(width: 440, height: 580)
            #endif
        }
        .deleteBotConfirmation($confirmDelete) { dismiss() }
    }

    #if os(iOS)
        private func dismissChatKeyboard() {
            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
        }
    #endif

    // MARK: rows

    @ViewBuilder private func row(_ item: ChatItem) -> some View {
        switch item.kind {
        case .separator(let date):
            Text(RelativeTime.separator(date))
                .appFont(.footnote)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
                .padding(.top, 18)
                .padding(.bottom, 6)
        case .entry(let e, let groupStart):
            if e.kind == "notice", let id = e.data.routineId {
                Button {
                    openRoutine(id)
                } label: {
                    Label(e.data.text ?? "Routine", systemImage: "clock.arrow.circlepath")
                        .appFont(.footnote).foregroundStyle(Palette.secondary).padding(.vertical, 10)
                }.buttonStyle(.plain)
            } else {
                ChatRow(entry: e, groupStart: groupStart, chat: bot) {
                    showTrace = true
                } openThread: { root in
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { openThread = ThreadTarget(id: root.id) }
                }
            }
        }
    }

    // MARK: chrome

    private var connectionSubtitle: some View {
        HStack(spacing: 6) {
            Group {
                if model.shownConnection == .connecting {
                    Spinner(size: 8)
                } else {
                    Image(systemName: connectionSymbol).appFont(.system(size: 8, weight: .medium))
                }
            }
            .frame(width: 12)
            .accessibilityHidden(true)
            Text(model.connectionLabel)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .appFont(.system(size: 10, weight: .medium))
        .foregroundStyle(Palette.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(model.hostName), \(connectionDescription)")
        .help("\(model.hostName) · \(connectionDescription)")
    }

    private var connectionSymbol: String {
        guard model.connection == .online else { return "wifi.slash" }
        switch model.hostRoute {
        case .relay: return "cloud"
        case .direct: return "wifi"
        case .loopback: return "desktopcomputer"
        case nil: return "network"
        }
    }

    private var connectionDescription: String {
        switch model.shownConnection {
        case .connecting: return "Connecting"
        case .computerOffline, .offline: return "Offline"
        case .unauthorized: return "No access"
        case .unpaired: return "Not paired"
        case .online: break
        }
        if model.mismatch != nil { return "Needs update" }
        switch model.hostRoute {
        case .relay: return "Connected through Cloudflare"
        case .direct: return "Connected over Wi-Fi or Tailscale"
        case .loopback: return "Connected locally"
        case nil: return "Connected"
        }
    }

    private var header: some View {
        #if os(iOS)
            // Like Grok Bot: the title pill holds the bot's actions; the corner is the computer.
            Menu {
                Text("\(model.hostName) · \(model.connectionLabel)")
                Button("Reconnect", systemImage: "arrow.clockwise") { model.restartStream() }
                Divider()
                Button("Details", systemImage: "info.circle", action: openDetails)
                ForEach(menuItems) { item in
                    if item.divider { Divider() }
                    Button(role: item.destructive ? .destructive : nil, action: item.action) {
                        if let icon = item.icon { Label(item.title, systemImage: icon) } else { Text(item.title) }
                    }
                }
            } label: {
                title
            }
            .accessibilityLabel("\(bot?.name ?? "Conversation") actions")
        #else
            Button(action: toggleDetails) { title }
                .buttonStyle(.plain)
                .accessibilityLabel("View conversation details")
                .help("View conversation details")
        #endif
    }

    #if os(iOS)
        private func openDetails() {
            if bot?.isGroup == true { editingGroup = true } else if let bot { editing = EditorRequest(BotDraft(bot)) }
        }

        /// Opens the computer's screen; pulses while this bot is operating it (watch, then take over).
        private var computerButton: some View {
            let operating = model.screen?.agentBot == botId
            return Button("Computer", systemImage: "desktopcomputer") {
                model.screenRequest = operating ? ScreenRequest(watching: botId) : ScreenRequest()
            }
            .symbolEffect(.pulse, options: .repeating, isActive: operating)
        }
    #endif

    private var title: some View {
        HStack(spacing: 7) {
            if let bot {
                if bot.isGroup { GroupAvatar(members: model.members(of: bot), size: 22) } else { CharacterAvatar(bot: bot, size: 22) }
            }
            Text(bot?.name ?? "")
                .appFont(.system(size: 13, weight: .semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
        }
    }

    #if os(macOS)
        private func toggleDetails() {
            if availableWidth < 680 {
                compactDetails.toggle()
            } else {
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showSettings.toggle() }
            }
        }

        /// Its own view so it stays live inside the compact modal (which captures its content).
        private var detailsPanel: some View {
            DetailsPanel(botId: botId, routineId: routineId, editing: $editingDetails, editGroup: { editingGroup = true }) {
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showSettings = false }
                compactDetails = false
            }
            .id(routineRequest)
        }
    #endif

    private func openRoutine(_ id: String?) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
            routineId = id
            routineRequest = UUID()
            #if os(macOS)
                editingDetails = false
                openThread = nil
                showSettings = true
                if availableWidth < 680 { compactDetails = true }
            #else
                showRoutines = true
            #endif
        }
    }

    private var menuItems: [MenuItem] {
        var items = [MenuItem("Full conversation", icon: "list.bullet.rectangle") { showTrace = true }]
        if let bot, bot.isGroup {
            items.append(MenuItem("Edit group", icon: "person.2") { editingGroup = true })
            items.append(MenuItem(bot.pinned ? "Unpin" : "Pin", icon: "pin") { model.setPinned(bot, !bot.pinned) })
            items.append(MenuItem("Delete group", icon: "trash", destructive: true, divider: true) { confirmDelete = bot })
            return items
        }
        if let bot {
            items.append(MenuItem("Edit profile", icon: "pencil") {
                #if os(macOS)
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        editingDetails = true
                        showSettings = true
                    }
                    if availableWidth < 680 { compactDetails = true }
                #else
                    editing = EditorRequest(BotDraft(bot))
                #endif
            })
            items.append(MenuItem(bot.pinned ? "Unpin" : "Pin", icon: "pin") { model.setPinned(bot, !bot.pinned) })
        }
        items.append(MenuItem("Routines", icon: "clock.arrow.circlepath") {
            openRoutine(nil)
        })
        items.append(MenuItem("New session", icon: "arrow.counterclockwise") { confirmNewSession = true })
        items.append(MenuItem("Delete bot", icon: "trash", destructive: true, divider: true) { confirmDelete = bot })
        return items
    }
}

// MARK: - Chat model

struct ChatItem: Identifiable {
    enum Kind {
        case separator(Date)
        case entry(Entry, groupStart: Bool)
    }

    let id: String
    let kind: Kind

    /// Chat-visible entries plus time separators (gaps > 1 h) and author grouping.
    /// `streaming`: a turn is running in this chat, so the text being generated right now
    /// (the lane's last entry, still `final == false`) shows in place and never pops in later.
    /// Earlier segments of the turn, tool calls in between, and a room pass stay trace-only.
    static func build(_ entries: [Entry], streaming: Bool = false) -> [ChatItem] {
        var out: [ChatItem] = []
        var lastDate: Date?
        var lastAuthor: String?
        let live = streaming ? entries.last.flatMap { $0.kind == "agent" && $0.data.final == false ? $0.id : nil } : nil
        for e in entries where e.isChat || (e.id == live && !(e.data.text ?? "").isEmpty && e.data.text != "(pass)") {
            let date = e.date
            // Bot-originated messages carry an empty nonce. Their entry IDs
            // distinguish them; only real nonces identify optimistic echoes.
            let id: String
            if e.kind == "user", let nonce = e.data.clientNonce, !nonce.isEmpty {
                id = "user-\(nonce)"
            } else {
                id = e.id
            }
            if lastDate.map({ date.timeIntervalSince($0) > 3600 }) ?? true {
                out.append(ChatItem(id: "sep-\(id)", kind: .separator(date)))
                lastAuthor = nil
            }
            // In a group each bot is its own author.
            let author = e.kind == "user" ? "user" : e.kind == "agent" ? "agent:\(e.data.author ?? "")" : e.kind
            out.append(ChatItem(id: id, kind: .entry(e, groupStart: author != lastAuthor)))
            lastAuthor = (author == "user" || e.kind == "agent") ? author : nil
            lastDate = date
        }
        return out
    }

    var isUserMessage: Bool {
        if case let .entry(entry, _) = kind { entry.kind == "user" } else { false }
    }

    var textContent: String? {
        if case let .entry(entry, _) = kind { entry.data.text } else { nil }
    }
}

/// The start of an empty group chat: who's in it and how the room works.
private struct GroupIntroCard: View {
    let group: Bot
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            GroupAvatar(members: model.members(of: group), size: 72)
            Text(group.name).appFont(.title2.weight(.semibold)).foregroundStyle(Palette.text)
            Text(model.members(of: group).map(\.name).joined(separator: " · "))
                .appFont(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            if !group.description.isEmpty {
                Text(group.description)
                    .appFont(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
            }
            Text("Everyone answers in turn. @mention a bot to ask just that one. Each bot works in its own folder.")
                .appFont(.footnote)
                .foregroundStyle(Palette.tertiary)
                .multilineTextAlignment(.center)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
    }
}

#if os(macOS)
/// A group's bots in the Mac inspector; clicking one opens its own chat.
private struct GroupMembersList: View {
    let group: Bot
    @Environment(BotStore.self) private var model

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                Text("\(group.members.count) bots").appFont(.system(size: 13, weight: .semibold)).padding(.bottom, 6)
                ForEach(model.members(of: group)) { bot in
                    Button { model.selection = bot.id } label: {
                        HStack(spacing: 10) {
                            CharacterAvatar(bot: bot, size: 26)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(bot.name).appFont(.system(size: 13)).foregroundStyle(Palette.text)
                                Text(bot.isWorking ? (bot.activity.isEmpty ? "Working…" : bot.activity) : bot.folderName)
                                    .appFont(.system(size: 11)).foregroundStyle(Palette.tertiary).lineLimit(1)
                            }
                            Spacer()
                        }
                        .padding(.vertical, 6)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(PressScale())
                    .help("Open \(bot.name)'s own chat")
                }
            }
            .padding(.horizontal, 16)
        }
    }
}
#endif

private struct IntroCard: View {
    let bot: Bot
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            CharacterAvatar(bot: bot, size: 72)
            Text(bot.name).appFont(.title2.weight(.semibold)).foregroundStyle(Palette.text)
            Text(bot.managedWorkspace ? "\(model.backendName(bot.backend)) · Personal workspace" : "\(model.backendName(bot.backend)) in \(bot.cwd)")
                .appFont(.footnote.monospaced())
                .foregroundStyle(Palette.tertiary)
                .multilineTextAlignment(.center)
            if !bot.description.isEmpty {
                Text(bot.description)
                    .appFont(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
            }
            Text("Tell it what you need. You'll get a notification when it's done or needs you.")
                .appFont(.footnote)
                .foregroundStyle(Palette.tertiary)
                .multilineTextAlignment(.center)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
    }
}

#if os(macOS)
/// The Mac inspector beside a conversation: the computer, the agent, and the bot's settings.
private struct DetailsPanel: View {
    let botId: String
    let routineId: String?
    @Binding var editing: Bool
    let editGroup: () -> Void
    let close: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var bot: Bot? { model.bots[botId] }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                if editing {
                    IconButton("Back to details", systemImage: "chevron.left") { setEditing(false) }
                    Text("Settings").appFont(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.text)
                    Spacer()
                } else if bot?.isGroup == true {
                    panelTitle("Members")
                    Spacer()
                    IconButton("Edit group", systemImage: "gearshape", action: editGroup)
                } else {
                    panelTitle("Details")
                    Spacer()
                    IconButton("Bot settings", systemImage: "gearshape") { setEditing(true) }
                }
                IconButton("Close details", systemImage: "chevron.right.2", action: close)
                    .keyboardShortcut("i", modifiers: [.command, .option])
            }
            .padding(.horizontal, 12)
            .frame(height: 44)
            ZStack {
                if let bot, bot.isGroup {
                    GroupMembersList(group: bot)
                } else if editing {
                    BotSettingsPanel(botId: botId)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                } else {
                    details
                        .transition(.move(edge: .leading).combined(with: .opacity))
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
            .clipped()
        }
        .background(Palette.background)
    }

    private func setEditing(_ on: Bool) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { editing = on }
    }

    private func panelTitle(_ title: String) -> some View {
        Text(title)
            .appFont(.system(size: 12, weight: .medium))
            .foregroundStyle(Palette.secondary)
            .padding(.leading, 4)
    }

    private var details: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                computerSummary
                RoutinesView(botId: botId, initialId: routineId)
                if let bot { agentSummary(bot) }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
    }

    private var computerSummary: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                // Laptop, Mac mini, Linux…: the computer's own icon and color, as everywhere else.
                ComputerBadge(model.computer, size: 32)
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.hostName)
                        .appFont(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Palette.text)
                        .lineLimit(2)
                    Text("Remote screen")
                        .appFont(.system(size: 11))
                        .foregroundStyle(Palette.secondary)
                }
            }
            if !model.isOffline {
                Label(computerStatus, systemImage: computerStatusSymbol)
                    .appFont(.system(size: 12, weight: .medium))
                    .foregroundStyle(Palette.text)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if screenReady {
                Text("View and control this Mac from your iPhone.")
                    .appFont(.system(size: 11))
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, -8)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
    }

    private func agentSummary(_ bot: Bot) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Agent")
                .appFont(.system(size: 13, weight: .semibold))
                .foregroundStyle(Palette.text)
            HStack {
                Text("Runtime").foregroundStyle(Palette.secondary)
                Spacer(minLength: 12)
                Text(model.backendName(bot.backend)).foregroundStyle(Palette.text)
            }
            .appFont(.system(size: 12))
            VStack(alignment: .leading, spacing: 6) {
                Label(bot.managedWorkspace ? "Workspace" : "Project folder", systemImage: "folder")
                    .appFont(.system(size: 11))
                    .foregroundStyle(Palette.secondary)
                Text(bot.managedWorkspace ? "Personal · managed by Codync" : bot.cwd)
                    .appFont(.system(size: 11))
                    .foregroundStyle(Palette.text)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            if !bot.description.isEmpty {
                Text(bot.description)
                    .appFont(.system(size: 12))
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var screenReady: Bool {
        !model.isOffline && model.screen?.enabled == true
            && model.screen?.connected == true && model.screen?.capture == true
    }

    private var computerStatusSymbol: String {
        guard let screen = model.screen, screen.enabled else { return "power" }
        if !screen.connected { return "arrow.triangle.2.circlepath" }
        if !screen.capture { return "exclamationmark.circle" }
        return screen.agentBot == botId ? "cursorarrow.motionlines" : "checkmark.circle"
    }

    private var computerStatus: String {
        guard let screen = model.screen, screen.enabled else { return "Remote screen is off" }
        guard screen.connected else { return "Connecting to computer…" }
        guard screen.capture else { return "Screen recording permission needed" }
        return screen.agentBot == botId ? "This bot is using your Mac" : "Ready to connect"
    }
}
#endif

private extension View {
    @ViewBuilder func conversationInitialBottomAnchor() -> some View {
        if #available(iOS 18, macOS 15, *) {
            defaultScrollAnchor(.bottom, for: .initialOffset)
        } else {
            defaultScrollAnchor(.bottom)
        }
    }

    @ViewBuilder func conversationBottomObserver(_ isAtBottom: Binding<Bool>) -> some View {
        if #available(iOS 18, macOS 15, *) {
            onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.contentSize.height - geometry.visibleRect.maxY < 48
            } action: { _, isAtBottomNow in
                isAtBottom.wrappedValue = isAtBottomNow
            }
        } else {
            self
        }
    }

    @ViewBuilder func conversationScrollEdges() -> some View {
        if #available(iOS 26, macOS 26, *) {
            #if os(iOS)
                scrollEdgeEffectStyle(.soft, for: .top)
            #else
                self
            #endif
        } else {
            self
        }
    }
}
