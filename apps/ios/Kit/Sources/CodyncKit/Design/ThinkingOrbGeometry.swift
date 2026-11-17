import Foundation

// The thinking orb: a halftone disc on the avatars' kind of dot grid. Dots never move; each state is
// a field that sets how full every dot is, so the light flows across the grid. Line for line with the
// desktop's lib/thinking-orb.ts and Android's ThinkingOrbGeometry.kt; all three are checked against
// Tests/CodyncKitTests/Fixtures/thinking-orb-frames.json. Independent of SwiftUI so tests can run it.
enum ThinkingOrbGeometry {
    struct Dot: Equatable {
        var x, y, radius, alpha: Double
    }

    /// One step coarser than the avatars' grid: a status glyph sits beside text.
    static func cells(size: Double) -> Int { size < 18 ? 5 : size < 30 ? 7 : 9 }

    /// The pose a still orb shows (widgets, Live Activity, Reduce Motion) and a moving one starts from,
    /// in seconds into its animation.
    static func rest(_ state: ThinkingOrb.State) -> Double {
        switch state {
        case .working, .searching: 0.45
        case .listening: 0.55
        case .connecting: 0.5
        case .shaping: 1.9
        }
    }

    /// Every dot of the orb in a `size` box at `time` seconds: the grid cropped to a circle.
    static func frame(state: ThinkingOrb.State, size: Double, time: Double) -> [Dot] {
        guard size.isFinite, size > 0, time.isFinite else { return [] }
        let n = cells(size: size), half = Double(n) / 2, pitch = size / Double(n)
        var dots: [Dot] = []
        for row in 0..<n {
            for col in 0..<n {
                let u = (Double(col) + 0.5 - half) / half, w = (Double(row) + 0.5 - half) / half
                guard hypot(u, w) <= 1.0001 else { continue }
                let v = clamp01(fill(state, u, w, time))
                // A full dot nearly touches its neighbors; an empty one stays a faint grain so the disc keeps its outline.
                dots.append(Dot(x: (Double(col) + 0.5) * pitch, y: (Double(row) + 0.5) * pitch,
                                radius: pitch * (0.14 + 0.36 * v), alpha: 0.5 + 0.5 * v))
            }
        }
        return dots
    }

    /// The working orb the Dynamic Island plays one frame per second (tools/timer-fonts.py):
    /// ten frames, one turn of the light, so the loop closes on itself.
    static func workingLoop(size: Double) -> [[Dot]] {
        (0..<10).map { frame(state: .working, size: size, time: Double($0) * 0.2) }
    }

    /// How full the dot at (`u`, `w`) on the unit disc is (0…1), `t` seconds in.
    private static func fill(_ state: ThinkingOrb.State, _ u: Double, _ w: Double, _ t: Double) -> Double {
        switch state {
        case .working:
            // A halftone-shaded ball whose light circles it clockwise every 2 s.
            let a = t * 2 * .pi / 2 - 3 * .pi / 4
            let nz = sqrt(max(0, 1 - u * u - w * w))
            return smooth((0.85 * (u * cos(a) + w * sin(a)) + 0.53 * nz - 0.3) / 0.62)
        case .searching:
            // A vertical beam sweeping left and right.
            let x = (pingPong(t / 2.4) * 2 - 1) * 0.85
            return gauss(u - x, 0.25)
        case .listening:
            // Rings opening from the center, two at a time.
            let rho = hypot(u, w)
            func ring(_ k: Double) -> Double {
                let phase = fract(t / 1.9 + k * 0.5)
                return gauss(rho - phase * 1.1, 0.22) * pow(1 - phase, 0.45)
            }
            return max(ring(0), ring(1))
        case .connecting:
            // Two corners lit, a pulse running between them.
            let m = pingPong(t / 1.8), ax = -0.48, ay = 0.48, bx = 0.48, by = -0.48
            let px = lerp(ax, bx, m), py = lerp(ay, by, m)
            return max(0.75 * gauss(hypot(u - ax, w - ay), 0.24), 0.75 * gauss(hypot(u - bx, w - by), 0.24),
                       gauss(hypot(u - px, w - py), 0.26))
        case .shaping:
            // A filled circle, triangle and square in turn, melting into each other.
            let hold = 1.1, change = 0.6, segment = hold + change
            let cycle = fract(t / (segment * 3)) * segment * 3
            let k = min(2, Int(cycle / segment)), local = cycle - Double(k) * segment
            let m = local > hold ? smooth((local - hold) / change) : 0
            let d = lerp(outlines[k](u, w), outlines[(k + 1) % 3](u, w), m)
            return 1 - smooth((d + 0.1) / 0.22)
        }
    }

    // Signed distances on the unit disc (negative inside, y down).
    private static let outlines: [@Sendable (Double, Double) -> Double] = [
        { u, w in hypot(u, w) - 0.62 },
        { u, w in
            // An equilateral triangle pointing up, centered on its centroid.
            let r = 0.62, k = sqrt(3.0)
            var x = abs(u) - r, y = -(w - 0.04) + r / k
            if x + k * y > 0 { (x, y) = ((x - k * y) / 2, (-k * x - y) / 2) }
            x -= max(-2 * r, min(0, x))
            return -hypot(x, y) * (y > 0 ? 1 : y < 0 ? -1 : 0)
        },
        { u, w in
            let dx = abs(u) - 0.5, dy = abs(w) - 0.5
            return hypot(max(dx, 0), max(dy, 0)) + min(max(dx, dy), 0)
        },
    ]

    private static func clamp01(_ v: Double) -> Double { min(1, max(0, v)) }
    private static func smooth(_ x: Double) -> Double { let c = clamp01(x); return c * c * (3 - 2 * c) }
    private static func lerp(_ a: Double, _ b: Double, _ m: Double) -> Double { a + (b - a) * m }
    private static func gauss(_ d: Double, _ s: Double) -> Double { exp(-(d * d) / (2 * s * s)) }
    private static func fract(_ x: Double) -> Double { x - floor(x) }
    /// Eased 0 → 1 → 0 over one unit of `x`.
    private static func pingPong(_ x: Double) -> Double { let f = fract(x); return f < 0.5 ? smooth(f * 2) : smooth(2 - f * 2) }
}
