import CodyncKit
import SwiftUI

public struct BotRow: View {
    let bot: Bot
    let compact: Bool
    @Environment(BotStore.self) private var model

    public init(bot: Bot, compact: Bool = false) {
        self.bot = bot
        self.compact = compact
    }

    // A Mac sidebar row is denser than a phone row (Grok Bot's desktop sidebar).
    #if os(macOS)
    private let avatar: CGFloat = 30
    private let rowPadding: CGFloat = 6
    private let lineSpacing: CGFloat = 1
    #else
    private let avatar: CGFloat = 46
    private let rowPadding: CGFloat = 10
    private let lineSpacing: CGFloat = 4
    #endif

    public var body: some View {
        HStack(spacing: compact ? 0 : InterfaceMetrics.value(mac: 8, mobile: 12)) {
            AvatarWithStatus(bot: bot, members: model.members(of: bot), size: avatar)
            VStack(alignment: .leading, spacing: lineSpacing) {
                HStack(alignment: .firstTextBaseline) {
                    if bot.pinned {
                        Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.tertiary)
                    }
                    Text(bot.name)
                        .font(InterfaceMetrics.body.weight(.semibold))
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    if model.screen?.agentBot == bot.id {
                        Image(systemName: "cursorarrow.motionlines")
                            .font(.caption)
                            .foregroundStyle(Palette.accent)
                            .accessibilityLabel("Using the computer")
                    }
                    Spacer(minLength: 8)
                    #if os(iOS)
                    Text(RelativeTime.day(Date(milliseconds: bot.lastAt)))
                        .font(InterfaceMetrics.secondary)
                        .foregroundStyle(bot.unread > 0 ? Palette.text : Palette.tertiary)
                    #endif
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
            .frame(width: compact ? 0 : nil, alignment: .leading)
            .opacity(compact ? 0 : 1)
            .clipped()
            .accessibilityHidden(compact)
        }
        .frame(maxWidth: .infinity, alignment: compact ? .center : .leading)
        .padding(.vertical, rowPadding)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// Live activity while working, otherwise the last message (Grok Bot row behavior).
    @ViewBuilder private var preview: some View {
        if bot.needsInput {
            Label {
                Text(bot.activity.isEmpty ? "Needs your approval" : bot.activity)
            } icon: {
                ThinkingOrb(state: .listening, size: 16, color: Palette.warning)
            }
                .font(InterfaceMetrics.secondary)
                .foregroundStyle(Palette.warning)
                .lineLimit(1)
        } else if bot.isWorking {
            HStack(spacing: 6) {
                ThinkingOrb(size: 13, color: Palette.secondary)
                Text(bot.activity.isEmpty ? "Working…" : bot.activity)
                    .lineLimit(1)
            }
            .font(InterfaceMetrics.secondary)
            .foregroundStyle(Palette.secondary)
        } else if bot.status == "error" {
            Text(bot.lastMessage ?? "Something went wrong")
                .font(InterfaceMetrics.secondary)
                .foregroundStyle(Palette.danger)
                .lineLimit(1)
        } else {
            Text(bot.lastMessage ?? "\(model.backendName(bot.backend)) · \(bot.folderName)")
                .font(InterfaceMetrics.secondary)
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
            EmptyView()
        case let .computerOffline(lastSeen):
            let seen = lastSeen.map { " Last seen \(RelativeTime.day($0))." } ?? ""
            banner(icon: "moon.zzz", title: "\(model.hostName) is offline",
                   detail: (model.canQueue ? "Messages wait and go out when it's back online." : "It's asleep or turned off.") + seen,
                   retry: false)
        case let .offline(reason):
            banner(icon: "wifi.exclamationmark", title: "Can't reach \(model.hostName)", detail: reason, retry: true)
        case let .unauthorized(reason):
            banner(icon: "lock.slash", title: "No access to \(model.hostName)", detail: reason, retry: false)
        }
    }

    private func banner(icon: String, title: String, detail: String, retry: Bool) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon).foregroundStyle(Palette.warning)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.footnote.weight(.semibold)).foregroundStyle(Palette.text)
                Text(detail).font(.footnote).foregroundStyle(Palette.secondary)
            }
            Spacer()
            if retry {
                IconButton("Retry", systemImage: "arrow.clockwise") { model.restartStream() }
            }
        }
        .padding(10)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 10))
    }
}
