import CodyncKit
import SwiftUI

public struct BotRow: View {
    let bot: Bot
    @Environment(BotStore.self) private var model

    public init(bot: Bot) { self.bot = bot }

    public var body: some View {
        HStack(spacing: 14) {
            AvatarWithStatus(bot: bot, size: 46)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline) {
                    if bot.pinned {
                        Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.tertiary)
                    }
                    Text(bot.name)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(RelativeTime.day(Date(milliseconds: bot.lastAt)))
                        .font(.subheadline)
                        .foregroundStyle(bot.unread > 0 ? Palette.text : Palette.tertiary)
                }
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    preview
                    Spacer(minLength: 4)
                    if bot.unread > 0 {
                        Text("\(bot.unread)")
                            .font(.caption2.bold())
                            .foregroundStyle(Palette.onAccent)
                            .padding(.horizontal, 6)
                            .frame(minWidth: 18, minHeight: 18)
                            .background(Palette.accentFill, in: Capsule())
                    }
                }
            }
        }
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// Live activity while working, otherwise the last message (Grok Bot row behavior).
    @ViewBuilder private var preview: some View {
        if bot.needsInput {
            Label(bot.activity.isEmpty ? "Needs your approval" : bot.activity, systemImage: "hand.raised.fill")
                .font(.subheadline)
                .foregroundStyle(Palette.warning)
                .lineLimit(1)
        } else if bot.isWorking {
            HStack(spacing: 6) {
                ThinkingOrb(size: 13, color: Palette.secondary)
                Text(bot.activity.isEmpty ? "Working…" : bot.activity)
                    .lineLimit(1)
            }
            .font(.subheadline)
            .foregroundStyle(Palette.secondary)
        } else if bot.status == "error" {
            Text(bot.lastMessage ?? "Something went wrong")
                .font(.subheadline)
                .foregroundStyle(Palette.danger)
                .lineLimit(1)
        } else {
            Text(bot.lastMessage ?? "\(model.backendName(bot.backend)) · \(bot.folderName)")
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
        }
    }
}

public struct ConnectionBanner: View {
    @Environment(BotStore.self) private var model

    public init() {}

    public var body: some View {
        switch model.connection {
        case .online, .unpaired:
            EmptyView()
        case .connecting:
            Label("Connecting to \(model.hostName)…", systemImage: "antenna.radiowaves.left.and.right")
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
        case let .offline(reason):
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "wifi.exclamationmark").foregroundStyle(Palette.warning)
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(model.hostName) is offline").font(.footnote.weight(.semibold)).foregroundStyle(Palette.text)
                    Text(reason).font(.footnote).foregroundStyle(Palette.secondary)
                }
                Spacer()
                Button("Retry", systemImage: "arrow.clockwise") { model.restartStream() }
                    .labelStyle(.iconOnly)
                    .help("Retry")
            }
            .padding(10)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 10))
        }
    }
}
