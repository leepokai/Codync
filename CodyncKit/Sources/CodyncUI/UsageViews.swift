import CodyncKit
import SwiftUI

public struct UsageCard: View {
    let provider: UsageProvider

    public init(provider: UsageProvider) { self.provider = provider }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(provider.name).font(.subheadline.bold())
                Spacer()
                Text("updated \(RelativeTime.short(Date(milliseconds: provider.updatedAt)))")
                    .font(.caption2)
                    .foregroundStyle(Palette.tertiary)
            }
            ForEach(provider.windows) { w in
                VStack(alignment: .leading, spacing: 3) {
                    HStack {
                        Text(w.label).font(.caption)
                        Spacer()
                        Text("\(Int(w.percent.rounded()))%").font(.caption.monospacedDigit().bold())
                        if let reset = w.resetDescription {
                            Text("· \(reset)").font(.caption2).foregroundStyle(Palette.tertiary)
                        }
                    }
                    UsageBar(percent: w.percent)
                }
            }
        }
        .padding(.vertical, 4)
    }
}

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
                            Gauge(value: min(w.percent, 100), in: 0...100) { EmptyView() }
                                .gaugeStyle(.accessoryCircularCapacity)
                                .scaleEffect(0.42)
                                .frame(width: 22, height: 22)
                                .tint(Palette.usageTint(w.percent))
                            VStack(alignment: .leading, spacing: 0) {
                                Text("\(p.name) \(w.label)").metaStyle()
                                Text("\(Int(w.percent.rounded()))%").font(.caption.bold().monospacedDigit()).foregroundStyle(Palette.text)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Palette.surface, in: Capsule())
                        .overlay(Capsule().stroke(Palette.border))
                    }
                }
            }
        }
    }
}
