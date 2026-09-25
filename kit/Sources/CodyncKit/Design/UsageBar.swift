import SwiftUI

/// One place for "how full is this limit" colors, shared by the apps, the menu bar and widgets.
public extension Palette {
    /// Fill for bars: ink, amber from 70%, red from 90%.
    static func usageFill(_ percent: Double) -> Color {
        percent >= 90 ? danger : percent >= 70 ? warning : accentFill
    }

    /// Tint for gauges and progress views .
    static func usageTint(_ percent: Double) -> Color {
        percent >= 90 ? danger : percent >= 70 ? warning : accent
    }
}

public struct UsageBar: View {
    let percent: Double
    let height: CGFloat

    public init(percent: Double, height: CGFloat = 6) {
        self.percent = percent
        self.height = height
    }

    public var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.bubbleAgent)
                Capsule()
                    .fill(Palette.usageFill(percent))
                    .frame(width: geo.size.width * min(1, max(0.02, percent / 100)))
            }
        }
        .frame(height: height)
    }
}
