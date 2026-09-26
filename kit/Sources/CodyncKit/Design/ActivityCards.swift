import SwiftUI

public extension BotActivityPresentation {
    var orbState: ThinkingOrb.State? {
        switch phase {
        case .working: .working
        case .needsInput: .listening
        case .waiting: .connecting
        case .completed, .failed, .stale: nil
        }
    }

    var tint: Color {
        switch phase {
        case .needsInput: Color(light: 0x936000, dark: 0xECAF52)
        case .failed: Palette.danger
        case .completed: Palette.added
        case .stale, .waiting: Palette.secondary
        case .working: Palette.text
        }
    }
}

/// Shared by the Lock Screen Live Activity and the in-app gallery.
public struct BotActivityCard: View {
    let name: String
    let shape: String
    let color: String
    let state: BotActivityPresentation

    public init(bot: Bot, state: BotActivityPresentation) {
        self.init(name: bot.name, shape: bot.avatarShape, color: bot.avatarColor, state: state)
    }

    public init(name: String, shape: String, color: String, state: BotActivityPresentation) {
        self.name = name
        self.shape = shape
        self.color = color
        self.state = state
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                CharacterAvatar(shape: shape, color: color, size: 32)
                VStack(alignment: .leading, spacing: 2) {
                    Text(name).font(.system(size: 14, weight: .semibold)).foregroundStyle(Palette.text)
                    Text(state.title).font(.system(size: 11, weight: .medium)).foregroundStyle(state.tint)
                }
                .lineLimit(1)
                Spacer(minLength: 8)
                BotActivityIndicator(state: state)
            }
            Text(state.detail).font(.system(size: 12)).foregroundStyle(Palette.secondary)
                .lineLimit(2).fixedSize(horizontal: false, vertical: true)
            if state.phase == .working, let started = state.startedAt {
                HStack(spacing: 6) {
                    Image(systemName: "clock")
                    Text(started, style: .timer).monospacedDigit()
                    Text("elapsed")
                }
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(Palette.tertiary)
            }
        }
        .padding(16)
        .accessibilityElement(children: .combine)
    }
}

/// Compact trailing / minimal: errors and delayed updates never look complete.
public struct BotActivityIndicator: View {
    let state: BotActivityPresentation
    let minimal: Bool

    public init(state: BotActivityPresentation, minimal: Bool = false) {
        self.state = state
        self.minimal = minimal
    }

    public var body: some View {
        Group {
            if !minimal, state.showsTimer, let started = state.startedAt {
                Text(started, style: .timer).monospacedDigit()
                    .font(.system(size: 12, weight: .medium))
                    .multilineTextAlignment(.trailing).frame(width: 46)
                    .minimumScaleFactor(0.7)
            } else if let orb = state.orbState {
                ThinkingOrb(state: orb, size: minimal ? 22 : 20, color: state.tint, animated: false)
                    .accessibilityHidden(false).accessibilityLabel(state.title)
            } else {
                Image(systemName: state.symbol).font(.system(size: 13, weight: .semibold))
                    .accessibilityLabel(state.title)
            }
        }
        .foregroundStyle(state.tint)
    }
}

/// The content below the camera in the expanded Dynamic Island.
public struct BotActivityDetail: View {
    let state: BotActivityPresentation
    public init(state: BotActivityPresentation) { self.state = state }

    public var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Label {
                Text(state.title)
            } icon: {
                if let orb = state.orbState {
                    ThinkingOrb(state: orb, size: 16, color: state.tint, animated: false)
                } else {
                    Image(systemName: state.symbol)
                }
            }
                .font(.system(size: 11, weight: .semibold)).foregroundStyle(state.tint)
            Text(state.detail).font(.system(size: 12)).foregroundStyle(Palette.secondary)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// Illustrations for the gallery/exporter, using the same activity content views.
/// The real Dynamic Island's regions and camera cutout are laid out by iOS.
public struct BotActivityPreview: View {
    public enum Form: String, CaseIterable {
        case lockScreen = "Lock Screen", compact = "Compact", minimal = "Minimal", expanded = "Expanded"
    }
    let bot: Bot
    let state: BotActivityPresentation
    let form: Form

    public init(bot: Bot, state: BotActivityPresentation, form: Form) {
        self.bot = bot; self.state = state; self.form = form
    }

    public var body: some View {
        Group {
            if form == .lockScreen {
                BotActivityCard(bot: bot, state: state)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
            } else {
                island.environment(\.colorScheme, .dark)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(form.rawValue) preview")
    }

    @ViewBuilder private var island: some View {
        switch form {
        case .compact:
            HStack {
                CharacterAvatar(bot: bot, size: 20, animated: false)
                Spacer(minLength: 70)
                BotActivityIndicator(state: state)
            }
            .padding(.horizontal, 12).frame(width: 210, height: 38)
            .background(.black, in: Capsule())
        case .minimal:
            BotActivityIndicator(state: state, minimal: true)
                .frame(width: 38, height: 38).background(.black, in: Circle())
        case .expanded:
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    CharacterAvatar(bot: bot, size: 26, animated: false)
                    Text(bot.name).font(.system(size: 14, weight: .semibold)).foregroundStyle(.white).lineLimit(1)
                    Spacer()
                    BotActivityIndicator(state: state)
                }
                BotActivityDetail(state: state)
                Text(state.phase == .needsInput ? "Respond in Codync ↗" : "Open conversation ↗")
                    .font(.system(size: 12, weight: .medium)).foregroundStyle(.white)
            }
            .padding(18).background(.black, in: RoundedRectangle(cornerRadius: 28))
        case .lockScreen: EmptyView()
        }
    }
}
