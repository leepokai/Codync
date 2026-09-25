import ActivityKit
import AppIntents
import CodyncKit
import SwiftUI
import WidgetKit

@main
struct CodyncWidgets: WidgetBundle {
    var body: some Widget {
        BotsWidget()
        UsageWidget()
        ProviderUsageWidget()
        BotLiveActivity()
    }
}

// MARK: - Bots at a glance

struct BotsEntry: TimelineEntry {
    let date: Date
    let bots: [Bot]
    let paired: Bool
    var links: [String: URL] = [:]
}

struct BotsTimeline: TimelineProvider {
    func placeholder(in context: Context) -> BotsEntry { BotsEntry(date: .now, bots: Bot.widgetPreview, paired: true) }

    func getSnapshot(in context: Context, completion: @escaping (BotsEntry) -> Void) {
        let storage = SharedStore.activeContext
        completion(BotsEntry(date: .now, bots: context.isPreview ? Bot.widgetPreview : storage.bots,
                             paired: context.isPreview || storage.pairing != nil))
    }

    /// The app writes the roster and reloads this widget when a bot's state changes.
    func getTimeline(in context: Context, completion: @escaping (Timeline<BotsEntry>) -> Void) {
        let storage = SharedStore.activeContext
        let entry = BotsEntry(date: .now, bots: storage.bots, paired: storage.pairing != nil,
                              links: Dictionary(storage.bots.map { ($0.id, storage.botURL($0.id)) }, uniquingKeysWith: { a, _ in a }))
        completion(Timeline(entries: [entry], policy: .never))
    }
}

struct BotsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "CodyncBots", provider: BotsTimeline()) { entry in
            BotsWidgetView(entry: entry)
                .containerBackground(Palette.surface, for: .widget)
        }
        .configurationDisplayName("Bots")
        .description("Bots that need you, running tasks, and their current activity.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge, .accessoryCircular, .accessoryRectangular, .accessoryInline])
    }
}

struct BotsWidgetView: View {
    let entry: BotsEntry
    @Environment(\.widgetFamily) private var family

    /// Needs you first, then working, then most recent.
    private var ordered: [Bot] {
        entry.bots.filter { !$0.hidden }.sorted {
            let rank = { (b: Bot) in b.needsInput ? 0 : b.isWorking ? 1 : 2 }
            return rank($0) != rank($1) ? rank($0) < rank($1) : $0.lastAt > $1.lastAt
        }
    }
    var body: some View {
        if !entry.paired {
            switch family {
            case .accessoryInline:
                Text("Connect Codync to your computer")
            case .accessoryCircular:
                Image(systemName: "desktopcomputer")
                    .accessibilityLabel("Connect Codync to your computer")
            case .accessoryRectangular:
                Label("Connect a computer", systemImage: "desktopcomputer")
                    .font(.caption)
            default:
                EmptyWidget(text: "Open Codync to pair with your computer.")
            }
        } else {
            switch family {
            case .accessoryInline:
                AccessoryWidgetCard(kind: .bots, family: .inline, bots: entry.bots).widgetAccentable()
            case .accessoryCircular:
                AccessoryWidgetCard(kind: .bots, family: .circular, bots: entry.bots).widgetAccentable()
            case .accessoryRectangular:
                AccessoryWidgetCard(kind: .bots, family: .rectangular, bots: entry.bots).widgetAccentable()
            case .systemLarge:
                BotsWidgetCard(bots: entry.bots, wide: true, large: true, links: entry.links)
            case .systemMedium:
                BotsWidgetCard(bots: entry.bots, wide: true, links: entry.links)
            default:
                BotsWidgetCard(bots: entry.bots, wide: false)
                    .widgetURL(ordered.first.flatMap { entry.links[$0.id] })
            }
        }
    }

}

private struct EmptyWidget: View {
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            CharacterAvatar(shape: "blob", color: "gray", size: 26, mood: .idle)
            Spacer(minLength: 0)
            Text(text).font(.caption).foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - Usage limits

struct UsageEntry: TimelineEntry {
    let date: Date
    let usage: Usage
}

struct UsageIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Usage limits"
}

struct UsageTimeline: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> UsageEntry {
        UsageEntry(date: .now, usage: .widgetPreview)
    }

    func snapshot(for configuration: UsageIntent, in context: Context) async -> UsageEntry {
        UsageEntry(date: .now, usage: context.isPreview ? .widgetPreview : (SharedStore.usage ?? Usage()))
    }

    /// Refreshes every 30 minutes, or right after the next limit resets if that's sooner.
    func timeline(for configuration: UsageIntent, in context: Context) async -> Timeline<UsageEntry> {
        await Self.timeline()
    }

    static func timeline() async -> Timeline<UsageEntry> {
        let storage = SharedStore.activeContext
        let computer = storage.pairing?.token
        let fetched = await fetch(storage: storage)
        guard SharedStore.activeContext.id == storage.id, storage.pairing?.token == computer else {
            return Timeline(entries: [UsageEntry(date: .now, usage: Usage())], policy: .after(.now + 5))
        }
        let usage = fetched ?? storage.usage ?? Usage()
        let nextReset = usage.providers.flatMap(\.windows).compactMap(\.resetDate).filter { $0 > .now }.min()
        var next = Date.now + (fetched == nil ? 5 * 60 : 30 * 60)
        if let nextReset, nextReset + 1 < next { next = max(nextReset + 1, .now + 5 * 60) }
        return Timeline(entries: [UsageEntry(date: .now, usage: usage)], policy: .after(next))
    }

    /// Asks the paired host directly (works over Tailscale), falling back to the app's cache.
    /// `refresh` makes the host re-read the limits first instead of answering from its cache.
    static func fetch(refresh: Bool = false, storage: SharedStore.Context = SharedStore.activeContext) async -> Usage? {
        guard let pairing = storage.orderedPairing, let client = await HostClient.resolve(pairing),
              let usage = try? await client.usage(refresh: refresh),
              SharedStore.activeContext.id == storage.id, storage.pairing?.token == pairing.token else { return nil }
        storage.usage = usage
        return usage
    }
}

struct UsageWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "CodyncUsage", intent: UsageIntent.self, provider: UsageTimeline()) { entry in
            UsageWidgetView(entry: entry)
                .containerBackground(Palette.surface, for: .widget)
        }
        .configurationDisplayName("Usage limits")
        .description("Claude and Codex subscription limits from your computer.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryCircular, .accessoryRectangular, .accessoryInline])
    }
}

/// The tightest limit is the headline (after CodexBar); the rest are thin grey bars
/// that only take color near the limit.
struct UsageWidgetView: View {
    let entry: UsageEntry
    @Environment(\.widgetFamily) private var family

    private var windows: [(provider: String, window: UsageWindow)] {
        entry.usage.providers.flatMap { p in p.windows.map { (p.name, $0) } }
    }

    private var tightest: (provider: String, window: UsageWindow)? { windows.max { $0.window.percent < $1.window.percent } }
    private var rest: [(provider: String, window: UsageWindow)] {
        windows.filter { $0.window.id != tightest?.window.id || $0.provider != tightest?.provider }
    }

    var body: some View {
        if let top = tightest {
            switch family {
            case .accessoryInline:
                AccessoryWidgetCard(kind: .usage, family: .inline, usage: entry.usage).widgetAccentable()
            case .accessoryCircular:
                AccessoryWidgetCard(kind: .usage, family: .circular, usage: entry.usage).widgetAccentable()
            case .accessoryRectangular:
                AccessoryWidgetCard(kind: .usage, family: .rectangular, usage: entry.usage).widgetAccentable()
            case .systemMedium:
                HStack(alignment: .top, spacing: 18) {
                    hero(top).frame(width: 118, alignment: .leading)
                    VStack(alignment: .leading, spacing: 9) {
                        ForEach(Array(rest.prefix(4).enumerated()), id: \.offset) { _, item in bar(item) }
                        Spacer(minLength: 0)
                    }
                }
            default:
                VStack(alignment: .leading, spacing: 0) {
                    hero(top)
                    Spacer(minLength: 6)
                    VStack(alignment: .leading, spacing: 7) {
                        ForEach(Array(rest.prefix(2).enumerated()), id: \.offset) { _, item in bar(item) }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        } else if family == .accessoryInline || family == .accessoryCircular || family == .accessoryRectangular {
            Text("No usage yet")
        } else {
            EmptyWidget(text: SharedStore.pairing == nil ? "Open Codync to pair with your computer." : "No usage reported yet.")
        }
    }

    private func hero(_ item: (provider: String, window: UsageWindow)) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(item.provider) · \(item.window.label)").font(.caption).foregroundStyle(Palette.secondary).lineLimit(1)
            Text(percent(item.window))
                .font(.system(size: 34, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(item.window.percent >= 90 ? Palette.danger : Palette.text)
                .minimumScaleFactor(0.7)
            resetLine(item.window).font(.caption2).foregroundStyle(Palette.tertiary).lineLimit(1)
        }
    }

    private func bar(_ item: (provider: String, window: UsageWindow)) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("\(item.provider) \(item.window.label)").lineLimit(1)
                Spacer(minLength: 4)
                Text(percent(item.window)).monospacedDigit()
            }
            .font(.caption2)
            .foregroundStyle(Palette.secondary)
            UsageTicks(percent: item.window.percent, tint: Palette.usageTint(item.window.percent), height: 10)
        }
    }

    private func percent(_ w: UsageWindow) -> String { "\(Int(w.percent.rounded()))%" }

    @ViewBuilder private func resetLine(_ w: UsageWindow) -> some View {
        if let date = w.resetDate, date > entry.date {
            Text("resets \(Text(date, style: .relative))")
        } else if let text = w.resetDescription {
            Text(text)
        } else {
            Text(" ")
        }
    }
}

// MARK: - One provider

enum UsageProviderChoice: String, AppEnum {
    case claude, codex

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Provider"
    static let caseDisplayRepresentations: [Self: DisplayRepresentation] = [.claude: "Claude", .codex: "Codex"]
}

struct ProviderUsageIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Provider usage"

    @Parameter(title: "Provider", default: .claude)
    var provider: UsageProviderChoice
}

struct ProviderUsageEntry: TimelineEntry {
    let date: Date
    let usage: Usage
    let provider: UsageProviderChoice
}

struct ProviderUsageTimeline: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> ProviderUsageEntry {
        ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .claude)
    }

    func snapshot(for configuration: ProviderUsageIntent, in context: Context) async -> ProviderUsageEntry {
        let usage = context.isPreview ? .widgetPreview : (SharedStore.usage ?? Usage())
        return ProviderUsageEntry(date: .now, usage: usage, provider: configuration.provider)
    }

    func timeline(for configuration: ProviderUsageIntent, in context: Context) async -> Timeline<ProviderUsageEntry> {
        let shared = await UsageTimeline.timeline()
        return Timeline(entries: shared.entries.map { ProviderUsageEntry(date: $0.date, usage: $0.usage, provider: configuration.provider) },
                        policy: shared.policy)
    }
}

struct ProviderUsageWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "CodyncProviderUsage", intent: ProviderUsageIntent.self, provider: ProviderUsageTimeline()) { entry in
            ProviderUsageView(entry: entry)
        }
        .configurationDisplayName("Provider usage")
        .description("Session and weekly limits in a compact, easy-to-read card.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

/// Uses the exact same card as the in-app widget gallery.
struct ProviderUsageView: View {
    let entry: ProviderUsageEntry
    @Environment(\.widgetFamily) private var family
    private var provider: UsageProvider? { entry.usage.providers.first { $0.id == entry.provider.rawValue } }

    var body: some View {
        Group {
            if let provider, !provider.windows.isEmpty {
                ProviderWidgetCard(provider: provider, layout: family == .systemSmall ? .small : family == .systemLarge ? .large : .medium, date: entry.date)
            } else {
                EmptyWidget(text: "Open Codync to connect a computer and check \(entry.provider.rawValue.capitalized) usage.")
            }
        }
        .containerBackground(Palette.surface, for: .widget)
        .widgetURL(URL(string: "codync://usage"))
    }
}

// MARK: - Live Activity

struct BotLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: BotActivityAttributes.self) { context in
            BotActivityCard(name: context.attributes.name, shape: context.attributes.avatarShape,
                            color: context.attributes.avatarColor, state: presentation(context))
                .activityBackgroundTint(Palette.surface)
                .widgetURL(context.attributes.link ?? URL(string: "codync://computers"))
        } dynamicIsland: { context in
            let state = presentation(context)
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 8) {
                        avatar(context, size: 26)
                        Text(context.attributes.name).font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(.white).lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    BotActivityIndicator(state: state).environment(\.colorScheme, .dark)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 10) {
                        BotActivityDetail(state: state)
                        if let link = context.attributes.link {
                            Link(destination: link) {
                                Label(state.phase == .needsInput ? "Respond in Codync" : "Open conversation", systemImage: "arrow.up.right")
                                    .font(.system(size: 12, weight: .medium))
                            }
                            .tint(.white)
                        }
                    }
                    .environment(\.colorScheme, .dark)
                }
            } compactLeading: {
                avatar(context, size: 20)
            } compactTrailing: {
                BotActivityIndicator(state: state).environment(\.colorScheme, .dark)
            } minimal: {
                BotActivityIndicator(state: state, minimal: true)
                    .environment(\.colorScheme, .dark)
                    .accessibilityLabel("\(context.attributes.name), \(state.title)")
            }
            .widgetURL(context.attributes.link ?? URL(string: "codync://computers"))
        }
    }

    private func presentation(_ context: ActivityViewContext<BotActivityAttributes>) -> BotActivityPresentation {
        .init(status: context.state.status, activity: context.state.activity,
              startedAt: context.state.startedAt, isStale: context.isStale)
    }

    private func avatar(_ context: ActivityViewContext<BotActivityAttributes>, size: CGFloat) -> some View {
        CharacterAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, size: size)
    }
}
