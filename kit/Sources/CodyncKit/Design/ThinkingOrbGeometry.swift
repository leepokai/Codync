import Foundation

// Native port of thinking-orbs 0.3.1, commit de85557 (MIT, Jakub Antalik).
// Source: src/engine/{core,orbits,lattice,web}.ts and resolved 20/64 pt presets.
// See Resources/ThirdPartyNotices/thinking-orbs-LICENSE.txt.
// Keep geometry independent of SwiftUI so upstream golden vectors can verify it.
enum ThinkingOrbGeometry {
    struct Dot {
        var x, y, z, radius, white: Double
        var alpha: Double = 1
    }
    struct Line {
        var x1, y1, x2, y2, white, alpha, width: Double
    }
    struct Frame {
        var dots: [Dot] = []
        var lines: [Line] = []
    }
    typealias Point = SIMD3<Double>

    static func speed(state: ThinkingOrb.State, size: Double) -> Double {
        let small = size < 40
        switch state {
        case .working: return small ? 3.9 : 1.885
        case .searching: return small ? 2.665 : 2.015
        case .listening: return small ? 3.998 : 4.388
        case .connecting: return small ? 6.63 : 3.315
        }
    }

    /// `time` is the engine time, already multiplied by the preset speed.
    static func frame(state: ThinkingOrb.State, size: Double, time: Double) -> Frame {
        guard size.isFinite, size > 0, time.isFinite else { return Frame() }
        var frame: Frame
        switch state {
        case .working: frame = orbits(size, time)
        case .searching: frame = lattice(size, time, listening: false)
        case .listening: frame = lattice(size, time, listening: true)
        case .connecting: frame = web(size, time)
        }
        frame.dots = frame.dots.filter { $0.alpha >= 0.02 }.map {
            var dot = $0; dot.radius = max(0.3, dot.radius); return dot
        }.sorted { $0.z < $1.z }
        frame.lines = frame.lines.filter { $0.alpha >= 0.02 }
        return frame
    }

    private static func projector(_ yaw: Double, _ tilt: Double, _ size: Double, _ scale: Double) -> (Point) -> Point {
        let sy = sin(yaw), cy = cos(yaw), st = sin(tilt), ct = cos(tilt)
        return { p in
            let x = p.x * cy + p.z * sy, z = -p.x * sy + p.z * cy
            return Point(size / 2 + x * scale, size / 2 - (p.y * ct - z * st) * scale, p.y * st + z * ct)
        }
    }
    private static func hash(_ a: Double, _ b: Double) -> Double {
        let h = sin(a * 12.9898 + b * 78.233) * 43758.5453
        return h - floor(h)
    }
    private static func noise(_ x: Double, _ y: Double) -> Double {
        let xi = floor(x), yi = floor(y)
        let dx = x - xi, dy = y - yi
        let fx = dx * dx * (3 - 2 * dx), fy = dy * dy * (3 - 2 * dy)
        let a = hash(xi, yi), b = hash(xi + 1, yi), c = hash(xi, yi + 1), d = hash(xi + 1, yi + 1)
        return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy
    }
    private static func length(_ p: Point) -> Double { sqrt(p.x * p.x + p.y * p.y + p.z * p.z) }

    private static func orbits(_ size: Double, _ t: Double) -> Frame {
        let small = size < 40, radius = size / 2 * 0.82
        let project = projector(t * 0.12, 0.3, size, 1)
        let rs = pow(size / 300, 0.6), multiplier = small ? 2.4 : 1
        let orbitCount = small ? 3 : 12, ghostCount = small ? 10 : 40
        var frame = Frame()
        for orb in 0..<orbitCount {
            let h1 = hash(Double(orb), 1.7), h2 = hash(Double(orb), 5.2), h3 = hash(Double(orb), 8.9)
            let ro = radius * (0.45 + 0.52 * h1), theta = h1 * 2 * .pi, phi = acos(2 * h2 - 1)
            let normal = Point(sin(phi) * cos(theta), cos(phi), sin(phi) * sin(theta))
            var u = Point(-normal.y, normal.x, 0)
            u /= max(1e-6, length(u))
            let v = Point(-normal.z * u.y, normal.z * u.x, normal.x * u.y - normal.y * u.x)
            let speed = (0.25 + 0.55 * h3) * (h3 > 0.5 ? 1.0 : -1.0)
            for k in 0..<ghostCount {
                let angle = Double(k) / Double(ghostCount) * 2 * .pi
                let p = project((u * cos(angle) + v * sin(angle)) * ro)
                let depth = (p.z / ro + 1) / 2
                frame.dots.append(Dot(x: p.x, y: p.y, z: p.z, radius: 0.9 * multiplier * rs,
                                      white: 0.72, alpha: 0.5 * (0.4 + 0.6 * depth)))
            }
            for m in 0..<3 {
                let angle = t * speed + Double(m) / 3 * 2 * .pi + h2 * 6
                let p = project((u * cos(angle) + v * sin(angle)) * ro)
                let depth = (p.z / ro + 1) / 2
                frame.dots.append(Dot(x: p.x, y: p.y, z: p.z,
                                      radius: (1.2 + 1.6 * depth) * multiplier * rs, white: 0.3 - 0.22 * depth))
            }
        }
        return frame
    }

    private static func lattice(_ size: Double, _ t: Double, listening: Bool) -> Frame {
        let small = size < 40, rs = pow(size / 300, 0.6)
        let rings = listening ? (small ? 5 : 9) : (small ? 6 : 11)
        let density = listening ? (small ? 13 : 23) : (small ? 14 : 29)
        let multiplier = listening ? (small ? 1.6 : 1) : (small ? 1.75 : 1.15)
        let radius = size / 2 * (listening ? 0.874 : 0.82)
        let tilt = listening ? 0.38 : 0.4 + 0.06 * sin(t * 0.35)
        let project = projector(t * (listening ? 0.18 : 0.5), tilt, size, listening ? 1 : radius)
        let scan = t * (0.5 + 1.2 * (small ? 4.335 : 4.08))
        var frame = Frame()
        for ri in 0...rings {
            let lat = -.pi / 2 + Double(ri) / Double(rings) * .pi
            let cosLat = cos(lat), sinLat = sin(lat)
            let wave = 0.62 * sin(t * 2.1 - Double(ri) * 0.52) + 0.38 * sin(t * 1.27 + Double(ri) * 0.83)
            let rr = listening ? radius * (0.88 + 0.105 * wave) : 1
            let lonCount = max(1, Int((abs(cosLat) * Double(density)).rounded()))
            for j in 0..<lonCount {
                let lon = Double(j) / Double(lonCount) * 2 * .pi
                let p = project(Point(cosLat * cos(lon), sinLat, cosLat * sin(lon)) * rr)
                let depth = (p.z / (listening ? radius : 1) + 1) / 2
                if listening {
                    let crest = max(0, wave)
                    frame.dots.append(Dot(x: p.x, y: p.y, z: p.z,
                                          radius: (0.6 + 1.7 * depth) * multiplier * (1 + 0.4 * crest) * rs,
                                          white: 0.66 - 0.56 * depth - 0.1 * crest))
                } else {
                    let angle = lon + t * 0.5 - scan
                    let delta = atan2(sin(angle), cos(angle))
                    let boost = exp(-(delta * delta) / 0.18) * max(0, p.z)
                    frame.dots.append(Dot(x: p.x, y: p.y, z: p.z,
                                          radius: ((0.6 + 1.7 * depth) * multiplier + boost) * rs,
                                          white: 0.62 - 0.54 * depth, alpha: 0.45 + 0.55 * min(1, boost)))
                }
            }
        }
        return frame
    }

    private static func web(_ size: Double, _ t: Double) -> Frame {
        let small = size < 40, rs = pow(size / 300, 0.6)
        let count = small ? 8 : 41, signals = small ? 1 : 7
        let multiplier = small ? 1.52 : 0.95, threshold = 0.72
        let project = projector(t * 0.12, 0.32, size, size / 2 * 0.8)
        let golden = Double.pi * (3 - sqrt(5))
        let nodes: [Point] = (0..<count).map { index in
            let i = Double(index), y = 1 - 2 * (i + 0.5) / Double(count)
            let r = sqrt(1 - y * y), a = i * golden
            var p = Point(r * cos(a), y, r * sin(a))
            p.x += 0.3 * (noise(i * 0.31 + 9, t * 0.24) - 0.5) * 2
            p.y += 0.3 * (noise(i * 0.53 + 27, t * 0.21) - 0.5) * 2
            p.z += 0.3 * (noise(i * 0.77 + 55, t * 0.27) - 0.5) * 2
            return p / length(p)
        }
        var frame = Frame()
        for i in 0..<count {
            for j in (i + 1)..<count {
                let distance = length(nodes[i] - nodes[j])
                guard distance < threshold else { continue }
                let a = project(nodes[i]), b = project(nodes[j]), depth = ((a.z + b.z) / 2 + 1) / 2
                frame.lines.append(Line(x1: a.x, y1: a.y, x2: b.x, y2: b.y, white: 0.42,
                                        alpha: (1 - distance / threshold) * (0.3 + 0.55 * depth), width: max(0.6, 0.8 * rs)))
            }
            let p = project(nodes[i]), depth = (p.z + 1) / 2
            let pulse = 1 + 0.25 * sin(t * 1.4 + Double(i) * 2.7)
            frame.dots.append(Dot(x: p.x, y: p.y, z: p.z,
                                  radius: (1.4 + 1.8 * depth) * multiplier * pulse * rs, white: 0.55 - 0.45 * depth))
        }
        for s in 0..<signals {
            let segment = floor(t * 0.55 + Double(s) * 7.31)
            let a = Int(floor(hash(segment, Double(s) * 3.1 + 1.7) * Double(count)))
            let b = Int(floor(hash(segment, Double(s) * 5.7 + 4.2) * Double(count)))
            guard a != b else { continue }
            let tick = t * 0.55 + Double(s) * 7.31, fraction = tick - floor(tick)
            let node = nodes[a] + (nodes[b] - nodes[a]) * fraction
            let p = project(node / max(1e-6, length(node))), depth = (p.z + 1) / 2
            frame.dots.append(Dot(x: p.x, y: p.y, z: p.z,
                                  radius: (1.4 * 1.5 + 1.8 * depth) * multiplier * rs,
                                  white: 0.05, alpha: 0.5 + 0.5 * depth))
        }
        return frame
    }
}
