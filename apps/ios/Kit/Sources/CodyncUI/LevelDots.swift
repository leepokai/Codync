import CodyncKit
import SwiftUI

/// A dotted line (the call bar, the composer's recording pill): it follows your voice while
/// listening, ripples while the bot talks, breathes while it works, and rests as faint dots otherwise.
struct LevelDots: View {
    let level: Float
    let speaking: Bool
    let working: Bool
    let color: Color
    let reduceMotion: Bool

    var body: some View {
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
