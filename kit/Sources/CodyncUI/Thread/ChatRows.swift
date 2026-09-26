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

    var body: some View {
        let reply = openThread.map { open in { open(entry) } }
        switch entry.kind {
        case "user":
            VStack(alignment: .trailing, spacing: 4) {
                UserBubble(entry: entry, botWorking: chat?.isWorking == true, reply: reply)
                threadChip
            }
            .padding(.top, groupStart ? 12 : 4)
        case "agent":
            VStack(alignment: .leading, spacing: 4) {
                if chat?.isGroup == true, groupStart { AuthorLabel(botId: entry.data.author) }
                AgentBubble(entry: entry, openTrace: openTrace, reply: reply)
                threadChip.padding(.leading, chat?.isGroup == true ? 34 : 0)
            }
            .padding(.top, groupStart ? 12 : 4)
        case "permission":
            VStack(alignment: .leading, spacing: 4) {
                if chat?.isGroup == true { AuthorLabel(botId: entry.data.author) }
                PermissionCard(entry: entry, hostName: model.hostName) { option in
                    model.respond(entry, option: option)
                }
            }
            .padding(.top, 12)
        default:
            NoticeRow(entry: entry).padding(.top, 10)
        }
    }

    @ViewBuilder private var threadChip: some View {
        if let summary = entry.data.thread, summary.count > 0, let openThread {
            ThreadChip(summary: summary) { openThread(entry) }
        }
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
                Text(RelativeTime.day(Date(milliseconds: summary.lastAt)))
                    .font(.footnote)
                    .foregroundStyle(Palette.tertiary)
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(Palette.surface, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel("View thread, \(summary.count) \(summary.count == 1 ? "reply" : "replies")")
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
            Text(entry.data.text ?? "")
                .font(InterfaceMetrics.body)
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
                .padding(.vertical, InterfaceMetrics.value(mac: 8, mobile: 10))
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                .contextActions {
                    var items = [MenuItem("Copy", icon: "doc.on.doc") { Pasteboard.copy(entry.data.text) }]
                    if let reply { items.append(MenuItem("Reply in thread", icon: "bubble.left.and.bubble.right", action: reply)) }
                    return items
                }
            status
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 56)
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
            #if os(macOS)
                Text(entry.date, style: .time).font(.system(size: 10)).foregroundStyle(Palette.tertiary)
            #else
                EmptyView()
            #endif
        }
    }
}

struct AgentBubble: View {
    let entry: Entry
    let openTrace: () -> Void
    var reply: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            MarkdownText(entry.data.text ?? "")
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
                .padding(.vertical, InterfaceMetrics.value(mac: 8, mobile: 10))
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                .contextActions {
                    var items = [
                        MenuItem("Copy", icon: "doc.on.doc") { Pasteboard.copy(entry.data.text) },
                        MenuItem("Show what it did", icon: "list.bullet.rectangle", action: openTrace),
                    ]
                    if let reply { items.insert(MenuItem("Reply in thread", icon: "bubble.left.and.bubble.right", action: reply), at: 1) }
                    return items
                }
            #if os(macOS)
                Text(entry.date, style: .time)
                    .font(.system(size: 10))
                    .foregroundStyle(Palette.tertiary)
                    .padding(.leading, 12)
            #endif
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        #if os(macOS)
            .padding(.trailing, 64)
        #else
            .padding(.trailing, 40)
        #endif
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
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
        }
    }
}

/// The bot's live activity line: a turning orb, what it's doing and for how long.
struct WorkingIndicator: View {
    let bot: Bot
    let openTrace: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Button(action: openTrace) {
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
                    Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
                }
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
                .padding(.vertical, 12)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            }
            .buttonStyle(.plain)
            Spacer()
        }
    }
}
