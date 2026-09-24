import ActivityKit
import AppIntents
import CodyncKit
import SwiftUI
import WidgetKit

@main
struct CodyncWidgets: WidgetBundle {
    var body: some Widget {
        UsageWidget()
        BotLiveActivity()
    }
}

// MARK: - Usage widget

struct UsageEntry: TimelineEntry {
    let date: Date
    let usage: Usage
}

struct UsageIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Usage limits"
}

struct UsageTimeline: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> UsageEntry {
        UsageEntry(date: .now, usage: .sample)
    }

    func snapshot(for configuration: UsageIntent, in context: Context) async -> UsageEntry {
        UsageEntry(date: .now, usage: SharedStore.usage ?? .sample)
    }

    func timeline(for configuration: UsageIntent, in context: Context) async -> Timeline<UsageEntry> {
        let usage = await Self.fetch() ?? SharedStore.usage ?? Usage()
        return Timeline(entries: [UsageEntry(date: .now, usage: usage)], policy: .after(.now + 15 * 60))
    }

    /// Asks the paired host directly (works over Tailscale), falling back to the app's cache.
    static func fetch() async -> Usage? {
        guard let pairing = SharedStore.orderedPairing, let client = await HostClient.resolve(pairing),
              let usage = try? await client.usage() else { return nil }
        SharedStore.usage = usage
        return usage
    }
}

extension Usage {
    static let sample = Usage(providers: [])
}

struct UsageWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "CodyncUsage", intent: UsageIntent.self, provider: UsageTimeline()) { entry in
            UsageWidgetView(entry: entry)
                .containerBackground(Palette.background, for: .widget)
        }
        .configurationDisplayName("Usage limits")
        .description("Claude and Codex subscription limits from your computer.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryCircular, .accessoryRectangular])
    }
}

struct UsageWidgetView: View {
    let entry: UsageEntry
    @Environment(\.widgetFamily) private var family

    private var windows: [(provider: String, window: UsageWindow)] {
        entry.usage.providers.flatMap { p in p.windows.map { (p.name, $0) } }
    }

    var body: some View {
        switch family {
        case .accessoryCircular:
            if let first = windows.first {
                Gauge(value: min(first.window.percent, 100), in: 0...100) {
                    Text(first.provider.prefix(1))
                } currentValueLabel: {
                    Text("\(Int(first.window.percent.rounded()))")
                }
                .gaugeStyle(.accessoryCircularCapacity)
            } else {
                Image(systemName: "gauge.with.dots.needle.0percent")
            }
        case .accessoryRectangular:
            VStack(alignment: .leading, spacing: 2) {
                ForEach(Array(windows.prefix(3).enumerated()), id: \.offset) { _, item in
                    HStack {
                        Text("\(item.provider) \(item.window.label)").lineLimit(1)
                        Spacer()
                        Text("\(Int(item.window.percent.rounded()))%").monospacedDigit()
                    }
                    .font(.caption2)
                }
            }
        default:
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 6) {
                    CharacterAvatar(shape: "blob", color: "green", size: 16)
                    Text("Usage").font(.caption.bold()).foregroundStyle(Palette.text)
                }
                if windows.isEmpty {
                    Text("Open Codync and pair with your computer.")
                        .font(.caption2)
                        .foregroundStyle(Palette.secondary)
                }
                ForEach(Array(windows.prefix(family == .systemSmall ? 3 : 4).enumerated()), id: \.offset) { _, item in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text("\(item.provider) · \(item.window.label)").lineLimit(1)
                            Spacer(minLength: 2)
                            Text("\(Int(item.window.percent.rounded()))%").bold().monospacedDigit()
                        }
                        .font(.caption2)
                        .foregroundStyle(Palette.text)
                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                Capsule().fill(Palette.bubbleAgent)
                                Capsule()
                                    .fill(item.window.percent >= 90 ? Palette.danger : item.window.percent >= 70 ? Palette.warning : Palette.accentFill)
                                    .frame(width: geo.size.width * min(1, max(0.02, item.window.percent / 100)))
                            }
                        }
                        .frame(height: 5)
                        if family == .systemMedium, let reset = item.window.resetDescription {
                            Text(reset).font(.system(size: 9)).foregroundStyle(Palette.tertiary)
                        }
                    }
                }
                Spacer(minLength: 0)
            }
        }
    }
}

// MARK: - Live Activity

struct BotLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: BotActivityAttributes.self) { context in
            LockScreenView(context: context)
                .activityBackgroundTint(Palette.background)
                .widgetURL(URL(string: "codync://bot/\(context.attributes.botId)"))
        } dynamicIsland: { context in
            let s = context.state
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    CharacterAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, size: 36,
                                    mood: s.status == "needsInput" ? .needsInput : .working)
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(context.attributes.name).font(.headline)
                        Text(statusText(s)).font(.caption).foregroundStyle(s.status == "needsInput" ? Palette.warning : .secondary).lineLimit(2)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if let started = s.startedAt, s.status == "working" {
                        Text(started, style: .timer).font(.caption.monospacedDigit()).frame(width: 52)
                    }
                }
            } compactLeading: {
                CharacterAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, size: 20)
            } compactTrailing: {
                if s.status == "needsInput" {
                    Image(systemName: "hand.raised.fill").foregroundStyle(Palette.warning)
                } else if s.status == "working", let started = s.startedAt {
                    Text(started, style: .timer).font(.caption2.monospacedDigit()).frame(width: 40)
                } else {
                    Image(systemName: "checkmark").foregroundStyle(Palette.accentFill)
                }
            } minimal: {
                CharacterAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, size: 20)
            }
            .widgetURL(URL(string: "codync://bot/\(context.attributes.botId)"))
        }
    }
}

private func statusText(_ s: BotActivityAttributes.ContentState) -> String {
    switch s.status {
    case "needsInput": s.activity.isEmpty ? "Needs your approval" : s.activity
    case "working": s.activity.isEmpty ? "Working…" : s.activity
    case "error": "Stopped with an error"
    default: "Done"
    }
}

private struct LockScreenView: View {
    let context: ActivityViewContext<BotActivityAttributes>

    var body: some View {
        let s = context.state
        HStack(spacing: 12) {
            CharacterAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, size: 44,
                            mood: s.status == "needsInput" ? .needsInput : s.status == "working" ? .working : .idle)
            VStack(alignment: .leading, spacing: 3) {
                Text(context.attributes.name).font(.headline).foregroundStyle(Palette.text)
                Text(statusText(s))
                    .font(.subheadline)
                    .foregroundStyle(s.status == "needsInput" ? Palette.warning : Palette.secondary)
                    .lineLimit(2)
            }
            Spacer()
            if s.status == "working", let started = s.startedAt {
                Text(started, style: .timer)
                    .font(.title3.monospacedDigit())
                    .foregroundStyle(Palette.accent)
                    .frame(width: 70, alignment: .trailing)
            }
        }
        .padding(16)
    }
}
