import CodyncKit
import SwiftUI

/// One endless conversation with a bot. Only deliberate messages show here;
/// tool calls and thinking live in the "Full conversation" sheet.
public struct ThreadView: View {
    let botId: String

    public init(botId: String) { self.botId = botId }
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var draft = ""
    @State private var showTrace = false
    @State private var editing: EditorRequest?
    @State private var confirmNewSession = false
    @State private var confirmDelete: Bot?
    /// Desktop: the bot's settings as an inspector beside the chat.
    @State private var showSettings = true
    @State private var editingDetails = false
    @State private var availableWidth: CGFloat = 800
    @State private var compactDetails = false
    @FocusState private var composerFocused: Bool

    private var bot: Bot? { model.bots[botId] }

    public var body: some View {
        #if os(macOS)
            GeometryReader { geometry in
                HStack(spacing: 0) {
                    conversation.frame(maxWidth: .infinity)
                    if showSettings && geometry.size.width >= 680 {
                        Rectangle().fill(Palette.border).frame(width: 1)
                        detailsPanel.frame(width: 320)
                    }
                }
                .onGeometryChange(for: CGFloat.self) {
                    $0.size.width
                } action: {
                    availableWidth = $0
                }
            }
            .ignoresSafeArea(.container, edges: .top)
            .sheet(isPresented: $compactDetails) {
                detailsPanel.frame(width: 340, height: 600)
            }
        #else
            conversation
        #endif
    }

    @ViewBuilder private var conversation: some View {
        let thread = model.thread(botId)
        let items = ChatItem.build(thread)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if !model.historyComplete.contains(botId), thread.count >= 50 {
                        Button("Load earlier messages") { Task { await model.loadOlder(botId) } }
                            .font(.footnote)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                    }
                    if items.isEmpty, let bot {
                        IntroCard(bot: bot).padding(.top, 40)
                    }
                    ForEach(items) { item in
                        row(item)
                            .id(item.id)
                    }
                    // Offline, "working" is only what the computer last said; don't show it as live.
                    if let bot, bot.isWorking, !model.isOffline {
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
                composer
            }
        }
        .inlineNavigationTitle()
        .toolbar {
            #if os(iOS)
                ToolbarItem(placement: .principal) { header }
            #endif
            #if os(iOS)
                if model.screen?.agentBot == botId {
                    ToolbarItem(placement: .primaryAction) {
                        // The bot is operating the computer: watch it live (and take over from there).
                        Button("Watch the screen", systemImage: "cursorarrow.motionlines") {
                            model.screenRequest = ScreenRequest(watching: botId)
                        }
                        .symbolEffect(.pulse, options: .repeating)
                        .tint(Palette.accent)
                    }
                }
            #endif
            #if os(iOS)
                ToolbarItem(placement: .primaryAction) { menu }
            #endif
        }
        #if os(macOS)
            .safeAreaInset(edge: .top, spacing: 0) {
                VStack(spacing: 0) {
                    HStack {
                        header
                        Spacer(minLength: 8)
                        menu
                        if !showSettings || availableWidth < 680 {
                            Button("Conversation details", systemImage: "chevron.right.2") { toggleDetails() }
                                .labelStyle(.iconOnly)
                                .help("Show conversation details")
                                .keyboardShortcut("i", modifiers: [.command, .option])
                        }
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.secondary)
                    .padding(.horizontal, 16)
                    .frame(height: 44)
                    Divider().overlay(Palette.border)
                    ConnectionBanner().padding(.horizontal, 16).padding(.top, model.isOffline ? 8 : 0)
                }
                .background(Palette.background)
            }
            .onAppear {
                if bot?.name == "New Bot", model.thread(botId).isEmpty { editingDetails = true }
            }
        #endif
        .onAppear { model.markRead(botId) }
        .sheet(isPresented: $showTrace) {
            NavigationStack { TraceView(botId: botId) }
                #if os(macOS)
                    .frame(width: 620, height: 560)
                #endif
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
        }
        .sheet(item: $editing) { request in
            NavigationStack { BotEditorView(draft: request.draft) }
        }
        .confirmationDialog("Start a new session?", isPresented: $confirmNewSession, titleVisibility: .visible) {
            Button("New session") { model.newSession(botId) }
        } message: {
            Text("The conversation stays here, but the agent starts with a fresh context.")
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
            switch e.kind {
            case "user":
                UserBubble(entry: e, botWorking: bot?.isWorking == true)
                    .padding(.top, groupStart ? 12 : 4)
            case "agent":
                AgentBubble(entry: e) { showTrace = true }
                    .padding(.top, groupStart ? 12 : 4)
            case "permission":
                PermissionCard(entry: e, hostName: model.hostName) { option in
                    model.respond(e, option: option)
                }
                .padding(.top, 12)
            default:
                NoticeRow(entry: e).padding(.top, 10)
            }
        }
    }

    // MARK: chrome

    private var header: some View {
        Button {
            #if os(macOS)
                toggleDetails()
            #else
                if let bot { editing = EditorRequest(BotDraft(bot)) }
            #endif
        } label: {
            HStack(spacing: 7) {
                if let bot { CharacterAvatar(bot: bot, size: 22) }
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
            if availableWidth < 680 { compactDetails.toggle() } else { showSettings.toggle() }
        }

        private var detailsPanel: some View {
            VStack(spacing: 0) {
                HStack(spacing: 10) {
                    if editingDetails {
                        Button("Back to details", systemImage: "chevron.left") { editingDetails = false }
                            .labelStyle(.iconOnly)
                        Text("Settings").font(.system(size: 13, weight: .semibold))
                        Spacer()
                    } else {
                        Spacer()
                        Button("Bot settings", systemImage: "gearshape") { editingDetails = true }
                            .labelStyle(.iconOnly)
                            .help("Bot settings")
                    }
                    Button("Close details", systemImage: "chevron.right.2") {
                        showSettings = false
                        compactDetails = false
                    }
                    .labelStyle(.iconOnly)
                    .help("Close details")
                    .keyboardShortcut("i", modifiers: [.command, .option])
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.secondary)
                .padding(.horizontal, 16)
                .frame(height: 44)
                if editingDetails {
                    BotSettingsPanel(botId: botId)
                } else {
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
                                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Palette.border, lineWidth: 0.5))
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
                            Button {
                                showTrace = true
                            } label: {
                                HStack {
                                    Label("Full conversation", systemImage: "list.bullet.rectangle")
                                    Spacer()
                                    Image(systemName: "chevron.right").font(.caption2)
                                }
                                .font(.system(size: 13))
                                .padding(12)
                                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 8))
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.horizontal, 16)
                        .padding(.bottom, 16)
                    }
                }
            }
            .background(Palette.background)
        }

        private var computerStatus: String {
            guard !model.isOffline else { return "Computer is offline" }
            guard let screen = model.screen, screen.enabled else { return "Remote screen is off" }
            guard screen.connected else { return "Connecting to computer…" }
            guard screen.capture else { return "Screen recording permission needed" }
            return screen.agentBot == botId ? "This bot is using your Mac" : "Ready · View this Mac from your iPhone"
        }
    #endif

    private var menu: some View {
        Menu {
            Button("Full conversation", systemImage: "list.bullet.rectangle") { showTrace = true }
            if let bot {
                Button("Edit profile", systemImage: "pencil") {
                    #if os(macOS)
                        editingDetails = true
                        showSettings = true
                        if availableWidth < 680 { compactDetails = true }
                    #else
                        editing = EditorRequest(BotDraft(bot))
                    #endif
                }
                Button(bot.pinned ? "Unpin" : "Pin", systemImage: "pin") { model.setPinned(bot, !bot.pinned) }
            }
            Button("New session", systemImage: "arrow.counterclockwise") { confirmNewSession = true }
            Divider()
            Button("Delete bot", systemImage: "trash", role: .destructive) { confirmDelete = bot }
        } label: {
            #if os(macOS)
            Image(systemName: "square.and.arrow.up")
            #else
            Image(systemName: "ellipsis.circle")
            #endif
        }
    }

    private var canSend: Bool {
        !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && model.connection == .online
    }

    private func submit() {
        guard canSend else { return }
        model.send(draft, to: botId)
        draft = ""
    }

    private var composer: some View {
        let working = bot?.isWorking == true
        return HStack(alignment: .bottom, spacing: 8) {
            TextField(working ? "Queue a message for \(bot?.name ?? "it")" : "Message \(bot?.name ?? "")", text: $draft, axis: .vertical)
                .lineLimit(1...8)
                .textFieldStyle(.plain)
                .focused($composerFocused)
                .padding(.vertical, 10)
                .sendOnReturn(submit)
            if working && draft.isEmpty {
                Button {
                    model.stop(botId)
                } label: {
                    Image(systemName: "stop.fill")
                        .font(.system(size: 12, weight: .bold))
                        .frame(width: 34, height: 34)
                        .background(Palette.accentFill, in: Circle())
                        .foregroundStyle(Palette.onAccent)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Stop")
                .help("Stop")
            } else {
                Button(action: submit) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 14, weight: .bold))
                        .frame(width: 34, height: 34)
                        .background(canSend ? Palette.accentFill : Palette.accentDim, in: Circle())
                        .foregroundStyle(canSend ? Palette.onAccent : Palette.tertiary)
                }
                .buttonStyle(.plain)
                .disabled(!canSend)
                .accessibilityLabel("Send")
                .help("Send")
            }
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .composerSurface(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.top, 6)
        .padding(.bottom, 16)
        #if os(macOS)
            .frame(maxWidth: 820)
            .frame(maxWidth: .infinity)
        #endif
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
            let author = e.kind == "user" ? "user" : e.kind == "agent" ? "agent" : e.kind
            out.append(ChatItem(id: e.id, kind: .entry(e, groupStart: author != lastAuthor)))
            lastAuthor = (author == "user" || author == "agent") ? author : nil
            lastDate = date
        }
        return out
    }
}

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
