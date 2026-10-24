import SwiftUI

public extension BotActivityPresentation {
    var orbState: ThinkingOrb.State? {
        switch phase {
        case .working: .working
        case .needsInput: .listening
        case .sending, .waiting, .stale: .connecting
        case .queued, .completed, .failed: nil
        }
    }

    var tint: Color {
        switch phase {
        case .needsInput: Palette.attention
        case .failed: Palette.danger
        case .completed: Palette.added
        case .sending, .queued, .stale, .waiting: Palette.secondary
        case .working: Palette.text
        }
    }

    /// The caption stays quiet while the bot works and takes the state's color otherwise.
    var captionTint: Color { phase == .working ? Palette.secondary : tint }

    /// How many of the progress line's four segments are filled, as on Android's Live update.
    var progressStep: Int {
        switch phase {
        case .sending, .queued: 1
        case .working, .stale, .waiting: 2
        case .needsInput: 3
        case .completed, .failed: 4
        }
    }

    var mood: CharacterAvatar.Mood {
        switch phase {
        case .working: .working
        case .needsInput: .needsInput
        case .sending, .queued, .completed, .failed, .stale, .waiting: .idle
        }
    }

    /// The elapsed time counts up on its own (no push) while the task is live.
    func elapsedStart(_ startedAt: Date?) -> Date? {
        phase == .working || phase == .needsInput ? startedAt : nil
    }
}

public extension AnyTransition {
    /// Rows a new phase brings (the Review button, the timer): rise and fade in.
    static var activityRow: AnyTransition {
        .asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity)
    }
}

/// The bot's face in its phase's mood. On a phase change the bot itself moves: the halftone
/// highlight sweeps to the new side and the eyes glance over (or the ripple appears).
public struct ActivityAvatar: View {
    let shape: String
    let color: String
    let state: BotActivityPresentation
    let size: CGFloat

    public init(shape: String, color: String, state: BotActivityPresentation, size: CGFloat) {
        self.shape = shape; self.color = color; self.state = state; self.size = size
    }

    public var body: some View {
        CharacterAvatar(shape: shape, color: color, size: size, mood: state.mood, still: true)
            .animation(Motion.activityPhase, value: state.mood)
    }
}

/// The step (or state) in its color, then the running time.
public struct ActivityCaption: View {
    let state: BotActivityPresentation
    let startedAt: Date?

    public init(state: BotActivityPresentation, startedAt: Date?) {
        self.state = state; self.startedAt = startedAt
    }

    public var body: some View {
        HStack(spacing: 4) {
            Text(L10n.tr(state.caption))
                .foregroundStyle(state.captionTint)
                .contentTransition(.interpolate)
                .layoutPriority(1)
            // No fixedSize on a timer Text: a Live Activity can't lay one out at its ideal width
            // and draws the whole card black. Unconstrained, it takes the width left over.
            if let since = state.elapsedStart(startedAt) {
                Text(L10n.tr("·")).foregroundStyle(Palette.tertiary)
                Text(since, style: .timer).monospacedDigit().foregroundStyle(Palette.secondary)
            }
        }
        .font(.system(size: 12))
        .lineLimit(1)
    }
}

/// The task's progress: four segments that fill as it moves, in the state's color.
public struct ActivityProgress: View {
    let state: BotActivityPresentation

    public init(state: BotActivityPresentation) { self.state = state }

    public var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<4, id: \.self) { index in
                Capsule().fill(index < state.progressStep ? state.tint : state.tint.opacity(0.16))
            }
        }
        .frame(height: 6)
    }
}

/// Opens the conversation on the request; approval itself stays on the permission card.
public struct ActivityReviewButton: View {
    let state: BotActivityPresentation
    let link: URL?

    public init(state: BotActivityPresentation, link: URL?) {
        self.state = state; self.link = link
    }

    public var body: some View {
        if let link { Link(destination: link) { label } } else { label }
    }

    private var label: some View {
        Label(L10n.tr("Review"), systemImage: "hand.raised.fill")
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Palette.onAccent)
            .frame(maxWidth: .infinity, minHeight: 34)
            .background(state.tint, in: Capsule())
    }
}

/// Shared by the Lock Screen Live Activity and the in-app gallery. The Live Activity passes its
/// own face and mark (the working loops drawn in timer fonts); the gallery uses the still ones.
public struct BotActivityCard<Face: View, Mark: View>: View {
    let name: String
    let state: BotActivityPresentation
    let startedAt: Date?
    let link: URL?
    let face: Face
    let mark: Mark

    public init(name: String, state: BotActivityPresentation, startedAt: Date? = nil, link: URL? = nil,
                @ViewBuilder face: () -> Face, @ViewBuilder mark: () -> Mark) {
        self.name = name
        self.state = state
        self.startedAt = startedAt
        self.link = link
        self.face = face()
        self.mark = mark()
    }

    public var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 12) {
                face
                VStack(alignment: .leading, spacing: 2) {
                    Text(name).font(.system(size: 14, weight: .semibold)).foregroundStyle(Palette.text).lineLimit(1)
                    ActivityCaption(state: state, startedAt: startedAt)
                }
                Spacer(minLength: 8)
                mark
            }
            ActivityProgress(state: state)
            if state.phase == .needsInput {
                ActivityReviewButton(state: state, link: link).transition(.activityRow)
            }
        }
        .padding(16)
        .animation(Motion.activityPhase, value: state)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(name), \(state.title). \(state.caption). \(L10n.tr("Progress, step \(state.progressStep) of 4"))")
    }
}

public extension BotActivityCard where Face == ActivityAvatar, Mark == BotActivityIndicator {
    init(bot: Bot, state: BotActivityPresentation, startedAt: Date? = nil) {
        self.init(name: widgetName(bot), state: state, startedAt: startedAt) {
            ActivityAvatar(shape: bot.avatarShape, color: bot.avatarColor, state: state, size: 32)
        } mark: {
            BotActivityIndicator(state: state, size: 28)
        }
    }
}

/// The state mark: a thinking orb while the task is live, a symbol once it has ended
/// (errors and delayed updates never look complete).
public struct BotActivityIndicator: View {
    let state: BotActivityPresentation
    let size: CGFloat

    public init(state: BotActivityPresentation, size: CGFloat = 20) {
        self.state = state
        self.size = size
    }

    public var body: some View {
        // The orb and the end symbols cross-fade; the bot beside it carries the motion.
        ZStack {
            Group {
                if let orb = state.orbState {
                    ThinkingOrb(state: orb, size: size, color: state.tint, animated: false)
                } else {
                    Image(systemName: state.symbol).font(.system(size: size * 0.6, weight: .semibold))
                }
            }
            .id(state.phase)
            .transition(.opacity)
        }
        .frame(width: size, height: size)
        .foregroundStyle(state.tint)
        .animation(Motion.activityPhase, value: state.phase)
        .accessibilityElement()
        .accessibilityLabel(state.title)
    }
}

/// The expanded island's bottom row: Review while the bot needs you, else the running time.
public struct ActivityIslandFooter: View {
    let state: BotActivityPresentation
    let startedAt: Date?
    let link: URL?

    public init(state: BotActivityPresentation, startedAt: Date?, link: URL?) {
        self.state = state; self.startedAt = startedAt; self.link = link
    }

    public var body: some View {
        Group {
            if state.phase == .needsInput {
                ActivityReviewButton(state: state, link: link)
            } else if let since = state.elapsedStart(startedAt) {
                // The timer alone, centered on its own: a timer Text reserves its widest width,
                // so beside an icon (or interpolated with one) it sat off center.
                Text(since, style: .timer)
                    .font(.system(size: 13, weight: .medium))
                    .monospacedDigit()
                    .foregroundStyle(Palette.tertiary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
            }
        }
        .transition(.activityRow)
        .animation(Motion.activityPhase, value: state)
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
    let startedAt: Date?

    public init(bot: Bot, state: BotActivityPresentation, form: Form, startedAt: Date? = nil) {
        self.bot = bot; self.state = state; self.form = form; self.startedAt = startedAt
    }

    public var body: some View {
        Group {
            if form == .lockScreen {
                BotActivityCard(bot: bot, state: state, startedAt: startedAt)
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
                islandFace(20)
                Spacer(minLength: 70)
                BotActivityIndicator(state: state)
            }
            .padding(.horizontal, 12).frame(width: 210, height: 38)
            .background(.black, in: Capsule())
        case .minimal:
            BotActivityIndicator(state: state)
                .frame(width: 38, height: 38).background(.black, in: Circle())
        case .expanded:
            VStack(spacing: 12) {
                HStack(spacing: 10) {
                    islandFace(26)
                    VStack(spacing: 2) {
                        Text(widgetName(bot)).font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                        Text(L10n.tr(state.caption)).font(.system(size: 12)).foregroundStyle(state.captionTint)
                    }
                    .lineLimit(1)
                    .frame(maxWidth: .infinity)
                    BotActivityIndicator(state: state, size: 28)
                }
                ActivityIslandFooter(state: state, startedAt: startedAt, link: nil)
            }
            .padding(18).background(.black, in: RoundedRectangle(cornerRadius: 28))
        case .lockScreen: EmptyView()
        }
    }

    /// The island plays the working bot's loop (WorkingBotFace in the widget); here the live
    /// avatar stands in for it.
    @ViewBuilder private func islandFace(_ size: CGFloat) -> some View {
        if state.phase == .working {
            CharacterAvatar(shape: bot.avatarShape, color: bot.avatarColor, size: size, mood: .working)
        } else {
            ActivityAvatar(shape: bot.avatarShape, color: bot.avatarColor, state: state, size: size)
        }
    }
}
