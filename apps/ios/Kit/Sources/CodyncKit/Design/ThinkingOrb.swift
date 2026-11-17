import SwiftUI

/// The halftone thinking orb: what a bot is doing, as light flowing over a fixed dot grid.
/// Supply an adjacent status label; this decorative view is hidden from VoiceOver.
public struct ThinkingOrb: View {
    public enum State: String, CaseIterable, Sendable {
        case working, searching, listening, connecting, shaping
    }

    let state: State
    let size: CGFloat
    let color: Color
    let animated: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @SwiftUI.State private var visible = false
    @SwiftUI.State private var epoch = Date.now

    public init(state: State = .working, size: CGFloat = 16, color: Color = Palette.text, animated: Bool = true) {
        self.state = state
        self.size = size
        self.color = color
        self.animated = animated
    }

    public var body: some View {
        let running = animated && !reduceMotion && visible && scenePhase == .active
        Group {
            if running {
                TimelineView(.animation(minimumInterval: 1.0 / 30)) { timeline in
                    drawing(time: ThinkingOrbGeometry.rest(state) + timeline.date.timeIntervalSince(epoch))
                }
            } else {
                drawing(time: ThinkingOrbGeometry.rest(state))
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
        .onAppear { epoch = .now; visible = true }
        .onDisappear { visible = false }
        .onChange(of: running) { _, isRunning in if isRunning { epoch = .now } }
    }

    private func drawing(time: Double) -> some View {
        let dots = ThinkingOrbGeometry.frame(state: state, size: size, time: time)
        return Canvas { context, _ in
            // Tintable ink, so it blends with cards, wallpaper and accessory rendering.
            for dot in dots {
                let rect = CGRect(x: dot.x - dot.radius, y: dot.y - dot.radius, width: dot.radius * 2, height: dot.radius * 2)
                context.fill(Path(ellipseIn: rect), with: .color(color.opacity(dot.alpha)))
            }
        }
    }
}
