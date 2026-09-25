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
    @State private var showSettings = false
    @FocusState private var composerFocused: Bool

    private var bot: Bot? { model.bots[botId] }

    public var body: some View {
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
                .padding(.horizontal, 14)
                .padding(.top, 8)
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
                ConnectionBanner().padding(.horizontal, 14)
                composer
            }
        }
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .principal) { header }
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
            ToolbarItem(placement: .primaryAction) { menu }
            #if os(macOS)
            ToolbarItem(placement: .primaryAction) {
                Button("Settings", systemImage: "sidebar.right") { showSettings.toggle() }
                    .help("Bot settings")
            }
            #endif
        }
        #if os(macOS)
        .inspector(isPresented: $showSettings) {
            BotSettingsPanel(botId: botId)
                .inspectorColumnWidth(min: 300, ideal: 360, max: 460)
        }
        .onAppear { if bot?.name == "New Bot", model.thread(botId).isEmpty { showSettings = true } }
        #endif
        .onAppear { model.markRead(botId) }
        .sheet(isPresented: $showTrace) {
            NavigationStack { TraceView(botId: botId) }
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
        case let .separator(date):
            Text(RelativeTime.separator(date))
                .font(.footnote)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
                .padding(.top, 18)
                .padding(.bottom, 6)
        case let .entry(e, groupStart):
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
        // A pill with the bot and its name; the avatar itself shows working / needs you.
        HStack(spacing: 8) {
            if let bot { CharacterAvatar(bot: bot, size: 26) }
            Text(bot?.name ?? "")
                .font(.body.weight(.semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
        }
        .padding(.leading, 8)
        .padding(.trailing, 16)
        .padding(.vertical, 6)
        .glass(in: Capsule())
        .onTapGesture {
            #if os(macOS)
            showSettings.toggle()
            #else
            if let bot { editing = EditorRequest(BotDraft(bot)) }
            #endif
        }
    }

    private var menu: some View {
        Menu {
            Button("Full conversation", systemImage: "list.bullet.rectangle") { showTrace = true }
            if let bot {
                Button("Edit profile", systemImage: "pencil") { editing = EditorRequest(BotDraft(bot)) }
                Button(bot.pinned ? "Unpin" : "Pin", systemImage: "pin") { model.setPinned(bot, !bot.pinned) }
            }
            Button("New session", systemImage: "arrow.counterclockwise") { confirmNewSession = true }
            Divider()
            Button("Delete bot", systemImage: "trash", role: .destructive) { confirmDelete = bot }
        } label: {
            Image(systemName: "ellipsis.circle")
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
                Button { model.stop(botId) } label: {
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
        .padding(.horizontal, 12)
        .padding(.top, 6)
        .padding(.bottom, 10)
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
