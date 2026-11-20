import CodyncKit
import SwiftUI

/// A pull past the top of a chat: the arrow sweeps in with the pull and the refresh starts only
/// when the reader lets go of a full circle, so a long pull never refreshes (or snaps back) under the
/// finger. Only the indicator reads it, so a pull never re-renders the list.
@MainActor @Observable final class PullToRefresh {
    /// How far past the top a pull completes the circle.
    static let distance: CGFloat = 110
    /// Room kept open above the chat while it refreshes, for the indicator.
    static let hold: CGFloat = 52
    /// The chat easing back up once the refresh is done.
    static let settle = Animation.spring(duration: 0.5, bounce: 0)
    var progress: CGFloat = 0
    var refreshing = false
}

struct PullIndicator: View {
    let pull: PullToRefresh
    @State private var turning = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // The system's arrow, swept in from its tail as the pull grows.
        Image(systemName: "arrow.clockwise")
            .font(.system(size: 17, weight: .medium))
            .foregroundStyle(Palette.secondary)
            .mask { Sweep(progress: pull.refreshing ? 1 : pull.progress) }
            .rotationEffect(.degrees(turning ? 360 : 0))
            .animation(turning && !reduceMotion ? .linear(duration: 0.8).repeatForever(autoreverses: false) : nil,
                       value: turning)
            .padding(8)
            .opacity(pull.refreshing ? 1 : min(1, pull.progress * 2))
            .onChange(of: pull.refreshing) { _, refreshing in turning = refreshing }
            .accessibilityHidden(!pull.refreshing)
            .accessibilityLabel("Refreshing")
            .allowsHitTesting(false)
    }
}

/// A clockwise wedge from the arrow's tail (just above 3 o'clock), `progress` of the way round.
private struct Sweep: Shape {
    var progress: CGFloat

    func path(in rect: CGRect) -> Path {
        let center = CGPoint(x: rect.midX, y: rect.midY)
        var path = Path()
        path.move(to: center)
        path.addArc(center: center, radius: hypot(rect.width, rect.height), startAngle: .degrees(-15),
                    endAngle: .degrees(-15 + 360 * progress), clockwise: false)
        path.closeSubpath()
        return path
    }
}
