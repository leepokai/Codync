import CodyncKit
import SwiftUI

/// The rows a conversation is made of.

/// One chat-visible entry in a chat, with what can be done to it.
struct ChatRow: View {
    let entry: Entry
    let groupStart: Bool
    /// The bot or group the chat belongs to.
    let chat: Bot?
    let openTrace: () -> Void
    /// Quotes this message in the composer; nil where it can't be.
    var reply: (() -> Void)?
    /// A failure notice's "Add a fallback": the bot's details.
    var addFallback: (() -> Void)?
    /// An artifact card's artifact, full size.
    var openArtifact: ((ArtifactRef) -> Void)?
    /// A voice call's line: the call's conversation.
    var openCall: (() -> Void)?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        switch entry.kind {
        case "user":
            VStack(alignment: .trailing, spacing: 4) {
                UserBubble(entry: entry, botWorking: chat?.isWorking == true, reply: reply)
                ReactionsRow(entry: entry)
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: entry.data.reactions)
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: entry.data.botReactions)
            .padding(.top, groupStart ? 12 : 4)
        case "agent":
            VStack(alignment: .leading, spacing: 4) {
                if chat?.isGroup == true, groupStart { AuthorLabel(botId: entry.data.author) }
                AgentBubble(entry: entry, openTrace: openTrace, reply: reply, openArtifact: openArtifact)
                ReactionsRow(entry: entry)
                    .padding(.leading, chat?.isGroup == true ? 34 : 0)
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: entry.data.reactions)
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: entry.data.botReactions)
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
            } else if let card = entry.data.handoff {
                HandoffCardRow(entry: entry, card: card).padding(.top, 10)
            } else if let change = entry.data.agentSwitch {
                AgentSwitchRow(entry: entry, change: change).padding(.top, 10)
            } else if entry.data.call != nil, let openCall {
                CallLineRow(entry: entry, open: openCall).padding(.top, 6)
            } else {
                NoticeRow(entry: entry, addFallback: addFallback).padding(.top, 10)
            }
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
    /// The name for an author that isn't one of this computer's bots (a bot on another computer).
    var name: String?
    @Environment(BotStore.self) private var model

    var body: some View {
        let bot = botId.flatMap { model.bots[$0] }
        HStack(spacing: 8) {
            if let bot { CharacterAvatar(bot: bot, size: 26, animated: false) }
            Text(model.authorName(botId, fallback: name))
                .font(.subheadline.weight(.medium))
                .foregroundStyle(bot.map { AvatarPalette.color($0.avatarColor) } ?? Palette.secondary)
        }
    }
}

/// The message a reply quotes, as its first line without Markdown markers.
struct QuoteLine: View {
    let text: String

    var body: some View {
        Label {
            Text(L10n.tr("“\(MessagePreview.text(text))”")).lineLimit(1)
        } icon: {
            Image(systemName: "text.bubble")
        }
        .font(.subheadline)
        .foregroundStyle(Palette.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Replying to \(text)")
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
            if !(entry.data.text ?? "").isEmpty || entry.data.replyTo != nil { text }
                status
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 56)
    }

    private var text: some View {
            VStack(alignment: .leading, spacing: 4) {
                if let quote = entry.data.replyTo { QuoteLine(text: quote.text) }
                if let text = entry.data.text, !text.isEmpty {
                    Text(text)
                        .font(.body)
                        .foregroundStyle(Palette.text)
                        .textSelection(.enabled)
                }
            }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .contextActions(reactions: model.reactionPick(entry)) {
                    var items = [MenuItem("Copy", icon: "square.on.square") { Pasteboard.copy(entry.data.text) }]
                    if let reply { items.append(MenuItem("Reply", icon: "arrowshape.turn.up.left", action: reply)) }
                    return items
                }
    }

    @ViewBuilder private var status: some View {
        switch entry.data.status {
        case "sending":
            Text(L10n.tr("Sending…")).font(.caption2).foregroundStyle(Palette.tertiary)
        case "queued":
            Text(botWorking ? "Queued until this response finishes" : "Queued")
                .font(.caption2)
                .foregroundStyle(Palette.tertiary)
        case "failed":
            HStack(spacing: 10) {
                Text(L10n.tr("Failed to send")).foregroundStyle(Palette.danger)
                Button(L10n.tr("Resend"), systemImage: "arrow.clockwise") { model.retry(entry) }.labelStyle(.iconOnly).help("Resend")
                Button(L10n.tr("Delete"), systemImage: "trash") { model.discard(entry) }.labelStyle(.iconOnly).help("Delete")
            }
            .buttonStyle(.plain)
            .font(.caption2.bold())
        case "cancelled":
            Text(L10n.tr("Not sent — stopped")).font(.caption2).foregroundStyle(Palette.tertiary)
        case "waiting":
            HStack(spacing: 10) {
                Text(L10n.tr("Waiting for the computer to come online")).foregroundStyle(Palette.tertiary)
                Button(L10n.tr("Cancel"), systemImage: "xmark.circle") { model.cancelQueued(entry) }
                    .labelStyle(.iconOnly)
                    .help("Don't send")
                    .accessibilityLabel("Don't send")
            }
            .buttonStyle(.plain)
            .font(.caption2)
        case "delivering":
            Text(L10n.tr("Delivered to the computer")).font(.caption2).foregroundStyle(Palette.tertiary)
        default:
                EmptyView()
        }
    }
}

struct AgentBubble: View {
    let entry: Entry
    let openTrace: () -> Void
    var reply: (() -> Void)?
    var openArtifact: ((ArtifactRef) -> Void)?
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Group {
                if let artifact = entry.data.artifact {
                    ArtifactCard(botId: entry.botId, ref: artifact) { openArtifact?(artifact) }
                } else if let files = entry.data.files, !files.isEmpty {
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
                    if let reply { items.insert(MenuItem("Reply", icon: "arrowshape.turn.up.left", action: reply), at: 1) }
                    return items
                }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.trailing, 40)
    }
}


struct NoticeRow: View {
    let entry: Entry
    var addFallback: (() -> Void)?

    var body: some View {
        let text = entry.data.text ?? ""
        // A call an older app logged, with no conversation to open.
        if let seconds = entry.data.callSeconds {
            Label("\(L10n.tr(text)) · \(String(format: "%02d:%02d", seconds / 60, seconds % 60))", systemImage: "waveform")
                .font(.footnote.monospacedDigit())
                .foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity)
        } else {
            styled(text)
        }
    }

    @ViewBuilder private func styled(_ text: String) -> some View {
        switch entry.data.style {
        case "error":
            VStack(alignment: .leading, spacing: 10) {
                Label {
                    Text(L10n.tr(text)).font(.footnote).foregroundStyle(Palette.text).textSelection(.enabled)
                } icon: {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.danger)
                }
                if entry.data.addFallback == true, let addFallback {
                    Button(L10n.tr("Add a fallback"), action: addFallback).buttonStyle(.secondary)
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.danger.opacity(0.12), in: RoundedRectangle(cornerRadius: 12))
        default:
            Text(L10n.tr(text))
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
        }
    }
}

/// The bot's current activity and elapsed time; with thinking details on (Settings), a tap unfolds the turn's steps.
struct WorkingIndicator: View {
    let bot: Bot
    let steps: [Entry]
    @AppStorage(ThinkingDetails.storageKey) private var details = false
    @State private var expanded = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var open: Bool { details && expanded }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 18, style: .continuous)
        VStack(alignment: .leading, spacing: 0) {
            if details {
                Button {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { expanded.toggle() }
                } label: {
                    status.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityHint(L10n.tr(open ? "Hide steps" : "Show steps"))
                .accessibilityValue(open ? "Expanded" : "Collapsed")
            } else {
                status
            }
            if open {
                WorkingSteps(steps: steps)
                    .padding(.top, 12)
                    .transition(.opacity)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(maxWidth: open ? .infinity : nil, alignment: .leading)
        .background(Palette.bubbleAgent, in: shape)
        // Folding steps never cross the pill's edge.
        .clipShape(shape)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.trailing, 40)
    }

    private var status: some View {
        HStack(spacing: 8) {
            ThinkingOrb(state: bot.needsInput ? .listening : .working, size: 16, color: bot.needsInput ? Palette.warning : Palette.secondary)
            Text(L10n.tr(bot.activity.isEmpty ? "Working…" : bot.activity))
                .font(.subheadline)
                .foregroundStyle(bot.needsInput ? Palette.warning : Palette.secondary)
                .lineLimit(1)
                .frame(maxWidth: open ? .infinity : nil, alignment: .leading)
            if let started = bot.startedAt {
                Text(Date(milliseconds: started), style: .timer)
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(Palette.tertiary)
            }
            if details {
                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Palette.tertiary)
                    .rotationEffect(.degrees(open ? 90 : 0))
            }
        }
    }
}

/// The turn's thinking, narration, tool calls and plan: hugs them, scrolls once they're taller than
/// 280 points, and keeps the newest in view. One scroll view, so what the reader unfolds stays unfolded.
/// A turn can have none yet: Claude's thinking text arrives as its thinking ends.
private struct WorkingSteps: View {
    let steps: [Entry]

    var body: some View {
        CappedHeight(max: 280) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if steps.isEmpty { Text(L10n.tr("Nothing yet.")).foregroundStyle(Palette.tertiary) }
                    ForEach(steps) { TraceRow(entry: $0, unfolded: true) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollIndicators(.hidden)
            .scrollBounceBehavior(.basedOnSize)
            .defaultScrollAnchor(.bottom)
        }
    }
}

extension BotStore {
    /// The quick-reaction row for a message.
    func reactionPick(_ entry: Entry) -> ReactionPick {
        ReactionPick(emoji: Self.quickReactions, chosen: entry.data.reactions ?? []) { self.react(entry, $0) }
    }
}

/// A message's reactions: the user's (tapping one takes it back), then the bots', each emoji with
/// the avatars of the bots that chose it.
struct ReactionsRow: View {
    let entry: Entry
    @Environment(BotStore.self) private var model

    var body: some View {
        let mine = entry.data.reactions ?? []
        let theirs = entry.data.botReactionGroups
            .map { (emoji: $0.emoji, bots: $0.bots.compactMap { model.bots[$0] }) }
            .filter { !$0.bots.isEmpty }
        if !mine.isEmpty || !theirs.isEmpty {
            HStack(spacing: 4) {
                ForEach(mine, id: \.self) { emoji in
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
                ForEach(theirs, id: \.emoji) { group in
                    HStack(spacing: 4) {
                        Text(group.emoji).font(.system(size: 15))
                        HStack(spacing: -5) {
                            ForEach(group.bots.prefix(3)) { bot in CharacterAvatar(bot: bot, size: 16, animated: false) }
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Palette.surface, in: Capsule())
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("\(group.bots.map(\.name).joined(separator: ", ")) reacted \(group.emoji)")
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
            && a.chat?.isWorking == b.chat?.isWorking && (a.reply == nil) == (b.reply == nil)
    }
}
