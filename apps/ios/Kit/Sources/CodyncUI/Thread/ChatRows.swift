import CodyncKit
import SwiftUI

/// The rows a conversation is made of.

/// One chat-visible entry in a chat or a thread, with what can be done to it.
struct ChatRow: View {
    let entry: Entry
    let groupStart: Bool
    /// The bot or group the chat belongs to.
    let chat: Bot?
    let openTrace: () -> Void
    /// Main chat only: start (or open) the thread on this message.
    var openThread: ((Entry) -> Void)?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let reply = openThread.map { open in { open(entry) } }
        switch entry.kind {
        case "user":
            VStack(alignment: .trailing, spacing: 4) {
                UserBubble(entry: entry, botWorking: chat?.isWorking == true, reply: reply)
                ReactionsRow(entry: entry)
                threadChip
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: entry.data.reactions)
            .padding(.top, groupStart ? 12 : 4)
        case "agent":
            VStack(alignment: .leading, spacing: 4) {
                if chat?.isGroup == true, groupStart { AuthorLabel(botId: entry.data.author) }
                AgentBubble(entry: entry, openTrace: openTrace, reply: reply)
                Group {
                    ReactionsRow(entry: entry)
                    threadChip
                }
                .padding(.leading, chat?.isGroup == true ? 34 : 0)
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: entry.data.reactions)
            .padding(.top, groupStart ? 12 : 4)
        case "permission":
            VStack(alignment: .leading, spacing: 4) {
                if chat?.isGroup == true { AuthorLabel(botId: entry.data.author) }
                PermissionCard(entry: entry, hostName: model.hostName, answering: model.answering[entry.id]) { option in
                    model.respond(entry, option: option)
                }
            }
            .padding(.top, 12)
        default:
            if let request = entry.data.connectionRequest {
                ConnectionRequestCard(entry: entry, request: request).padding(.top, 12)
            } else {
                NoticeRow(entry: entry).padding(.top, 10)
            }
        }
    }

    @ViewBuilder private var threadChip: some View {
        if let summary = entry.data.thread, summary.count > 0, let openThread {
            ThreadChip(summary: summary) { openThread(entry) }
        }
    }
}

/// A chat's time separator: Today 3:27 AM.
struct TimeSeparator: View {
    let date: Date

    var body: some View {
        Text(RelativeTime.separator(date))
            .font(.footnote)
            .foregroundStyle(Palette.tertiary)
            .frame(maxWidth: .infinity)
            .padding(.top, 18)
            .padding(.bottom, 6)
    }
}

/// Who wrote a message in a group: their avatar and name in their color.
struct AuthorLabel: View {
    let botId: String?
    @Environment(BotStore.self) private var model

    var body: some View {
        let bot = botId.flatMap { model.bots[$0] }
        HStack(spacing: 8) {
            if let bot { CharacterAvatar(bot: bot, size: 26, animated: false) }
            Text(model.authorName(botId))
                .font(.subheadline.weight(.medium))
                .foregroundStyle(bot.map { AvatarPalette.color($0.avatarColor) } ?? Palette.secondary)
        }
    }
}

/// Under a message with a thread (Slack's): who replied, how many, how recently.
struct ThreadChip: View {
    let summary: ThreadSummary
    let open: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Button(action: open) {
            HStack(spacing: 6) {
                HStack(spacing: -6) {
                    ForEach(summary.authors.compactMap { model.bots[$0] }.prefix(3)) { bot in
                        CharacterAvatar(bot: bot, size: 18, animated: false)
                    }
                }
                Text(summary.count == 1 ? "1 reply" : "\(summary.count) replies")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.accent)
                if let unread = summary.unread, unread > 0 {
                    Text("\(unread) new")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(Palette.text)
                        .transition(.opacity)
                } else {
                    Text(RelativeTime.day(Date(milliseconds: summary.lastAt)))
                        .font(.footnote)
                        .foregroundStyle(Palette.tertiary)
                }
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(Palette.surface, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(PressScale())
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: summary.unread)
        .accessibilityLabel(
            "View thread, \(summary.count) \(summary.count == 1 ? "reply" : "replies")"
                + ((summary.unread ?? 0) > 0 ? ", \(summary.unread ?? 0) new" : "")
        )
        .help("View thread")
    }
}

struct UserBubble: View {
    let entry: Entry
    let botWorking: Bool
    var reply: (() -> Void)?
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            if let attachments = entry.data.attachments, !attachments.isEmpty {
                AttachmentList(attachments: attachments, botId: entry.botId)
            }
            if !(entry.data.text ?? "").isEmpty { text }
                status
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 56)
    }

    private var text: some View {
            Text(entry.data.text ?? "")
                .font(.body)
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .contextActions(reactions: model.reactionPick(entry)) {
                    var items = [MenuItem("Copy", icon: "square.on.square") { Pasteboard.copy(entry.data.text) }]
                    if let reply { items.append(MenuItem("Reply in thread", icon: "arrowshape.turn.up.left", action: reply)) }
                    return items
                }
    }

    @ViewBuilder private var status: some View {
        switch entry.data.status {
        case "sending":
            Text("Sending…").font(.caption2).foregroundStyle(Palette.tertiary)
        case "queued":
            Text(botWorking ? "Queued until this response finishes" : "Queued")
                .font(.caption2)
                .foregroundStyle(Palette.tertiary)
        case "failed":
            HStack(spacing: 10) {
                Text("Failed to send").foregroundStyle(Palette.danger)
                Button("Resend", systemImage: "arrow.clockwise") { model.retry(entry) }.labelStyle(.iconOnly).help("Resend")
                Button("Delete", systemImage: "trash") { model.discard(entry) }.labelStyle(.iconOnly).help("Delete")
            }
            .buttonStyle(.plain)
            .font(.caption2.bold())
        case "cancelled":
            Text("Not sent — stopped").font(.caption2).foregroundStyle(Palette.tertiary)
        case "waiting":
            HStack(spacing: 10) {
                Text("Waiting for the computer to come online").foregroundStyle(Palette.tertiary)
                Button("Cancel", systemImage: "xmark.circle") { model.cancelQueued(entry) }
                    .labelStyle(.iconOnly)
                    .help("Don't send")
                    .accessibilityLabel("Don't send")
            }
            .buttonStyle(.plain)
            .font(.caption2)
        case "delivering":
            Text("Delivered to the computer").font(.caption2).foregroundStyle(Palette.tertiary)
        default:
                EmptyView()
        }
    }
}

struct AgentBubble: View {
    let entry: Entry
    let openTrace: () -> Void
    var reply: (() -> Void)?
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Group {
                if let files = entry.data.files, !files.isEmpty {
                    ForEach(files) { file in FileDownloadCard(entry: entry, file: file) }
                } else {
                    MarkdownText(entry.data.text ?? "").equatable()
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                }
            }
                .contextActions(reactions: model.reactionPick(entry)) {
                    var items = [
                        MenuItem("Copy", icon: "square.on.square") { Pasteboard.copy(entry.data.text) },
                        MenuItem("Show what it did", icon: "list.bullet", action: openTrace),
                    ]
                    if let reply { items.insert(MenuItem("Reply in thread", icon: "arrowshape.turn.up.left", action: reply), at: 1) }
                    return items
                }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.trailing, 40)
    }
}


struct NoticeRow: View {
    let entry: Entry

    var body: some View {
        let text = entry.data.text ?? ""
        if let seconds = entry.data.callSeconds {
            Label("\(text) · \(String(format: "%02d:%02d", seconds / 60, seconds % 60))", systemImage: "waveform")
                .font(.footnote.monospacedDigit())
                .foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity)
        } else {
            styled(text)
        }
    }

    @ViewBuilder private func styled(_ text: String) -> some View {
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
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
        }
    }
}

/// The bot's live activity line: a turning orb, what it's doing and for how long. Tapping it
/// unfolds what the bot is thinking right now (the whole trace stays in "Full conversation").
struct WorkingIndicator: View {
    let bot: Bot
    /// The running turn's latest thinking, if the agent shares it.
    var thinking: String?
    @State private var expanded = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Button {
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { expanded.toggle() }
            } label: {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 8) {
                        ThinkingOrb(state: bot.needsInput ? .listening : .working, size: 16, color: bot.needsInput ? Palette.warning : Palette.secondary)
                        Text(bot.activity.isEmpty ? "Working…" : bot.activity)
                            .font(.subheadline)
                            .foregroundStyle(bot.needsInput ? Palette.warning : Palette.secondary)
                            .lineLimit(1)
                        if let started = bot.startedAt {
                            Text(Date(milliseconds: started), style: .timer)
                                .font(.footnote.monospacedDigit())
                                .foregroundStyle(Palette.tertiary)
                        }
                        if thinking != nil {
                            Image(systemName: "chevron.down")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(Palette.tertiary)
                                .rotationEffect(.degrees(expanded ? 180 : 0))
                        }
                    }
                    if expanded, let thinking {
                        ScrollView {
                            Text(thinking)
                                .font(.footnote)
                                .foregroundStyle(Palette.secondary)
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .defaultScrollAnchor(.bottom)
                        .frame(maxHeight: 180)
                        .fixedSize(horizontal: false, vertical: true)
                        .transition(.opacity)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            }
            .buttonStyle(.plain)
            .disabled(thinking == nil)
            .accessibilityHint(thinking == nil ? "" : expanded ? "Hides its thinking" : "Shows its thinking")
            Spacer(minLength: 40)
        }
    }
}

extension BotStore {
    /// The latest thinking of the turn running in a chat (`thread` nil) or thread: the newest
    /// thought since the last message.
    func currentThinking(_ botId: String, thread: String?) -> String? {
        for e in allEntries(botId).reversed() where e.threadId == thread {
            if e.kind == "user" || (e.kind == "agent" && e.data.final == true) { return nil }
            if e.kind == "thought", let text = e.data.text, !text.isEmpty { return text }
        }
        return nil
    }
}

extension BotStore {
    /// The quick-reaction row for a message.
    func reactionPick(_ entry: Entry) -> ReactionPick {
        ReactionPick(emoji: Self.quickReactions, chosen: entry.data.reactions ?? []) { self.react(entry, $0) }
    }
}

/// The user's reactions under a message; tapping one takes it back.
struct ReactionsRow: View {
    let entry: Entry
    @Environment(BotStore.self) private var model

    var body: some View {
        if let reactions = entry.data.reactions, !reactions.isEmpty {
            HStack(spacing: 4) {
                ForEach(reactions, id: \.self) { emoji in
                    Button { model.react(entry, emoji) } label: {
                        Text(emoji)
                            .font(.system(size: 15))
                            .padding(.horizontal, 8)
                            .padding(.vertical, 3)
                            .background(Palette.surface, in: Capsule())
                            .contentShape(Capsule())
                    }
                    .buttonStyle(PressScale())
                    .help("Remove \(emoji)")
                    .accessibilityLabel("Remove reaction \(emoji)")
                    .transition(.scale.combined(with: .opacity))
                }
            }
        }
    }
}

/// iPhone: a row renders again only when what it shows changed, so a reply being written
/// elsewhere in the chat skips it.
extension ChatRow: @MainActor Equatable {
    static func == (a: Self, b: Self) -> Bool {
        a.entry == b.entry && a.groupStart == b.groupStart && a.chat?.isGroup == b.chat?.isGroup
            && a.chat?.isWorking == b.chat?.isWorking && (a.openThread == nil) == (b.openThread == nil)
    }
}
