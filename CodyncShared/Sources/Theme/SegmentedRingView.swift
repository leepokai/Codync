import SwiftUI

/// Segmented progress ring showing individual task segments.
/// Each task is a separate arc with brightness indicating status.
/// Shared across iOS and macOS targets.
public struct SegmentedRingView: View {
    public let completedCount: Int
    public let totalCount: Int
    public let isWorking: Bool
    public let fg: Color
    public var lineWidth: CGFloat = 2
    public var gapDegrees: Double = 6

    public init(completedCount: Int, totalCount: Int, isWorking: Bool, fg: Color,
                lineWidth: CGFloat = 2, gapDegrees: Double = 6) {
        self.completedCount = completedCount
        self.totalCount = totalCount
        self.isWorking = isWorking
        self.fg = fg
        self.lineWidth = lineWidth
        self.gapDegrees = gapDegrees
    }

    public var body: some View {
        Canvas { context, size in
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            let radius = min(size.width, size.height) / 2 - lineWidth / 2
            let total = max(totalCount, 1)
            let totalGap = gapDegrees * Double(total)
            let segmentDegrees = (360.0 - totalGap) / Double(total)

            for i in 0..<total {
                let startAngle = -90.0 + Double(i) * (segmentDegrees + gapDegrees)
                let endAngle = startAngle + segmentDegrees

                let path = Path { p in
                    p.addArc(
                        center: center,
                        radius: radius,
                        startAngle: .degrees(startAngle),
                        endAngle: .degrees(endAngle),
                        clockwise: false
                    )
                }

                let opacity: Double
                if i < completedCount {
                    opacity = 0.8
                } else if i == completedCount && isWorking {
                    opacity = 0.4
                } else {
                    opacity = 0.12
                }

                context.stroke(
                    path,
                    with: .color(fg.opacity(opacity)),
                    style: StrokeStyle(lineWidth: lineWidth, lineCap: .round)
                )
            }
        }
    }
}
