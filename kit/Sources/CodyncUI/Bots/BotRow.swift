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
            AvatarWithStatus(bot: bot, size: avatar)
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
            Label(bot.activity.isEmpty ? "Needs your approval" : bot.activity, systemImage: "hand.raised.fill")
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
            Label("Connecting to \(model.hostName)…", systemImage: "antenna.radiowaves.left.and.right")
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
        case let .offline(reason):
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "wifi.exclamationmark").foregroundStyle(Palette.warning)
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(model.hostName) is offline").font(.footnote.weight(.semibold)).foregroundStyle(Palette.text)
                    Text(reason).font(.footnote).foregroundStyle(Palette.secondary)
                    #if os(iOS)
                    if reason == HostError.unreachable.localizedDescription { tailscaleHint }
                    #endif
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

    /// Away from home the phone needs Tailscale: say whether it's off here or not set up at all.
    @ViewBuilder private var tailscaleHint: some View {
        if model.pairing?.urls.contains(where: Tailscale.isAddress) == true {
            if !Tailscale.isConnected {
                Text("Tailscale is off on this iPhone. Turn it on to reach \(model.hostName) away from home.")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Palette.warning)
            }
        } else {
            Link("Away from home? Set up Tailscale", destination: Tailscale.downloadURL)
                .font(.footnote.weight(.medium))
        }
    }
}
