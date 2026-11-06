import ActivityKit
import CodyncKit
import SwiftUI
import WidgetKit

/// The bot's Live Activity while this phone is on a call with it: the bot, the call's length and its two buttons
/// (mute, end), which work without opening the app (`MuteCallIntent`, `EndCallIntent`).
@MainActor
struct CallActivityViews {
    let context: ActivityViewContext<BotActivityAttributes>
    let state: BotActivityPresentation

    private var since: Date { context.state.callStartedAt ?? .now }
    private var muted: Bool { context.state.callMuted == true }

    func avatar(_ size: CGFloat) -> some View {
        ActivityAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, state: state, size: size)
    }

    func timer(size: CGFloat) -> some View {
        Text(since, style: .timer)
            .font(.system(size: size).monospacedDigit())
            .foregroundStyle(Palette.secondary)
            .lineLimit(1)
    }

    /// The call's length over the bot's name.
    var title: some View {
        VStack(alignment: .leading, spacing: 1) {
            timer(size: 14)
            Text(context.attributes.name).font(.system(size: 20, weight: .semibold)).foregroundStyle(Palette.text).lineLimit(1)
        }
    }

    /// Mute and end, round. `fill` is the mute button's ground (the island is always dark; the Lock Screen follows
    /// the system's appearance).
    func buttons(fill: Color, size: CGFloat = 44) -> some View {
        HStack(spacing: 10) {
            Button(intent: MuteCallIntent(botId: context.attributes.botId)) {
                Image(systemName: muted ? "mic.slash" : "mic")
                    .font(.system(size: size * 0.42, weight: .medium))
                    .foregroundStyle(muted ? Palette.onAccent : Palette.text)
                    .frame(width: size, height: size)
                    .background(muted ? Palette.accentFill : fill, in: Circle())
            }
            .accessibilityLabel(muted ? "Unmute" : "Mute")
            Button(intent: EndCallIntent(botId: context.attributes.botId)) {
                Image(systemName: "xmark")
                    .font(.system(size: size * 0.42, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: size, height: size)
                    .background(Palette.danger, in: Circle())
            }
            .accessibilityLabel("End call")
        }
        .buttonStyle(.plain)
    }

    /// The island: expanded, the bot and the call's length on one side and the buttons on the other; compact, the
    /// avatar and the length; minimal, the avatar.
    var island: DynamicIsland {
        DynamicIsland {
            DynamicIslandExpandedRegion(.leading) {
                HStack(spacing: 12) {
                    avatar(44)
                    title
                }
                .frame(maxHeight: .infinity)
                .dynamicIsland(verticalPlacement: .belowIfTooWide)
            }
            DynamicIslandExpandedRegion(.trailing) {
                buttons(fill: .white.opacity(0.18)).frame(maxHeight: .infinity)
            }
        } compactLeading: {
            avatar(20)
        } compactTrailing: {
            timer(size: 14).frame(maxWidth: 44)
        } minimal: {
            avatar(20).accessibilityLabel("\(context.attributes.name), \(state.title)")
        }
        .keylineTint(state.tint)
        .widgetURL(context.attributes.link ?? AppEnvironment.current.link("computers"))
    }

    /// The Lock Screen and banner card: the bot, the call, its buttons, and what the bot is doing.
    var card: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                avatar(44)
                title
                Spacer(minLength: 8)
                buttons(fill: Palette.text.opacity(0.12))
            }
            Text(context.state.activity.isEmpty ? L10n.tr(state.title) : context.state.activity)
                .font(.system(size: 13))
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
                .contentTransition(.interpolate)
        }
        .padding(16)
    }
}
