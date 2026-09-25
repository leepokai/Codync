import CodyncKit
import SwiftUI

/// Compact usage chips at the top of the roster.
public struct UsageStrip: View {
    let usage: Usage

    public init(usage: Usage) { self.usage = usage }

    public var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(usage.providers) { p in
                    ForEach(p.windows.prefix(2)) { w in
                        HStack(spacing: 6) {
                            AgentIcon(registry: p.registry, size: 14)
                            UsageRing(percent: w.percent)
                            VStack(alignment: .leading, spacing: 0) {
                                Text(w.title).font(.caption2).foregroundStyle(Palette.tertiary)
                                Text("\(Int(w.percent.rounded()))%").font(.caption.bold().monospacedDigit()).foregroundStyle(Palette.text)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Palette.surface, in: Capsule())
                    }
                }
            }
        }
    }
}

/// A small capacity ring: the replacement for the circular `Gauge`.
private struct UsageRing: View {
    let percent: Double

    var body: some View {
        ZStack {
            Circle().stroke(Palette.text.opacity(0.1), lineWidth: 2.5)
            Circle()
                .trim(from: 0, to: min(max(percent, 0), 100) / 100)
                .stroke(Palette.usageTint(percent), style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: 16, height: 16)
        .frame(width: 22, height: 22)
        .accessibilityHidden(true)
    }
}
