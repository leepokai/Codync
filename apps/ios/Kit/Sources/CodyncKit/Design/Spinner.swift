import SwiftUI

/// Loading indicator: a thin arc that turns.
public struct Spinner: View {
    var size: CGFloat
    @State private var turning = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 14) { self.size = size }

    public var body: some View {
        Circle()
            .trim(from: 0, to: 0.7)
            .stroke(Palette.secondary, style: StrokeStyle(lineWidth: max(1.5, size / 9), lineCap: .round))
            .frame(width: size, height: size)
            .rotationEffect(.degrees(turning ? 360 : 0))
            .animation(reduceMotion ? nil : .linear(duration: 0.8).repeatForever(autoreverses: false), value: turning)
            .onAppear { turning = true }
            .accessibilityLabel(L10n.tr("Loading"))
    }
}
