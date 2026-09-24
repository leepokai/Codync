import SwiftUI

/// The app's busy indicator: a small dotted sphere turning in place, after
/// thinking-orbs' "working" orb. Depth is carried by dot size and opacity.
public struct ThinkingOrb: View {
    let size: CGFloat
    let color: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 16, color: Color = Palette.text) {
        self.size = size
        self.color = color
    }

    public var body: some View {
        let lattice = Self.lattice(size < 20 ? 60 : 140)
        TimelineView(.animation(paused: reduceMotion)) { timeline in
            let t = reduceMotion ? 0.6 : timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 3600)
            Canvas { ctx, canvas in
                let c = canvas.width / 2, r = c * 0.84
                let (sy, cy) = (sin(t * 0.9), cos(t * 0.9))
                let (st, ct) = (sin(0.35), cos(0.35))
                let dots = lattice.map { p -> (CGFloat, CGFloat, Double) in
                    let x1 = p.x * cy + p.z * sy, z1 = -p.x * sy + p.z * cy
                    let y1 = p.y * ct - z1 * st, z2 = p.y * st + z1 * ct
                    return (c + x1 * r, c - y1 * r, z2)
                }.sorted { $0.2 < $1.2 }
                for (x, y, z) in dots {
                    let depth = (z + 1) / 2
                    let d = canvas.width * (0.035 + 0.075 * depth)
                    ctx.fill(Path(ellipseIn: CGRect(x: x - d / 2, y: y - d / 2, width: d, height: d)),
                             with: .color(color.opacity(0.12 + 0.88 * depth)))
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    /// Evenly spread directions on a unit sphere (Fibonacci lattice).
    private static func lattice(_ n: Int) -> [(x: Double, y: Double, z: Double)] {
        let golden = Double.pi * (3 - 5.0.squareRoot())
        return (0..<n).map { i in
            let y = 1 - 2 * (Double(i) + 0.5) / Double(n), rad = (1 - y * y).squareRoot(), a = Double(i) * golden
            return (rad * cos(a), y, rad * sin(a))
        }
    }
}
