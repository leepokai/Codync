import CodyncKit
import SwiftUI

/// A dotted line (the call bar, the composer's recording pill): it follows your voice while
/// listening, ripples while the bot talks, breathes while it works, and rests as faint dots otherwise.
/// While a call connects it shows an indeterminate progress line instead, and fades back to the dots.
struct LevelDots: View {
    let level: Float
    let speaking: Bool
    let working: Bool
    var connecting = false
    let color: Color
    let reduceMotion: Bool

    var body: some View {
        ZStack {
            dots.opacity(connecting ? 0 : 1)
            if connecting { ConnectingLine(reduceMotion: reduceMotion).transition(.opacity) }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: connecting)
    }

    private var dots: some View {
        TimelineView(.animation(paused: reduceMotion)) { context in
            Canvas { canvas, size in
                let t = context.date.timeIntervalSinceReferenceDate
                let spacing: CGFloat = 8
                let count = max(1, Int(size.width / spacing))
                for i in 0..<count {
                    let x = CGFloat(i) * spacing + spacing / 2
                    let wave = (sin(Double(i) * 0.55 - t * 6) + 1) / 2
                    let amount: Double = if speaking {
                        0.5 + 0.5 * wave
                    } else if level > 0.05 {
                        max(0.2, Double(level)) * (0.45 + 0.55 * wave)
                    } else if working {
                        0.25 * ((sin(t * 2.4) + 1) / 2)
                    } else {
                        0.08
                    }
                    let height = 4 + CGFloat(amount) * (size.height - 4)
                    let rect = CGRect(x: x - 1.25, y: (size.height - height) / 2, width: 2.5, height: height)
                    let tint = speaking ? Palette.accent : color
                    canvas.fill(Path(roundedRect: rect, cornerRadius: 1.25), with: .color(tint.opacity(0.5 + 0.5 * amount)))
                }
            }
        }
    }
}

/// Indeterminate progress: a short capsule glides back and forth along a faint track, easing in and out
/// at both ends; with reduced motion it rests at the left. White in dark mode, black in light.
private struct ConnectingLine: View {
    let reduceMotion: Bool
    /// Seconds for one trip there and back.
    private static let period = 2.4

    var body: some View {
        TimelineView(.animation(paused: reduceMotion)) { context in
            Canvas { canvas, size in
                let mid = size.height / 2
                let track = CGRect(x: 0, y: mid - 0.75, width: size.width, height: 1.5)
                canvas.fill(Path(roundedRect: track, cornerRadius: 0.75), with: .color(Palette.accent.opacity(0.15)))
                let width = size.width / 3
                // A cosine turns each end smoothly instead of jumping back to the start.
                let progress = reduceMotion ? 0
                    : (1 - cos(2 * .pi * context.date.timeIntervalSinceReferenceDate / Self.period)) / 2
                let segment = CGRect(x: progress * (size.width - width), y: mid - 1.75, width: width, height: 3.5)
                canvas.fill(Path(roundedRect: segment, cornerRadius: 1.75), with: .color(Palette.accent))
            }
        }
    }
}
