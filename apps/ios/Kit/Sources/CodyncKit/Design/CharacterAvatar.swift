import SwiftUI

/// Grok-Bot-style character drawn like the app icon: an even grid of dots,
/// shaded as if the silhouette were a ball, with the eyes left hollow (two
/// missing pairs of dots). The eyes glance side to side while the bot works
/// and blink now and then; a badge shows when it needs you.
public struct CharacterAvatar: View {
    public enum Mood: Sendable { case idle, working, needsInput }

    let shape: String
    let color: Color
    let size: CGFloat
    let mood: Mood
    let still: Bool
    var customIcon: CustomIcon?

    /// `still` draws one frame of the mood (widgets and Live Activities render one frame);
    /// a change of mood then moves the halftone highlight and the eyes, animatable dot by dot.
    public init(shape: String, color: String, size: CGFloat = 40, mood: Mood = .idle, still: Bool = false, customIcon: CustomIcon? = nil) {
        self.init(shape: shape, tint: AvatarPalette.color(color), size: size, mood: mood, still: still)
        self.customIcon = customIcon
    }

    public init(shape: String, tint: Color, size: CGFloat = 40, mood: Mood = .idle, still: Bool = false) {
        self.shape = shape
        self.color = tint
        self.size = size
        self.mood = mood
        self.still = still
    }

    public init(bot: Bot, size: CGFloat = 40, animated: Bool = true) {
        self.init(
            shape: bot.avatarShape,
            color: bot.avatarColor,
            size: size,
            mood: animated ? bot.mood : .idle
        )
        customIcon = bot.customIcon
    }

    /// The bot's current mood, drawn as one frame.
    public init(bot: Bot, size: CGFloat = 40, still: Bool) {
        self.init(shape: bot.avatarShape, color: bot.avatarColor, size: size, mood: bot.mood, still: still)
        customIcon = bot.customIcon
    }

    public var body: some View {
        Group {
            if let customIcon, customIcon.image != nil {
                CustomIconView(icon: customIcon, size: size, tint: color)
            } else {
                DottedBody(shape: shape, color: color, size: size, mood: mood, paused: still || customIcon != nil)
            }
        }
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

public extension Bot {
    var mood: CharacterAvatar.Mood { needsInput ? .needsInput : isWorking ? .working : .idle }
}

/// Keep the halftone at every size. Small icons use fewer, larger dots so the
/// gaps and hollow eyes survive rasterization in widgets and the Dynamic Island.
struct DotGrid {
    let cells: Int
    init(size: CGFloat) { cells = size < 18 ? 7 : size < 28 ? 9 : 13 }
    var eyeColumns: [Int] { cells == 7 ? [2, 4] : cells == 9 ? [3, 6] : [4, 8] }
    var eyeRows: [Int] { cells == 13 ? [4, 5, 6] : cells == 9 ? [3, 4] : [2, 3] }
}

struct DottedBody: View {
    let shape: String
    let color: Color
    let size: CGFloat
    let mood: CharacterAvatar.Mood
    let paused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Where the light sits and where the eyes look.
    struct Pose: Equatable {
        var yaw: Double
        var glance: Int
        var blinking = false
        /// The needs-you ripple's phase, nil without one.
        var ripple: Double?

        /// One frame of the mood: in widgets and Live Activities, which render one frame, a
        /// change of mood moves the halftone highlight and the eyes to the new pose.
        static func still(_ mood: CharacterAvatar.Mood) -> Pose {
            switch mood {
            case .idle: Pose(yaw: -0.7, glance: 0)
            case .working: Pose(yaw: 1.1, glance: 1)
            case .needsInput: Pose(yaw: 0, glance: 0, ripple: 0.6)
            }
        }
    }

    /// One dot's look for a pose: radius, grey ink, and how much of the bot's color shows.
    struct Ink {
        let dot: Dot
        let radius: CGFloat
        let ink: Double
        let tint: Double
        let eye: Bool
    }

    var body: some View {
        let grid = DotGrid(size: size)
        let step = size / CGFloat(grid.cells)
        let dots = Self.grid(shape: shape, size: size, step: step, cells: grid.cells)
        if paused {
            // One view per dot (not a Canvas), so the system interpolates each dot's size and
            // color between two poses, also inside a Live Activity.
            ZStack(alignment: .topLeading) {
                ForEach(Self.inks(dots, grid: grid, step: step, size: size, pose: .still(mood)), id: \.dot.id) { d in
                    Circle()
                        .fill(Palette.text.opacity(d.ink))
                        .overlay(Circle().fill(color.opacity(d.tint)))
                        .frame(width: d.radius * 2, height: d.radius * 2)
                        .position(d.dot.center)
                        .opacity(d.eye ? 0 : 1)
                }
            }
            .frame(width: size, height: size)
        } else {
            let still = mood == .idle || reduceMotion
            TimelineView(.animation(paused: still)) { timeline in
                let t = still ? 0 : timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 3600)
                // Glance: whole-cell steps left / center / right, like a small display.
                let pose = Pose(yaw: mood == .working ? t * 1.4 : -0.7,
                                glance: mood == .working ? Int((sin(t * 2 * .pi / 3.2) * 1.4).rounded()) : 0,
                                blinking: !still && (t / 4.7).truncatingRemainder(dividingBy: 1) < 0.035,
                                ripple: mood == .needsInput ? t * 5 : nil)
                Canvas { ctx, _ in
                    for d in Self.inks(dots, grid: grid, step: step, size: size, pose: pose) where !d.eye {
                        let p = d.dot.center, r = d.radius
                        let dot = Path(ellipseIn: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 2))
                        ctx.fill(dot, with: .color(Palette.text.opacity(d.ink)))
                        if d.tint > 0 { ctx.fill(dot, with: .color(color.opacity(d.tint))) }
                    }
                }
            }
        }
    }

    /// The working loop the Dynamic Island plays one frame per second (a timer Text in a font
    /// whose digits are these frames; see tools/timer-fonts.py): the light circles the ball
    /// once, the eyes look right, back, left, and blink on the last frame.
    static func workingLoop(shape: String, size: CGFloat) -> [[Ink]] {
        let cells = DotGrid(size: size)
        let step = size / CGFloat(cells.cells)
        let dots = grid(shape: shape, size: size, step: step, cells: cells.cells)
        let glances = [0, 1, 1, 1, 0, -1, -1, -1, 0, 0]
        return glances.indices.map { k in
            let pose = Pose(yaw: -0.7 + Double(k) * 2 * .pi / 10, glance: glances[k], blinking: k == 9)
            return inks(dots, grid: cells, step: step, size: size, pose: pose)
        }
    }

    /// Shades the dots as a lit ball; the eyes stay hollow.
    static func inks(_ dots: [Dot], grid: DotGrid, step: CGFloat, size: CGFloat, pose: Pose) -> [Ink] {
        let eyeRows = pose.blinking ? Array(grid.eyeRows.suffix(1)) : grid.eyeRows
        let eyeCols = grid.eyeColumns.map { $0 + pose.glance }
        let half = size / 2
        let lx = sin(pose.yaw) * 0.8, ly = 0.55, lz = cos(pose.yaw) * 0.5 + 0.6  // never fully behind
        let ll = (lx * lx + ly * ly + lz * lz).squareRoot()
        return dots.map { d in
            let p = d.center
            let u = (p.x - half) / half, v = (half - p.y) / half
            let z = max(0.2, 1 - u * u - v * v).squareRoot()
            let nl = (u * u + v * v + z * z).squareRoot()
            var shade = 0.3 + 0.7 * max(0, (u * lx + v * ly + z * lz) / (nl * ll))
            if let phase = pose.ripple {
                let ripple = 0.5 + 0.5 * sin((u * u + v * v).squareRoot() * 9 - phase)
                shade *= 0.6 + 0.4 * ripple
            }
            return Ink(dot: d,
                       radius: step * 0.42 * (0.55 + 0.45 * shade),
                       ink: grid.cells < 13 ? 0.4 + 0.4 * shade : 0.2 + 0.4 * min(1, shade / 0.7),
                       tint: shade > 0.6 ? (shade - 0.6) / 0.4 : 0,
                       eye: eyeCols.contains(d.col) && eyeRows.contains(d.row))
        }
    }

    struct Dot {
        let row: Int
        let col: Int
        let center: CGPoint
        var id: Int { row * 100 + col }
    }

    /// Square-grid dot centers that fall inside the silhouette.
    static func grid(shape: String, size: CGFloat, step: CGFloat, cells: Int) -> [Dot] {
        let path = CharacterShape(kind: shape).path(in: CGRect(x: 0, y: 0, width: size, height: size))
        var out: [Dot] = []
        for row in 0..<cells {
            for col in 0..<cells {
                let p = CGPoint(x: (CGFloat(col) + 0.5) * step, y: (CGFloat(row) + 0.5) * step)
                if path.contains(p) { out.append(Dot(row: row, col: col, center: p)) }
            }
        }
        return out
    }
}

/// The eight Grok Bot character silhouettes.
public struct CharacterShape: Shape {
    let kind: String

    public init(kind: String) { self.kind = kind }

    public func path(in r: CGRect) -> Path {
        let w = r.width, h = r.height
        switch kind {
        case "pebble":
            return Path(ellipseIn: r.insetBy(dx: 0, dy: h * 0.1))
        case "squircle":
            return Path(roundedRect: r.insetBy(dx: w * 0.04, dy: h * 0.04), cornerRadius: w * 0.3, style: .continuous)
        case "tablet":
            return Path(roundedRect: r.insetBy(dx: w * 0.14, dy: 0), cornerRadius: w * 0.22, style: .continuous)
        case "wedge":
            var p = Path()
            p.move(to: CGPoint(x: w * 0.5, y: h * 0.04))
            p.addQuadCurve(to: CGPoint(x: w * 0.98, y: h * 0.86), control: CGPoint(x: w * 0.9, y: h * 0.4))
            p.addQuadCurve(to: CGPoint(x: w * 0.02, y: h * 0.86), control: CGPoint(x: w * 0.5, y: h * 1.04))
            p.addQuadCurve(to: CGPoint(x: w * 0.5, y: h * 0.04), control: CGPoint(x: w * 0.1, y: h * 0.4))
            return p
        case "hex":
            var p = Path()
            for i in 0..<6 {
                let a = Double(i) * .pi / 3 - .pi / 2
                let pt = CGPoint(x: w / 2 + cos(a) * w * 0.49, y: h / 2 + sin(a) * h * 0.49)
                i == 0 ? p.move(to: pt) : p.addLine(to: pt)
            }
            p.closeSubpath()
            return p.strokedPath(.init(lineWidth: w * 0.08, lineJoin: .round)).union(p)
        case "cloud":
            var p = Path()
            p.addEllipse(in: CGRect(x: 0, y: h * 0.3, width: w * 0.55, height: h * 0.55))
            p.addEllipse(in: CGRect(x: w * 0.45, y: h * 0.3, width: w * 0.55, height: h * 0.55))
            p.addEllipse(in: CGRect(x: w * 0.18, y: h * 0.08, width: w * 0.64, height: h * 0.64))
            p.addRoundedRect(in: CGRect(x: w * 0.1, y: h * 0.5, width: w * 0.8, height: h * 0.35), cornerSize: CGSize(width: w * 0.15, height: w * 0.15))
            return p
        case "teardrop":
            var p = Path()
            p.move(to: CGPoint(x: w * 0.5, y: 0))
            p.addCurve(to: CGPoint(x: w * 0.94, y: h * 0.62), control1: CGPoint(x: w * 0.62, y: h * 0.2), control2: CGPoint(x: w * 0.94, y: h * 0.38))
            p.addArc(center: CGPoint(x: w * 0.5, y: h * 0.62), radius: w * 0.44, startAngle: .degrees(0), endAngle: .degrees(180), clockwise: false)
            p.addCurve(to: CGPoint(x: w * 0.5, y: 0), control1: CGPoint(x: w * 0.06, y: h * 0.38), control2: CGPoint(x: w * 0.38, y: h * 0.2))
            return p
        default: // blob
            var p = Path()
            let c = CGPoint(x: w / 2, y: h / 2)
            let steps = 64
            for i in 0...steps {
                let a = Double(i) / Double(steps) * 2 * .pi
                let rr = 0.46 + 0.035 * sin(a * 3 + 0.6)
                let pt = CGPoint(x: c.x + cos(a) * w * rr, y: c.y + sin(a) * h * rr)
                i == 0 ? p.move(to: pt) : p.addLine(to: pt)
            }
            p.closeSubpath()
            return p
        }
    }
}

/// A group chat's face, clustered like iMessage: two bots tucked diagonally,
/// three in a triangle, four in a 2×2 grid; past four the last cell counts the rest.
public struct GroupAvatar: View {
    let members: [Bot]
    let size: CGFloat
    let animated: Bool

    public init(members: [Bot], size: CGFloat = 40, animated: Bool = true) {
        self.members = members
        self.size = size
        self.animated = animated
    }

    public var body: some View {
        ZStack {
            switch members.count {
            case 0:
                Image(systemName: "person.2")
                    .font(.system(size: size * 0.4))
                    .foregroundStyle(.secondary)
            case 1:
                CharacterAvatar(bot: members[0], size: size, animated: animated)
            case 2:
                place(members[1], size * 0.66, .topTrailing)
                place(members[0], size * 0.66, .bottomLeading)
            case 3:
                place(members[0], size * 0.55, .top)
                place(members[1], size * 0.55, .bottomLeading)
                place(members[2], size * 0.55, .bottomTrailing)
            default:
                place(members[0], size * 0.5, .topLeading)
                place(members[1], size * 0.5, .topTrailing)
                place(members[2], size * 0.5, .bottomLeading)
                if members.count == 4 {
                    place(members[3], size * 0.5, .bottomTrailing)
                } else {
                    Text(L10n.tr("+\(members.count - 3)"))
                        .font(.system(size: size * 0.24, weight: .bold, design: .rounded))
                        .foregroundStyle(.secondary)
                        .frame(width: size * 0.5, height: size * 0.5)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private func place(_ bot: Bot, _ side: CGFloat, _ corner: Alignment) -> some View {
        CharacterAvatar(bot: bot, size: side, animated: animated)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: corner)
    }
}

/// Avatar with the roster status dot: lime = unread, orange = needs you.
/// A group shows its `members`.
public struct AvatarWithStatus: View {
    let bot: Bot
    let members: [Bot]
    let size: CGFloat

    public init(bot: Bot, members: [Bot] = [], size: CGFloat = 44) {
        self.bot = bot
        self.members = members
        self.size = size
    }

    public var body: some View {
        Group {
            if bot.isGroup && bot.customIcon == nil { GroupAvatar(members: members, size: size) } else { CharacterAvatar(bot: bot, size: size) }
        }
            .overlay(alignment: bot.lead ? .topTrailing : .bottomTrailing) {
                if bot.needsInput {
                    Image(systemName: "exclamationmark")
                        .font(.system(size: size * 0.2, weight: .black))
                        .foregroundStyle(.white)
                        .frame(width: size * 0.36, height: size * 0.36)
                        .background(Palette.warning, in: Circle())
                        .overlay(Circle().stroke(Palette.background, lineWidth: 2))
                } else if bot.unread > 0 {
                    Circle()
                        .fill(Palette.accentFill)
                        .frame(width: size * 0.28, height: size * 0.28)
                        .overlay(Circle().stroke(Palette.background, lineWidth: 2))
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if bot.lead { LeadAgentBadge(size: max(12, size * 0.34)) }
            }
    }
}
