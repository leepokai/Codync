import CodyncKit
import Foundation
import Observation

/// Whether a computer's screen shows: not asked for yet, showing, or paused after `idleLimit` with nobody touching
/// the app (a picture nobody watches still costs the computer's capture and the relay's traffic).
@MainActor @Observable final class ScreenPause {
    enum State {
        case start, watching, paused

        /// What the one button in place of the picture says; nil while watching.
        var prompt: String? {
            switch self {
            case .start: "Tap to see the screen"
            case .paused: "Paused. Tap to continue."
            case .watching: nil
            }
        }
    }

    /// How long a screen streams with nobody touching the app before it pauses.
    static let idleLimit = Duration.seconds(5 * 60)

    private(set) var state: State
    private let screen: AnalyticsEvent.Screen

    /// `screen`: which picture, for `remote_screen_paused`.
    init(_ screen: AnalyticsEvent.Screen, state: State = .start) {
        self.screen = screen
        self.state = state
    }

    /// The idle time left before a screen that went live at `start` pauses, counted from the later of that and the
    /// last touch; nil once it's up.
    static func idleLeft(lastTouch: Date, start: Date, now: Date) -> Duration? {
        let left = idleLimit - .seconds(now.timeIntervalSince(max(lastTouch, start)))
        return left > .zero ? left : nil
    }

    /// Start or continue.
    func play() {
        Motion.animate { state = .watching }
    }

    /// Ask first again (the tab left the screen, the app went to the background).
    func reset() {
        state = .start
    }

    /// Pauses once nobody has touched the app for `idleLimit`, counted from this call at the earliest. Run it while the
    /// picture is live (`.task(id:)`): connecting, reconnecting or failed doesn't count, and live again starts over.
    func pauseWhenIdle() async {
        let start = Date.now
        while let left = Self.idleLeft(lastTouch: Touches.last, start: start, now: .now) {
            guard (try? await Task.sleep(for: left)) != nil else { return }
        }
        Motion.animate { state = .paused }
        Analytics.shared.capture(.remoteScreenPaused(screen))
    }
}
