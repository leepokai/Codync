import CodyncKit
import SwiftUI

public struct BotRow: View {
    let bot: Bot
    /// The agent whose logo marks the avatar, if any.
    let agent: String?
    @Environment(BotStore.self) private var model

    public init(bot: Bot, agent: String? = nil) {
        self.bot = bot
        self.agent = agent
    }

    private let avatar: CGFloat = 46
    private let rowPadding: CGFloat = 10
    private let lineSpacing: CGFloat = 4

    public var body: some View {
        HStack(spacing: 12) {
            AvatarWithStatus(bot: bot, members: model.members(of: bot), size: avatar)
                .overlay(alignment: .bottomLeading) {
                    if let agent {
                        AgentBadge(registry: model.agentRegistry(agent), size: avatar * 0.4)
                    }
                }
            VStack(alignment: .leading, spacing: lineSpacing) {
                HStack(alignment: .firstTextBaseline) {
                    if bot.pinned {
                        Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.tertiary)
                    }
                    Text(bot.name)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    if model.screen?.agentBot == bot.id {
                        Image(systemName: "cursorarrow.motionlines")
                            .font(.caption)
                            .foregroundStyle(Palette.accent)
                            .accessibilityLabel("Using the computer")
                    }
                    Spacer(minLength: 8)
                    Text(RelativeTime.day(Date(milliseconds: bot.lastAt)))
                        .font(.subheadline)
                        .foregroundStyle(bot.unread > 0 ? Palette.text : Palette.tertiary)
                }
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    preview
                    Spacer(minLength: 4)
                    if bot.unread > 0 { UnreadBadge(count: bot.unread, waiting: bot.waiting > 0) }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, rowPadding)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// The most pressing thing (`Bot.rosterLine`): what needs the user, the chat's own activity, its work in the
    /// background, otherwise the last message (Grok Bot row behavior).
    @ViewBuilder private var preview: some View {
        switch bot.rosterLine {
        case .needsYou:
            Label {
                Text(L10n.tr(bot.needsYouLine))
            } icon: {
                ThinkingOrb(state: .listening, size: 16, color: Palette.warning)
            }
                .font(.subheadline)
                .foregroundStyle(Palette.warning)
                .lineLimit(1)
        case .activity:
            working(L10n.tr(bot.activity.isEmpty ? "Working…" : bot.activity))
        case .work:
            let work = WorkSections(bot.work)
            if work.working.isEmpty { line(work.counts(), color: Palette.secondary) } else { working(work.counts()) }
        case .lastMessage where bot.status == "error":
            line(MessagePreview.text(bot.lastMessage ?? "Something went wrong"), color: Palette.danger)
        case .lastMessage:
            line(MessagePreview.text(bot.lastMessage ?? (bot.isGroup ? "\(bot.members.count) bots" : "\(model.backendName(bot.backend)) · \(bot.managedWorkspace ? L10n.tr("Personal workspace") : bot.folderName)")),
                 color: Palette.secondary)
        }
    }

    private func working(_ text: String) -> some View {
        HStack(spacing: 6) {
            ThinkingOrb(size: 13, color: Palette.secondary)
            Text(text).lineLimit(1)
        }
        .font(.subheadline)
        .foregroundStyle(Palette.secondary)
    }

    private func line(_ text: String, color: Color) -> some View {
        Text(text).font(.subheadline).foregroundStyle(color).lineLimit(1)
    }
}

/// An unread count in an accent capsule: a bot's, or a folded section's. `waiting`: it counts a card waiting on
/// the user, so it takes the needs-you colour of the avatar badge.
public struct UnreadBadge: View {
    let count: Int
    let waiting: Bool

    public init(count: Int, waiting: Bool) {
        self.count = count
        self.waiting = waiting
    }

    public var body: some View {
        Text("\(count)")
            .font(.caption2.bold())
            .foregroundStyle(waiting ? Palette.onWarning : Palette.onAccent)
            .padding(.horizontal, 6)
            .frame(minWidth: 18, minHeight: 18)
            .background(waiting ? Palette.warning : Palette.accentFill, in: Capsule())
    }
}
