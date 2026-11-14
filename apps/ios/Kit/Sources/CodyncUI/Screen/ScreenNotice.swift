import CodyncKit
import SwiftUI

/// What shows in place of the screen (the full viewer, the bot details' picture): what's wrong, or a
/// spinner while waiting, and a retry after a failure; or the one button that starts or continues it.
struct ScreenNotice: View {
    let icon: String?
    let text: String
    var retry: (() -> Void)?
    /// The whole area is one button, labeled with the text.
    var tap: (() -> Void)?
    @Environment(\.screenNoticeCompact) private var compact

    var body: some View {
        if let tap {
            Button(action: tap) {
                notice.frame(maxWidth: .infinity, maxHeight: .infinity).contentShape(Rectangle())
            }
            .buttonStyle(PressScale())
            .accessibilityLabel(L10n.tr(text))
        } else {
            notice
        }
    }

    private var notice: some View {
        VStack(spacing: compact ? 8 : 12) {
            if let icon, tap != nil {
                Image(systemName: icon).font(.system(size: compact ? 30 : 44)).foregroundStyle(.primary)
            } else if let icon {
                Image(systemName: icon).font(compact ? .title2 : .largeTitle).foregroundStyle(.secondary)
            } else {
                Spinner(size: compact ? 18 : 22)
            }
            Text(L10n.tr(text)).font(compact ? .footnote : .callout).multilineTextAlignment(.center).foregroundStyle(.secondary)
            if let retry {
                Button(L10n.tr("Try again"), systemImage: "arrow.clockwise", action: retry)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.secondary)
            }
        }
        .padding(compact ? 16 : 32)
        .frame(maxWidth: 420)
    }
}

extension ScreenNotice {
    /// Why the computer's screen can't show here, or nil when it can. A cloud computer asleep wakes
    /// because this app connects to it.
    init?(_ model: BotStore) {
        if model.isAsleep {
            self.init(icon: nil, text: "Waking the computer…")
        } else if model.connection == .connecting {
            self.init(icon: nil, text: "Connecting to the screen…")
        } else if model.connection != .online {
            self.init(icon: "wifi.slash", text: "This computer is offline.")
        } else if let screen = model.screen {
            guard !screen.available else { return nil }
            self.init(icon: "lock.display", text: screen.accessGuidance)
        } else {
            self.init(icon: "arrow.down.circle", text: "Update Codync on your computer to use its screen from here.")
        }
    }

    /// Start or continue the screen; nil while it's wanted.
    init?(_ pause: ScreenPause) {
        guard let prompt = pause.state.prompt else { return nil }
        self.init(icon: "play.circle", text: prompt, tap: pause.play)
    }

    /// A session that isn't showing the screen yet: connecting, reconnecting, or failed with a retry.
    init?(_ session: ScreenSession) {
        switch session.phase {
        case .connecting: self.init(icon: nil, text: "Connecting to the screen…")
        case .reconnecting: self.init(icon: nil, text: "Reconnecting…")
        case let .failed(message): self.init(icon: "exclamationmark.triangle", text: message) { Task { await session.start() } }
        case .live, .closed: return nil
        }
    }
}

extension EnvironmentValues {
    /// Notices sized for a small picture rather than the whole screen.
    @Entry var screenNoticeCompact = false
}

/// "{name} is using the computer", over the screen.
struct AgentAtScreen: View {
    let name: String

    var body: some View {
        Label(L10n.fill(L10n.tr("{name} is using the computer"), ["name": name]), systemImage: "cursorarrow.motionlines")
            .font(.caption.weight(.medium))
            .lineLimit(1)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.ultraThinMaterial, in: Capsule())
    }
}
