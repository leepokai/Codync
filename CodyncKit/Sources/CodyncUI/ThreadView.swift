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
                    if let bot, bot.isWorking {
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
        .safeAreaInset(edge: .bottom) { composer }
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .principal) { header }
            ToolbarItem(placement: .primaryAction) { menu }
        }
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
            Text(date.formatted(.relative(presentation: .named)) + " · " + date.formatted(date: .omitted, time: .shortened))
                .metaStyle()
                .frame(maxWidth: .infinity)
                .padding(.vertical, 16)
        case let .entry(e, groupStart):
            switch e.kind {
            case "user":
                UserBubble(entry: e, botWorking: bot?.isWorking == true)
                    .padding(.top, groupStart ? 12 : 3)
            case "agent":
                AgentBubble(entry: e, bot: bot, showAvatar: groupStart) { showTrace = true }
                    .padding(.top, groupStart ? 12 : 3)
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
        HStack(spacing: 8) {
            if let bot { CharacterAvatar(bot: bot, size: 28) }
            VStack(alignment: .leading, spacing: 0) {
                Text(bot?.name ?? "").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
                Group {
                    if let bot, bot.needsInput {
                        Text("Needs you").metaStyle(Palette.warning)
                    } else if let bot, bot.isWorking {
                        Text("Working").metaStyle(Palette.secondary)
                    } else if let bot {
                        Text("\(model.backendName(bot.backend)) · \(bot.folderName)").metaStyle()
                    }
                }
                .lineLimit(1)
            }
        }
        .onTapGesture { if let bot { editing = EditorRequest(BotDraft(bot)) } }
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

    private var composer: some View {
        let working = bot?.isWorking == true
        let canSend = !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && model.connection == .online
        return HStack(alignment: .bottom, spacing: 8) {
            TextField(working ? "Message (queued until it's done)" : "Message \(bot?.name ?? "")", text: $draft, axis: .vertical)
                .lineLimit(1...6)
                .textFieldStyle(.plain)
                .focused($composerFocused)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).stroke(Palette.border))
            if working && draft.isEmpty {
                Button {
                    model.stop(botId)
                } label: {
                    Image(systemName: "stop.fill")
                        .font(.system(size: 14, weight: .bold))
                        .frame(width: 38, height: 38)
                        .background(Palette.bubbleAgent, in: Circle())
                        .foregroundStyle(Palette.text)
                }
                .accessibilityLabel("Stop")
            } else {
                Button {
                    model.send(draft, to: botId)
                    draft = ""
                } label: {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 16, weight: .bold))
                        .frame(width: 38, height: 38)
                        .background(canSend ? Palette.accentFill : Palette.bubbleAgent, in: Circle())
                        .foregroundStyle(canSend ? Palette.onAccent : Palette.tertiary)
                }
                .disabled(!canSend)
                .accessibilityLabel("Send")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
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
