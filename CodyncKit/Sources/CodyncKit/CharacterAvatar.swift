import SwiftUI

/// Grok-Bot-style character: a colored shape with eyes. The eyes look around
/// while the bot works; a badge shows when it needs you.
public struct CharacterAvatar: View {
    public enum Mood: Sendable { case idle, working, needsInput }

    let shape: String
    let color: Color
    let size: CGFloat
    let mood: Mood

    public init(shape: String, color: String, size: CGFloat = 40, mood: Mood = .idle) {
        self.shape = shape
        self.color = AvatarPalette.color(color)
        self.size = size
        self.mood = mood
    }

    public init(bot: Bot, size: CGFloat = 40, animated: Bool = true) {
        self.init(
            shape: bot.avatarShape,
            color: bot.avatarColor,
            size: size,
            mood: !animated ? .idle : bot.needsInput ? .needsInput : bot.isWorking ? .working : .idle
        )
    }

    public var body: some View {
        ZStack {
            CharacterShape(kind: shape)
                .fill(color.gradient)
            Eyes(size: size, working: mood == .working)
                .offset(y: -size * 0.03)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

private struct Eyes: View {
    let size: CGFloat
    let working: Bool

    var body: some View {
        let eye = size * 0.17
        HStack(spacing: size * 0.12) {
            ForEach(0..<2, id: \.self) { _ in
                Capsule()
                    .fill(.white)
                    .frame(width: eye * 0.8, height: eye * 1.15)
                    .overlay(
                        Circle().fill(Color(hex: 0x141712)).frame(width: eye * 0.45)
                    )
            }
        }
        .phaseAnimator(working ? [-1.0, 1.0] : [0.0]) { view, phase in
            view.offset(x: phase * size * 0.06)
        } animation: { _ in .easeInOut(duration: 0.9) }
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

/// Avatar with the roster status dot: lime = unread, orange = needs you.
public struct AvatarWithStatus: View {
    let bot: Bot
    let size: CGFloat

    public init(bot: Bot, size: CGFloat = 44) {
        self.bot = bot
        self.size = size
    }

    public var body: some View {
        CharacterAvatar(bot: bot, size: size)
            .overlay(alignment: .bottomTrailing) {
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
    }
}
