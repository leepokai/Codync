import SwiftUI

/// Monochrome Lock Screen widgets; the system supplies the background and tint.
public struct AccessoryWidgetCard: View {
    public enum Kind { case bots, usage }
    public enum Family: String, CaseIterable { case circular = "Circle", rectangular = "Rectangle", inline = "Inline" }
    let kind: Kind
    let family: Family
    let bots: [Bot]
    let usage: Usage
    let paired: Bool

    public init(kind: Kind, family: Family, bots: [Bot] = [], usage: Usage = Usage(), paired: Bool = true) {
        self.kind = kind; self.family = family; self.bots = bots.filter { !$0.hidden }
        self.usage = usage; self.paired = paired
    }

    private var needs: Int { bots.filter(\.needsInput).count }
    private var working: Int { bots.filter { $0.isWorking && !$0.needsInput }.count }
    private var count: Int { needs > 0 ? needs : working > 0 ? working : bots.count }
    private var botLabel: String { needs > 0 ? (needs == 1 ? "needs you" : "need you") : working > 0 ? "working" : (bots.count == 1 ? "bot" : "bots") }
    private var top: (name: String, window: UsageWindow)? {
        usage.providers.flatMap { p in p.windows.map { (name: p.name, window: $0) } }
            .max { $0.window.percent < $1.window.percent }
    }
    private var headline: String {
        guard paired else { return "Connect a computer" }
        if kind == .bots { return bots.isEmpty ? "No bots yet" : "\(count) \(botLabel)" }
        return top.map { "\($0.name) \(Int($0.window.percent.rounded()))%" } ?? "No usage yet"
    }

    public var body: some View {
        Group {
            switch family {
            case .inline:
                Label(headline, systemImage: kind == .bots ? "bubble.left.and.bubble.right" : "chart.bar")
                    .font(.caption).lineLimit(1)
            case .circular:
                if !paired {
                    Image(systemName: "desktopcomputer").accessibilityLabel(headline)
                } else if kind == .usage, let top {
                    Gauge(value: min(100, max(0, top.window.percent)), in: 0...100) {
                        Text(top.name.prefix(2))
                    } currentValueLabel: {
                        Text("\(Int(top.window.percent.rounded()))").font(.system(size: 18, weight: .semibold))
                    }
                    .gaugeStyle(.accessoryCircularCapacity)
                    .accessibilityLabel(headline)
                } else {
                    VStack(spacing: 0) {
                        Text(kind == .bots ? "\(count)" : "—").font(.system(size: 24, weight: .semibold, design: .rounded))
                        Text(kind == .bots ? (needs > 0 ? "need you" : working > 0 ? "working" : "bots") : "usage")
                            .font(.system(size: 9))
                    }
                }
            case .rectangular:
                VStack(alignment: .leading, spacing: 2) {
                    Text(headline).font(.system(size: 12, weight: .semibold)).lineLimit(1)
                    if paired, kind == .bots {
                        ForEach(bots.sorted { rank($0) < rank($1) }.prefix(2)) { bot in
                            Text("\(bot.name) · \(bot.needsInput ? "Needs you" : bot.isWorking ? "Working" : "Ready")")
                                .font(.system(size: 10)).lineLimit(1)
                        }
                    } else if paired, let top {
                        Text(top.window.title).font(.system(size: 10)).lineLimit(1)
                        if let reset = top.window.resetsShort() {
                            Text(reset).font(.system(size: 9)).lineLimit(1)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func rank(_ bot: Bot) -> Int { bot.needsInput ? 0 : bot.isWorking ? 1 : 2 }
}
