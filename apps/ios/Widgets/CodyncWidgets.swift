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

/// A bot on one of the account's computers; bot IDs are only unique per computer.
struct WidgetBot: Identifiable {
    let bot: Bot
    let link: URL?
    let id: String

    init(_ snapshot: BotSnapshot, storage: SharedStore.Context?) {
        bot = snapshot.bot
        link = storage?.botURL(BotReference(accountId: storage?.accountID, computerId: snapshot.computerId, botId: snapshot.bot.id))
        id = "\(snapshot.computerId)/\(snapshot.bot.id)"
    }
}

struct BotsEntry: TimelineEntry {
    let date: Date
    let bots: [WidgetBot]
    let paired: Bool

    /// The app's snapshot of every computer's bots in the selected account.
    static func current(preview: Bool = false) -> BotsEntry {
        let storage = SharedStore.activeContext
        let bots = storage.bots.map { WidgetBot($0, storage: storage) }
        if bots.isEmpty && preview { return .sample }
        return BotsEntry(date: .now, bots: bots, paired: !storage.computers.isEmpty)
    }

    static var sample: BotsEntry {
        BotsEntry(date: .now, bots: Bot.samples.map { WidgetBot(BotSnapshot(computerId: "sample", bot: $0), storage: nil) }, paired: true)
    }
}

struct BotsTimeline: TimelineProvider {
    func placeholder(in context: Context) -> BotsEntry { .sample }

    func getSnapshot(in context: Context, completion: @escaping (BotsEntry) -> Void) {
        completion(.current(preview: context.isPreview))
    }

    /// The app writes the roster and reloads this widget when a bot's state changes.
    func getTimeline(in context: Context, completion: @escaping (Timeline<BotsEntry>) -> Void) {
        completion(Timeline(entries: [.current()], policy: .never))
    }
}

struct BotsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "CodyncBots", provider: BotsTimeline()) { entry in
            BotsWidgetView(entry: entry)
                .containerBackground(Palette.background, for: .widget)
        }
        .configurationDisplayName("Bots")
        .description("Which bots need you, which are working, and what they said last.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryCircular, .accessoryRectangular, .accessoryInline])
    }
}

struct BotsWidgetView: View {
    let entry: BotsEntry
    @Environment(\.widgetFamily) private var family

    private var bots: [Bot] { entry.bots.map(\.bot) }

    /// Needs you first, then working, then most recent.
    private var ordered: [WidgetBot] {
        entry.bots.filter { !$0.bot.hidden }.sorted { a, b in
            let rank = { (b: Bot) in b.needsInput ? 0 : b.isWorking ? 1 : 2 }
            return rank(a.bot) != rank(b.bot) ? rank(a.bot) < rank(b.bot) : a.bot.lastAt > b.bot.lastAt
        }
    }
    private var needing: Int { bots.filter(\.needsInput).count }
    private var working: Int { bots.filter { $0.isWorking && !$0.needsInput }.count }

    /// The one line that sums up the roster.
    private var headline: (count: Int, label: String, needs: Bool) {
        if needing > 0 { return (needing, needing == 1 ? "needs you" : "need you", true) }
        if working > 0 { return (working, "working", false) }
        return (bots.count, bots.count == 1 ? "bot, all quiet" : "bots, all quiet", false)
    }

    var body: some View {
        if !entry.paired {
            EmptyWidget(text: "Open Codync to pair with your computer.")
        } else {
            switch family {
            case .accessoryInline:
                Text(needing > 0 ? "\(needing) bot\(needing == 1 ? "" : "s") need\(needing == 1 ? "s" : "") you" : working > 0 ? "\(working) working" : "Bots all quiet")
            case .accessoryCircular:
                VStack(spacing: 0) {
                    Text("\(headline.count)").font(.system(size: 24, weight: .semibold, design: .rounded))
                    Text(needing > 0 ? "need" : working > 0 ? "busy" : "bots").font(.system(size: 10, weight: .medium))
                }
                .widgetAccentable()
            case .accessoryRectangular:
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(headline.count) \(headline.label)").font(.headline).widgetAccentable()
                    ForEach(ordered.prefix(2)) { item in
                        Text("\(item.bot.name) · \(line(item.bot))").font(.caption2).lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            case .systemMedium:
                HStack(alignment: .top, spacing: 16) {
                    summary.frame(width: 104, alignment: .leading)
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(ordered.prefix(3)) { item in
                            Link(destination: item.link ?? URL(string: "codync://computers")!) { BotLine(bot: item.bot, detail: line(item.bot)) }
                        }
                        Spacer(minLength: 0)
                    }
                }
            default:
                VStack(alignment: .leading, spacing: 0) {
                    HStack(spacing: -6) {
                        ForEach(ordered.prefix(3)) { item in
                            CharacterAvatar(bot: item.bot, size: 30, animated: false)
                        }
                    }
                    Spacer(minLength: 6)
                    summary
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .widgetURL(ordered.first?.link)
            }
        }
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(headline.count)")
                .font(.system(size: 40, weight: .semibold, design: .rounded))
                .foregroundStyle(headline.needs ? Palette.warning : Palette.text)
                .contentTransition(.numericText())
            Text(headline.label).font(.subheadline).foregroundStyle(Palette.secondary)
            if family == .systemSmall, let first = ordered.first?.bot, first.isWorking {
                Text(first.name).font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1).padding(.top, 2)
            }
        }
    }

    private func line(_ bot: Bot) -> String {
        if bot.needsInput { return bot.activity.isEmpty ? "Needs your approval" : bot.activity }
        if bot.isWorking { return bot.activity.isEmpty ? "Working…" : bot.activity }
        return bot.lastMessage ?? "Idle"
    }
}

private struct BotLine: View {
    let bot: Bot
    let detail: String

    var body: some View {
        HStack(spacing: 10) {
            CharacterAvatar(bot: bot, size: 28, animated: false)
            VStack(alignment: .leading, spacing: 1) {
                Text(bot.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(bot.needsInput ? Palette.warning : Palette.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
    }
}

private struct EmptyWidget: View {
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            CharacterAvatar(shape: "blob", color: "gray", size: 30)
            Spacer(minLength: 0)
            Text(text).font(.caption).foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

extension Bot {
    static let samples: [Bot] = {
        let json = """
        [{"id":"1","name":"Reviewer","avatarShape":"blob","avatarColor":"green","backend":"claude","cwd":"/","status":"needsInput","activity":"Run cargo test --all?","lastAt":3},
         {"id":"2","name":"Fixer","avatarShape":"squircle","avatarColor":"orange","backend":"codex","cwd":"/","status":"working","activity":"Editing store.rs","lastAt":2},
         {"id":"3","name":"Docs writer","avatarShape":"teardrop","avatarColor":"violet","backend":"claude","cwd":"/","status":"idle","lastMessage":"README updated.","lastAt":1}]
        """
        return (try? JSONDecoder().decode([Bot].self, from: Data(json.utf8))) ?? []
    }()
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
        UsageEntry(date: .now, usage: .sample)
    }

    func snapshot(for configuration: UsageIntent, in context: Context) async -> UsageEntry {
        UsageEntry(date: .now, usage: Self.cached ?? .sample)
    }

    /// Refreshes every 30 minutes, or right after the next limit resets if that's sooner.
    func timeline(for configuration: UsageIntent, in context: Context) async -> Timeline<UsageEntry> {
        await Self.timeline()
    }

    static func timeline() async -> Timeline<UsageEntry> {
        let fetched = await fetch()
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
    static func fetch(refresh: Bool = false) async -> Usage? {
        let storage = SharedStore.activeContext
        // Keys exist once a computer is saved; the widget never creates them.
        guard let id = storage.lastComputerId, let computer = storage.computers.first(where: { $0.id == id }),
              let identity = try? DeviceIdentity.load(context: storage),
              let transport = try? await HostConnector.connect(computer, identity: identity) else { return nil }
        let usage = try? await HostClient(transport: transport).usage(refresh: refresh)
        await transport.shutdown()
        guard let usage else { return nil }
        storage.usage[id] = usage
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
                .font(.system(size: 40, weight: .semibold, design: .rounded))
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
            UsageBar(percent: item.window.percent, height: 4)
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
        ProviderUsageEntry(date: .now, usage: .preview, provider: .claude)
    }

    func snapshot(for configuration: ProviderUsageIntent, in context: Context) async -> ProviderUsageEntry {
        let usage = UsageTimeline.cached.flatMap { $0.providers.isEmpty ? nil : $0 } ?? .preview
        return ProviderUsageEntry(date: .now, usage: usage, provider: configuration.provider)
    }

    func timeline(for configuration: ProviderUsageIntent, in context: Context) async -> Timeline<ProviderUsageEntry> {
        let shared = await UsageTimeline.timeline()
        return Timeline(entries: shared.entries.map { ProviderUsageEntry(date: $0.date, usage: $0.usage, provider: configuration.provider) },
                        policy: shared.policy)
    }
}

/// The widget's refresh button: asks the host to re-read the limits; WidgetKit
/// reloads the widget when this returns.
struct RefreshUsageIntent: AppIntent {
    static let title: LocalizedStringResource = "Refresh usage"

    func perform() async throws -> some IntentResult {
        _ = await UsageTimeline.fetch(refresh: true)
        return .result()
    }
}

extension Usage {
    static let preview: Usage = {
        let json = """
        {"providers":[{"id":"claude","name":"Claude","source":"claude","updatedAt":0,"windows":[
          {"id":"five_hour","label":"5-hour","percent":16},{"id":"seven_day","label":"Weekly","percent":59}]},
         {"id":"codex","name":"Codex","source":"sessions","updatedAt":0,"windows":[
          {"id":"primary","label":"5-hour","percent":8},{"id":"secondary","label":"Weekly","percent":70}]}]}
        """
        return (try? JSONDecoder().decode(Usage.self, from: Data(json.utf8))) ?? Usage()
    }()
}

struct ProviderUsageWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "CodyncProviderUsage", intent: ProviderUsageIntent.self, provider: ProviderUsageTimeline()) { entry in
            ProviderUsageView(entry: entry)
        }
        .configurationDisplayName("Provider usage")
        .description("One provider's session and weekly limits, with its character.")
        .supportedFamilies([.systemMedium])
    }
}

/// A provider card: its character and name on the left, the two main limits on the right.
struct ProviderUsageView: View {
    let entry: ProviderUsageEntry

    private var provider: UsageProvider? { entry.usage.providers.first { $0.id == entry.provider.rawValue } }

    var body: some View {
        let tint = provider?.tint ?? Palette.accent
        Group {
            if let provider {
                HStack(spacing: 18) {
                    VStack(alignment: .leading, spacing: 2) {
                        ProviderMascot(provider, size: 52)
                        Spacer(minLength: 4)
                        Text(provider.name).font(.system(.title3, design: .serif, weight: .bold)).foregroundStyle(Palette.text)
                        Text("Usage").font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                    .frame(width: 80, alignment: .leading)
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(provider.windows.prefix(2)) { window(provider, $0) }
                        Spacer(minLength: 0)
                        HStack {
                            Text("updated \(Date(milliseconds: provider.updatedAt).formatted(date: .omitted, time: .shortened))")
                            Spacer()
                            Button(intent: RefreshUsageIntent()) {
                                Image(systemName: "arrow.clockwise").font(.subheadline.weight(.semibold))
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Refresh")
                        }
                        .font(.caption2)
                        .foregroundStyle(Palette.secondary)
                    }
                }
            } else {
                EmptyWidget(text: SharedStore.activeContext.computers.isEmpty ? "Open Codync to pair with your computer."
                    : "No \(entry.provider.rawValue.capitalized) usage reported yet.")
            }
        }
        .containerBackground(for: .widget) {
            LinearGradient(colors: [tint.opacity(0.16), tint.opacity(0.03), tint.opacity(0.12)],
                           startPoint: .topLeading, endPoint: .bottomTrailing)
                .background(Palette.background)
        }
        .widgetURL(URL(string: "codync://usage"))
    }

    private func window(_ provider: UsageProvider, _ w: UsageWindow) -> some View {
        let color = w.percent >= 90 ? Palette.danger : provider.tint
        return VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(w.title).font(.system(.subheadline, design: .serif, weight: .bold)).foregroundStyle(Palette.text)
                Text([w.span, w.resetsShort(now: entry.date)].compactMap(\.self).joined(separator: " · "))
                    .font(.caption2)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                Spacer(minLength: 4)
                Text("\(Text("\(Int(w.percent.rounded()))").font(.system(.title3, design: .serif, weight: .bold)))\(Text("%").font(.system(.caption, design: .serif, weight: .bold)))")
                    .foregroundStyle(color)
                    .monospacedDigit()
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.text.opacity(0.1))
                    Capsule().fill(color).frame(width: geo.size.width * min(1, max(0.03, w.percent / 100)))
                }
            }
            .frame(height: 7)
        }
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
