import CodyncKit
import SwiftUI

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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

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
        let items = ChatItem.build(thread)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if !model.historyComplete.contains(botId), thread.count >= 50 {
                        Button("Load earlier messages") { Task { await model.loadOlder(botId) } }
                            .buttonStyle(.plain)
                            .font(.footnote)
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
                    }
                    // Offline, "working" is only what the computer last said; don't show it as live.
                    if let bot, bot.isWorking(in: botId, thread: nil), !model.isOffline {
                        WorkingIndicator(bot: bot) { showTrace = true }
                            .padding(.top, 6)
                            .id("working")
                    }
                    Color.clear.frame(height: 8).id("bottom")
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                #if os(macOS)
                    .frame(maxWidth: 820)
                    .frame(maxWidth: .infinity)
                #endif
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: items.last?.id) { _, _ in
                withAnimation(.snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
                model.markRead(botId)
            }
            .onChange(of: bot?.isWorking) { _, _ in
                withAnimation(.snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
            }
        }
        .background(Palette.background)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 6) {
                #if os(iOS)
                    ConnectionBanner().padding(.horizontal, 14)
                #endif
                Composer(botId: botId)
            }
        }
        #if os(iOS)
            .safeAreaInset(edge: .top, spacing: 0) {
                ScreenHeader {
                    BackButton { dismiss() }
                } title: {
                    header
                } trailing: {
                    if model.screen?.agentBot == botId {
                        // The bot is operating the computer: watch it live (and take over from there).
                        IconButton("Watch the screen", systemImage: "cursorarrow.motionlines") {
                            model.screenRequest = ScreenRequest(watching: botId)
                        }
                        .symbolEffect(.pulse, options: .repeating)
                        .transition(.opacity)
                    }
                    if bot?.isGroup != true {
                        IconButton("Call", systemImage: "phone") { calling = true }
                    }
                    menu
                }
                .animation(Motion.reduced(Motion.fade, reduceMotion), value: model.screen?.agentBot == botId)
            }
            .hidesSystemNavigationBar()
            .codyncOverlay(isPresented: $calling) { close in
                CallView(botId: botId, close: close)
            }
        #endif
        #if os(macOS)
            .safeAreaInset(edge: .top, spacing: 0) {
                VStack(spacing: 0) {
                    HStack {
                        header
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
                    ConnectionBanner().padding(.horizontal, 16).padding(.top, model.isOffline ? 8 : 0)
                }
                .background(Palette.background)
            }
            .onAppear {
                if bot?.name == "New Bot", model.chat(botId).isEmpty { editingDetails = true }
            }
        #endif
        .onAppear { model.markRead(botId) }
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
        .deleteBotConfirmation($confirmDelete) { dismiss() }
    }

    // MARK: rows

    @ViewBuilder private func row(_ item: ChatItem) -> some View {
        switch item.kind {
        case .separator(let date):
            Text(RelativeTime.separator(date))
                .font(.footnote)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
                .padding(.top, 18)
                .padding(.bottom, 6)
        case .entry(let e, let groupStart):
            ChatRow(entry: e, groupStart: groupStart, chat: bot) {
                showTrace = true
            } openThread: { root in
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { openThread = ThreadTarget(id: root.id) }
            }
        }
    }

    // MARK: chrome

    private var header: some View {
        Button {
            #if os(macOS)
                toggleDetails()
            #else
                if bot?.isGroup == true { editingGroup = true } else if let bot { editing = EditorRequest(BotDraft(bot)) }
            #endif
        } label: {
            HStack(spacing: 7) {
                if let bot {
                    if bot.isGroup { GroupAvatar(members: model.members(of: bot), size: 22) } else { CharacterAvatar(bot: bot, size: 22) }
                }
                Text(bot?.name ?? "")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
            }
            #if os(iOS)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .glass(in: Capsule())
            #endif
        }
        .buttonStyle(.plain)
        .accessibilityLabel("View conversation details")
        .help("View conversation details")
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
            DetailsPanel(botId: botId, editing: $editingDetails, editGroup: { editingGroup = true }) {
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showSettings = false }
                compactDetails = false
            }
        }
    #endif

    private var menu: some View {
        DropdownMenu {
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
            items.append(MenuItem("New session", icon: "arrow.counterclockwise") { confirmNewSession = true })
            items.append(MenuItem("Delete bot", icon: "trash", destructive: true, divider: true) { confirmDelete = bot })
            return items
        } label: {
            Image(systemName: "ellipsis.circle")
                .font(.system(size: InterfaceMetrics.value(mac: 14, mobile: 18), weight: .medium))
                .foregroundStyle(Palette.secondary)
                .frame(width: InterfaceMetrics.value(mac: 28, mobile: 36), height: InterfaceMetrics.value(mac: 28, mobile: 36))
                .contentShape(Rectangle())
        }
        .accessibilityLabel("More")
        .help("More")
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
    static func build(_ entries: [Entry]) -> [ChatItem] {
        var out: [ChatItem] = []
        var lastDate: Date?
        var lastAuthor: String?
        for e in entries where e.isChat {
            let date = e.date
            if lastDate.map({ date.timeIntervalSince($0) > 3600 }) ?? true {
                out.append(ChatItem(id: "sep-\(e.id)", kind: .separator(date)))
                lastAuthor = nil
            }
            // In a group each bot is its own author.
            let author = e.kind == "user" ? "user" : e.kind == "agent" ? "agent:\(e.data.author ?? "")" : e.kind
            out.append(ChatItem(id: e.id, kind: .entry(e, groupStart: author != lastAuthor)))
            lastAuthor = (author == "user" || e.kind == "agent") ? author : nil
            lastDate = date
        }
        return out
    }
}

/// The start of an empty group chat: who's in it and how the room works.
private struct GroupIntroCard: View {
    let group: Bot
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            GroupAvatar(members: model.members(of: group), size: 72)
            Text(group.name).font(.title2.weight(.semibold)).foregroundStyle(Palette.text)
            Text(model.members(of: group).map(\.name).joined(separator: " · "))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            Text("Everyone answers in turn. @mention a bot to ask just that one. Each bot works in its own folder.")
                .font(.footnote)
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
                Text("\(group.members.count) bots").font(.system(size: 13, weight: .semibold)).padding(.bottom, 6)
                ForEach(model.members(of: group)) { bot in
                    Button { model.selection = bot.id } label: {
                        HStack(spacing: 10) {
                            CharacterAvatar(bot: bot, size: 26)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(bot.name).font(.system(size: 13)).foregroundStyle(Palette.text)
                                Text(bot.isWorking ? (bot.activity.isEmpty ? "Working…" : bot.activity) : bot.folderName)
                                    .font(.system(size: 11)).foregroundStyle(Palette.tertiary).lineLimit(1)
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
            Text(bot.name).font(.title2.weight(.semibold)).foregroundStyle(Palette.text)
            Text("\(model.backendName(bot.backend)) in \(bot.cwd)")
                .font(.footnote.monospaced())
                .foregroundStyle(Palette.tertiary)
                .multilineTextAlignment(.center)
            if !bot.description.isEmpty {
                Text(bot.description)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
            }
            Text("Tell it what you need. You'll get a notification when it's done or needs you.")
                .font(.footnote)
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
                    Text("Settings").font(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.text)
                    Spacer()
                } else if bot?.isGroup == true {
                    Spacer()
                    IconButton("Edit group", systemImage: "gearshape", action: editGroup)
                } else {
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

    private var details: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                VStack(spacing: 12) {
                    VStack(spacing: 12) {
                        Image(systemName: "desktopcomputer")
                            .font(.system(size: 32, weight: .light))
                        Text(computerStatus)
                            .font(.system(size: 12))
                            .multilineTextAlignment(.center)
                            .foregroundStyle(Palette.secondary)
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: 164)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 8))
                    Text(model.hostName)
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                }
                if let bot {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Agent").font(.system(size: 13, weight: .semibold))
                        Text(model.backendName(bot.backend)).foregroundStyle(Palette.secondary)
                        Text(bot.cwd)
                            .font(.system(size: 12))
                            .foregroundStyle(Palette.secondary)
                            .textSelection(.enabled)
                        if !bot.description.isEmpty {
                            Text(bot.description).foregroundStyle(Palette.secondary)
                        }
                    }
                    .font(.system(size: 13))
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
    }

    private var computerStatus: String {
        guard !model.isOffline else { return "Computer is offline" }
        guard let screen = model.screen, screen.enabled else { return "Remote screen is off" }
        guard screen.connected else { return "Connecting to computer…" }
        guard screen.capture else { return "Screen recording permission needed" }
        return screen.agentBot == botId ? "This bot is using your Mac" : "Ready · View this Mac from your iPhone"
    }
}
#endif
