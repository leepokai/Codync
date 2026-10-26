import CodyncKit
import SwiftUI

public struct BotRow: View {
    let bot: Bot
    @Environment(BotStore.self) private var model

    public init(bot: Bot) { self.bot = bot }

    private let avatar: CGFloat = 46
    private let rowPadding: CGFloat = 10
    private let lineSpacing: CGFloat = 4

    public var body: some View {
        HStack(spacing: 12) {
            AvatarWithStatus(bot: bot, members: model.members(of: bot), size: avatar)
            VStack(alignment: .leading, spacing: lineSpacing) {
                HStack(alignment: .firstTextBaseline) {
                    if bot.pinned && !bot.lead {
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
                    if bot.unread > 0 {
                        Text(L10n.tr("\(bot.unread)"))
                            .font(.caption2.bold())
                            .foregroundStyle(Palette.onAccent)
                            .padding(.horizontal, 6)
                            .frame(minWidth: 18, minHeight: 18)
                            .background(Palette.accentFill, in: Capsule())
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, rowPadding)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// Live activity while working, otherwise the last message (Grok Bot row behavior).
    @ViewBuilder private var preview: some View {
        if bot.needsInput {
            Label {
                Text(L10n.tr(bot.activity.isEmpty ? "Needs your approval" : bot.activity))
            } icon: {
                ThinkingOrb(state: .listening, size: 16, color: Palette.warning)
            }
                .font(.subheadline)
                .foregroundStyle(Palette.warning)
                .lineLimit(1)
        } else if bot.isWorking {
            HStack(spacing: 6) {
                ThinkingOrb(size: 13, color: Palette.secondary)
                Text(L10n.tr(bot.activity.isEmpty ? "Working…" : bot.activity))
                    .lineLimit(1)
            }
            .font(.subheadline)
            .foregroundStyle(Palette.secondary)
        } else if bot.status == "error" {
            Text(MessagePreview.text(bot.lastMessage ?? "Something went wrong"))
                .font(.subheadline)
                .foregroundStyle(Palette.danger)
                .lineLimit(1)
        } else {
            Text(MessagePreview.text(bot.lastMessage ?? (bot.isGroup ? "\(bot.members.count) bots" : "\(model.backendName(bot.backend)) · \(bot.managedWorkspace ? L10n.tr("Personal workspace") : bot.folderName)")))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
        }
    }
}
