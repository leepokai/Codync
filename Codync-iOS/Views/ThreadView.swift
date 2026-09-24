import CodyncKit
import SwiftUI

/// One endless conversation with a bot. Only deliberate messages show here;
/// tool calls and thinking live in the "Full conversation" sheet.
struct ThreadView: View {
    let botId: String
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var draft = ""
    @State private var showTrace = false
    @State private var editing: EditorRequest?
    @State private var confirmNewSession = false
    @State private var confirmDelete = false
    @FocusState private var composerFocused: Bool

    private var bot: Bot? { model.bots[botId] }

    var body: some View {
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
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) { header }
            ToolbarItem(placement: .topBarTrailing) { menu }
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
        .confirmationDialog("Delete \(bot?.name ?? "bot")?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete bot and its conversation", role: .destructive) {
                if let bot { model.delete(bot) }
                dismiss()
            }
        }
    }

    // MARK: rows

    @ViewBuilder private func row(_ item: ChatItem) -> some View {
        switch item.kind {
        case let .separator(date):
            Text(date.formatted(.relative(presentation: .named)).capitalized + " · " + date.formatted(date: .omitted, time: .shortened))
                .font(.caption2)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
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
                        Text("Needs you").foregroundStyle(Palette.warning)
                    } else if let bot, bot.isWorking {
                        Text("Working").foregroundStyle(Palette.accent)
                    } else if let bot {
                        Text("\(BackendInfo.name(bot.backend)) · \(bot.folderName)").foregroundStyle(Palette.tertiary)
                    }
                }
                .font(.caption2)
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
            Button("Delete bot", systemImage: "trash", role: .destructive) { confirmDelete = true }
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

// MARK: - Rows

struct UserBubble: View {
    let entry: Entry
    let botWorking: Bool
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            Text(entry.data.text ?? "")
                .font(.body)
                .foregroundStyle(Palette.onAccent)
                .textSelection(.enabled)
                .padding(.horizontal, 13)
                .padding(.vertical, 9)
                .background(Palette.accentFill, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = entry.data.text }
                }
            status
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 48)
    }

    @ViewBuilder private var status: some View {
        switch entry.data.status {
        case "sending":
            Text("Sending…").font(.caption2).foregroundStyle(Palette.tertiary)
        case "queued" where botWorking:
            Text("Waiting to send — it'll read this when it's done").font(.caption2).foregroundStyle(Palette.tertiary)
        case "failed":
            HStack(spacing: 10) {
                Text("Failed to send").foregroundStyle(Palette.danger)
                Button("Resend") { model.retry(entry) }
                Button("Delete") { model.discard(entry) }
            }
            .font(.caption2.bold())
        case "cancelled":
            Text("Not sent — stopped").font(.caption2).foregroundStyle(Palette.tertiary)
        default:
            EmptyView()
        }
    }
}

struct AgentBubble: View {
    let entry: Entry
    let bot: Bot?
    let showAvatar: Bool
    let openTrace: () -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            Group {
                if showAvatar, let bot {
                    CharacterAvatar(bot: bot, size: 28, animated: false)
                } else {
                    Color.clear
                }
            }
            .frame(width: 28, height: 28)
            MarkdownText(entry.data.text ?? "")
                .padding(.horizontal, 13)
                .padding(.vertical, 9)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = entry.data.text }
                    Button("Show what it did", systemImage: "list.bullet.rectangle", action: openTrace)
                }
            Spacer(minLength: 24)
        }
    }
}

struct NoticeRow: View {
    let entry: Entry

    var body: some View {
        let text = entry.data.text ?? ""
        switch entry.data.style {
        case "divider":
            HStack(spacing: 10) {
                Rectangle().fill(Palette.border).frame(height: 1)
                Text(text)
                    .font(.caption2)
                    .foregroundStyle(Palette.tertiary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 260)
                    .layoutPriority(1)
                Rectangle().fill(Palette.border).frame(height: 1)
            }
            .padding(.vertical, 6)
        case "error":
            Label {
                Text(text).font(.footnote).foregroundStyle(Palette.text).textSelection(.enabled)
            } icon: {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.danger)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.danger.opacity(0.12), in: RoundedRectangle(cornerRadius: 12))
        default:
            Text(text)
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
        }
    }
}

/// Grok Bot's typing bubble, plus the live activity line and elapsed time.
struct WorkingIndicator: View {
    let bot: Bot
    let openTrace: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            CharacterAvatar(bot: bot, size: 28)
            VStack(alignment: .leading, spacing: 4) {
                TypingDots()
                    .padding(.horizontal, 12)
                    .padding(.vertical, 11)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                Button(action: openTrace) {
                    HStack(spacing: 4) {
                        Text(bot.activity.isEmpty ? "Working…" : bot.activity).lineLimit(1)
                        if let started = bot.startedAt {
                            Text("·")
                            Text(Date(milliseconds: started), style: .timer).monospacedDigit()
                        }
                        Image(systemName: "chevron.right").font(.caption2)
                    }
                    .font(.caption)
                    .foregroundStyle(bot.needsInput ? Palette.warning : Palette.secondary)
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
    }
}

struct TypingDots: View {
    var body: some View {
        HStack(spacing: 4) {
            ForEach(0..<3, id: \.self) { i in
                Circle()
                    .fill(Palette.secondary)
                    .frame(width: 6, height: 6)
                    .phaseAnimator([0.3, 1.0]) { view, phase in
                        view.opacity(phase)
                    } animation: { _ in .easeInOut(duration: 0.5).delay(Double(i) * 0.15) }
            }
        }
    }
}

private struct IntroCard: View {
    let bot: Bot

    var body: some View {
        VStack(spacing: 12) {
            CharacterAvatar(bot: bot, size: 72)
            Text(bot.name).font(.title2.bold()).foregroundStyle(Palette.text)
            Text("\(BackendInfo.name(bot.backend)) in \(bot.cwd)")
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
