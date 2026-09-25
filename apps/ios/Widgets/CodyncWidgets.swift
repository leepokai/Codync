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

    /// The app's snapshot of every computer's bots in the selected account. Bot IDs are only
    /// unique per computer, so each copy is re-keyed `<computerId>/<botId>` for the cards and links.
    static var current: BotsEntry {
        let storage = SharedStore.activeContext
        var bots: [Bot] = []
        var links: [String: URL] = [:]
        for snapshot in storage.bots {
            var bot = snapshot.bot
            bot.id = "\(snapshot.computerId)/\(snapshot.bot.id)"
            links[bot.id] = storage.botURL(BotReference(accountId: storage.accountID, computerId: snapshot.computerId, botId: snapshot.bot.id))
            bots.append(bot)
        }
        return BotsEntry(date: .now, bots: bots, paired: !storage.computers.isEmpty, links: links)
    }
}

struct BotsTimeline: TimelineProvider {
    func placeholder(in context: Context) -> BotsEntry { BotsEntry(date: .now, bots: Bot.widgetPreview, paired: true) }

    func getSnapshot(in context: Context, completion: @escaping (BotsEntry) -> Void) {
        completion(context.isPreview ? placeholder(in: context) : .current)
    }

    /// The app writes the roster and reloads this widget when a bot's state changes.
    func getTimeline(in context: Context, completion: @escaping (Timeline<BotsEntry>) -> Void) {
        completion(Timeline(entries: [.current], policy: .never))
    }
}

struct BotsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "CodyncBots", provider: BotsTimeline()) { entry in
            BotsWidgetView(entry: entry)
                .containerBackground(Palette.surface, for: .widget)
        }
        .configurationDisplayName("Bots")
        .description("Which bots need you, which are working, and what they said last.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryCircular, .accessoryRectangular, .accessoryInline])
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
    private var needing: Int { ordered.filter(\.needsInput).count }
    private var working: Int { ordered.filter { $0.isWorking && !$0.needsInput }.count }

    /// The one line that sums up the roster.
    private var headline: (count: Int, label: String, needs: Bool) {
        if needing > 0 { return (needing, needing == 1 ? "needs you" : "need you", true) }
        if working > 0 { return (working, "working", false) }
        return (ordered.count, ordered.count == 1 ? "bot, all quiet" : "bots, all quiet", false)
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
                Text(needing > 0 ? "\(needing) bot\(needing == 1 ? "" : "s") need\(needing == 1 ? "s" : "") you" : working > 0 ? "\(working) working" : ordered.isEmpty ? "No bots yet" : "Bots all quiet")
            case .accessoryCircular:
                VStack(spacing: 0) {
                    Text("\(headline.count)").font(.system(size: 24, weight: .semibold, design: .rounded))
                    Text(needing > 0 ? "need" : working > 0 ? "busy" : "bots").font(.system(size: 10, weight: .medium))
                }
                .widgetAccentable()
            case .accessoryRectangular:
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(headline.count) \(headline.label)").font(.headline).widgetAccentable()
                    ForEach(ordered.prefix(2)) { bot in
                        Text("\(bot.name) · \(line(bot))").font(.caption2).lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            case .systemMedium:
                BotsWidgetCard(bots: entry.bots, wide: true, links: entry.links)
            default:
                BotsWidgetCard(bots: entry.bots, wide: false)
                    .widgetURL(ordered.first.flatMap { entry.links[$0.id] })
            }
        }
    }

    private func line(_ bot: Bot) -> String {
        if bot.needsInput { return bot.activity.isEmpty ? "Needs your approval" : bot.activity }
        if bot.isWorking { return bot.activity.isEmpty ? "Working…" : bot.activity }
        return bot.lastMessage ?? "Idle"
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
        UsageEntry(date: .now, usage: context.isPreview ? .widgetPreview : (Self.cached ?? Usage()))
    }

    /// Refreshes every 30 minutes, or right after the next limit resets if that's sooner.
    func timeline(for configuration: UsageIntent, in context: Context) async -> Timeline<UsageEntry> {
        await Self.timeline()
    }

    static func timeline() async -> Timeline<UsageEntry> {
        let storage = SharedStore.activeContext
        let computer = storage.lastComputerId
        let fetched = await fetch(storage: storage)
        // The app switched account or computer meanwhile: redraw from the new one shortly.
        guard SharedStore.activeContext.id == storage.id, storage.lastComputerId == computer else {
            return Timeline(entries: [UsageEntry(date: .now, usage: Usage())], policy: .after(.now + 5))
        }
        let usage = fetched ?? cached ?? Usage()
        let nextReset = usage.providers.flatMap(\.windows).compactMap(\.resetDate).filter { $0 > .now }.min()
        var next = Date.now + (fetched == nil ? 5 * 60 : 30 * 60)
        if let nextReset, nextReset + 1 < next { next = max(nextReset + 1, .now + 5 * 60) }
        return Timeline(entries: [UsageEntry(date: .now, usage: usage)], policy: .after(next))
    }

    /// The last active computer's usage, as the app last saw it.
    static var cached: Usage? {
        let storage = SharedStore.activeContext
        return storage.lastComputerId.flatMap { storage.usage[$0] }
    }

    /// Asks the last active computer directly or through the relay, one call, then disconnects;
    /// nil falls back to the app's cache. `refresh` makes the host re-read the limits first.
    static func fetch(refresh: Bool = false, storage: SharedStore.Context = SharedStore.activeContext) async -> Usage? {
        // Keys exist once a computer is saved; the widget never creates them.
        guard let id = storage.lastComputerId, let computer = storage.computers.first(where: { $0.id == id }),
              let identity = try? DeviceIdentity.load(context: storage),
              let transport = try? await HostConnector.connect(computer, identity: identity) else { return nil }
        let usage = try? await HostClient(transport: transport).usage(refresh: refresh)
        await transport.shutdown()
        guard let usage, SharedStore.activeContext.id == storage.id, storage.lastComputerId == id else { return nil }
        storage.usage[id] = usage
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
                Text("\(top.provider) \(percent(top.window)) · \(top.window.label)")
            case .accessoryCircular:
                Gauge(value: min(top.window.percent, 100), in: 0...100) {
                    Text(top.provider.prefix(2))
                } currentValueLabel: {
                    Text("\(Int(top.window.percent.rounded()))")
                }
                .gaugeStyle(.accessoryCircularCapacity)
            case .accessoryRectangular:
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(top.provider) \(top.window.label) \(percent(top.window))").font(.headline).widgetAccentable()
                    Text(rest.prefix(2).map { "\($0.window.label) \(percent($0.window))" }.joined(separator: " · "))
                        .font(.caption2).lineLimit(1)
                    resetLine(top.window).font(.caption2)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
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
            EmptyWidget(text: SharedStore.activeContext.computers.isEmpty ? "Open Codync to pair with your computer." : "No usage reported yet.")
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
        let usage = context.isPreview ? .widgetPreview : (UsageTimeline.cached ?? Usage())
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
        .supportedFamilies([.systemSmall, .systemMedium])
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
                ProviderWidgetCard(provider: provider, layout: family == .systemSmall ? .small : .medium, date: entry.date)
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
            LockScreenView(context: context)
                .activityBackgroundTint(Palette.background)
                .widgetURL(context.attributes.link ?? URL(string: "codync://computers"))
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
                    Image(systemName: "checkmark").foregroundStyle(Palette.text)
                }
            } minimal: {
                CharacterAvatar(shape: context.attributes.avatarShape, color: context.attributes.avatarColor, size: 20)
            }
            .widgetURL(context.attributes.link ?? URL(string: "codync://computers"))
        }
    }
}

/// Status only: Live Activity pushes never carry free text (spec §6.7).
private func statusText(_ s: BotActivityAttributes.ContentState) -> String {
    switch s.status {
    case "needsInput": "Needs your approval"
    case "working": "Working…"
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
