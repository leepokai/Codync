import AppIntents
import SwiftUI

/// Shared by the widget extension and its in-app gallery so previews use the
/// same typography, spacing and progress rendering as the installed widget.
public struct ProviderWidgetCard: View {
    public enum Layout { case small, medium, large }
    let provider: UsageProvider
    let layout: Layout
    let date: Date
    let iconStyle: UsageIconStyle

    public init(provider: UsageProvider, layout: Layout, date: Date = .now, iconStyle: UsageIconStyle = SharedStore.usageIconStyle) {
        self.provider = provider
        self.layout = layout
        self.date = date
        self.iconStyle = iconStyle
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: layout == .small ? 6 : 8) {
            HStack(spacing: 7) {
                ProviderMascot(provider, size: 22, style: iconStyle)
                Text(provider.name)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                Spacer(minLength: 0)
                if layout != .small {
                    Text(L10n.tr("Usage"))
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(Palette.secondary)
                }
            }
            if layout == .small, let window = provider.tightest {
                Spacer(minLength: 0)
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.tr(window.title)).font(.system(size: 11)).foregroundStyle(Palette.secondary).lineLimit(1)
                    HStack(alignment: .firstTextBaseline, spacing: 1) {
                        Text(L10n.tr("\(Int(window.percent.rounded()))"))
                            .font(.system(size: 32, weight: .semibold, design: .rounded))
                            .contentTransition(.numericText(value: window.percent))
                        Text(L10n.tr("%")).font(.system(size: 17, weight: .medium))
                    }
                    .monospacedDigit()
                    .foregroundStyle(window.percent >= 90 ? Palette.danger : Palette.text)
                    .minimumScaleFactor(0.7)
                }
                UsageTicks(percent: window.percent, tint: provider.widgetTint, height: 13)
                Text(window.resetsShort(now: date).map(L10n.resetLine) ?? L10n.tr("Usage reported by your computer"))
                    .font(.system(size: 9)).foregroundStyle(Palette.secondary)
                    .lineLimit(1)
            } else {
                if layout == .large, let top = provider.tightest {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(L10n.tr("\(Int(top.percent.rounded()))%"))
                            .font(.system(size: 36, weight: .semibold, design: .rounded)).monospacedDigit()
                            .contentTransition(.numericText(value: top.percent))
                            .foregroundStyle(top.percent >= 90 ? Palette.danger : Palette.text)
                        Text("\(L10n.tr(top.title)) · \(L10n.tr("highest usage"))").font(.system(size: 11)).foregroundStyle(Palette.secondary)
                            .lineLimit(2)
                    }
                    .padding(.vertical, 4)
                }
                ForEach(provider.limits.prefix(layout == .large ? 4 : 2)) { window in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(alignment: .firstTextBaseline) {
                            Text(L10n.tr(window.title))
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(Palette.secondary)
                                .lineLimit(1)
                            Spacer(minLength: 4)
                            Text(L10n.tr("\(Int(window.percent.rounded()))%"))
                                .font(.system(size: 13, weight: .semibold))
                                .monospacedDigit()
                                .contentTransition(.numericText(value: window.percent))
                                .foregroundStyle(window.percent >= 90 ? Palette.danger : Palette.text)
                        }
                        UsageTicks(percent: window.percent, tint: provider.widgetTint, height: 12)
                        if let reset = window.resetsShort(now: date) {
                            Text(L10n.resetLine(reset)).font(.system(size: 9)).foregroundStyle(Palette.secondary)
                                .lineLimit(1).frame(maxWidth: .infinity, alignment: .trailing)
                        }
                    }
                    .padding(.bottom, layout == .large ? 2 : 0)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        // A new timeline entry rolls the numbers and fills the ticks instead of swapping.
        .animation(Motion.activityPhase, value: provider)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(provider.name) usage")
        .accessibilityValue(accessibleUsage)
    }

    private var accessibleUsage: String {
        let windows = layout == .small ? provider.tightest.map { [$0] } ?? [] : Array(provider.limits.prefix(layout == .large ? 4 : 2))
        return windows.map { window in
            "\(window.title), \(Int(window.percent.rounded())) percent used" +
                (window.resetsShort(now: date).map { ", \($0)" } ?? "")
        }.joined(separator: "; ")
    }
}

/// Fine, separated ticks keep low and high usage legible without a heavy gauge.
public struct UsageTicks: View {
    let percent: Double
    let tint: Color
    let height: CGFloat

    public init(percent: Double, tint: Color, height: CGFloat = 16) {
        self.percent = percent
        self.tint = tint
        self.height = height
    }

    public var body: some View {
        GeometryReader { geometry in
            let count = max(1, Int(geometry.size.width / 4))
            let value = percent.isFinite ? min(100, max(0, percent)) : 0
            let filled = Int((Double(count) * value / 100).rounded())
            let color = value >= 90 ? Palette.danger : tint
            HStack(spacing: 1.5) {
                ForEach(0..<count, id: \.self) { i in
                    RoundedRectangle(cornerRadius: 1)
                        .fill(i < filled ? color : color.opacity(0.16))
                }
            }
        }
        .frame(height: height)
        .accessibilityLabel("Usage")
        .accessibilityValue("\(Int((percent.isFinite ? percent : 0).rounded())) percent")
    }
}

public extension UsageProvider {
    var widgetTint: Color { id == "codex" ? Color(light: 0x326FAD, dark: 0x70ACDC) : tint }
}

public extension Usage {
    /// Illustrative data only: widget placeholders and explicitly labelled previews.
    static var widgetPreview: Usage {
        let json = """
        {"providers":[{"id":"claude","name":"Claude","source":"claude","updatedAt":0,"windows":[
          {"id":"five_hour","label":"5-hour","percent":16,"resetsText":"3h 20m"},{"id":"seven_day","label":"Weekly","percent":59,"resetsText":"2d 6h"}]},
         {"id":"codex","name":"Codex","source":"sessions","updatedAt":0,"windows":[
          {"id":"primary","label":"5-hour","percent":8,"resetsText":"2h 10m"},{"id":"secondary","label":"Weekly","percent":27,"resetsText":"4d 2h"}]}]}
        """
        return (try? JSONDecoder().decode(Usage.self, from: Data(json.utf8))) ?? Usage()
    }
}

public struct BotsWidgetCard: View {
    let bots: [Bot]
    let wide: Bool
    let large: Bool
    let links: [String: URL]

    public init(bots: [Bot], wide: Bool, large: Bool = false, links: [String: URL] = [:]) {
        self.bots = bots.filter { !$0.hidden }
        self.wide = wide
        self.large = large
        self.links = links
    }

    private var ordered: [Bot] {
        bots.sorted {
            let rank: (Bot) -> Int = { $0.needsInput ? 0 : $0.isWorking ? 1 : 2 }
            return rank($0) == rank($1) ? $0.lastAt > $1.lastAt : rank($0) < rank($1)
        }
    }
    private var needs: Int { bots.filter(\.needsInput).count }
    private var working: Int { bots.filter { $0.isWorking && !$0.needsInput }.count }
    private var count: Int { needs > 0 ? needs : working > 0 ? working : bots.count }
    private var label: String { needs > 0 ? (needs == 1 ? "Needs you" : "Need you") : working > 0 ? "Working" : bots.isEmpty ? "No bots yet" : "All quiet" }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.tr("Bots")).font(.system(size: 13, weight: .semibold))
                Spacer()
                Image(systemName: "bubble.left.and.bubble.right")
                    .font(.system(size: 12)).foregroundStyle(Palette.secondary)
            }
            if large {
                HStack {
                    summary
                    Spacer()
                    Text(L10n.tr("\(bots.count) total · \(working) working"))
                        .font(.system(size: 11)).foregroundStyle(Palette.secondary)
                }
                Divider()
                botRows(limit: 6)
                Spacer(minLength: 0)
            } else if wide {
                HStack(alignment: .center, spacing: 16) {
                    summary.frame(width: 76, alignment: .leading)
                    Rectangle().fill(Palette.border).frame(width: 1)
                    botRows(limit: 3)
                }
            } else {
                Spacer(minLength: 0)
                summary
                HStack(spacing: -4) {
                    ForEach(ordered.prefix(3)) { bot in
                        CharacterAvatar(bot: bot, size: 23, still: true)
                    }
                    Spacer(minLength: 0)
                    if working > 0 && needs > 0 {
                        Text(L10n.tr("\(working) working")).font(.system(size: 9)).foregroundStyle(Palette.secondary)
                    }
                }
            }
        }
        .foregroundStyle(Palette.text)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        // A new timeline entry moves rows into their new order and morphs counts and states.
        .animation(Motion.activityPhase, value: bots)
    }

    private func botRows(limit: Int) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(ordered.prefix(limit)) { bot in
                Group {
                    if let link = links[bot.id] {
                        Link(destination: link) { row(bot) }
                    } else { row(bot) }
                }
                .transition(.activityRow)
            }
            if bots.isEmpty {
                Text(L10n.tr("Create your first bot in Codync."))
                    .font(.system(size: 12)).foregroundStyle(Palette.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(L10n.tr("\(count)")).font(.system(size: 36, weight: .semibold, design: .rounded))
                .monospacedDigit().foregroundStyle(needs > 0 ? Palette.attention : Palette.text)
                .contentTransition(.numericText(value: Double(count)))
            Text(L10n.tr(label)).font(.system(size: 12, weight: .medium)).foregroundStyle(Palette.secondary)
                .contentTransition(.interpolate)
        }
        .accessibilityElement(children: .combine)
    }

    /// Name and step on the left, the state on the right in its color.
    private func row(_ bot: Bot) -> some View {
        let state = bot.widgetState
        return HStack(spacing: 7) {
            CharacterAvatar(bot: bot, size: 24, still: true)
            VStack(alignment: .leading, spacing: 1) {
                Text(widgetName(bot)).font(.system(size: 11, weight: .semibold)).foregroundStyle(Palette.text)
                if bot.isWorking, !bot.activity.isEmpty {
                    Text(L10n.tr(bot.activity)).font(.system(size: 10)).foregroundStyle(Palette.secondary)
                        .contentTransition(.interpolate)
                }
            }
            .lineLimit(1)
            Spacer(minLength: 4)
            Text(L10n.tr(state.label))
                .font(.system(size: 10, weight: bot.needsInput ? .semibold : .regular))
                .foregroundStyle(state.tint)
                .lineLimit(1)
                .contentTransition(.interpolate)
        }
        .accessibilityElement(children: .combine)
    }
}

/// Sample bots in the gallery have translated names. A bot the user named stays as they wrote it.
func widgetName(_ bot: Bot) -> String {
    bot.id.hasPrefix("preview-") ? L10n.tr(bot.name) : bot.name
}

public extension Bot {
    /// The state word the widgets show and its color: only what needs you (or failed) stands out.
    var widgetState: (label: String, tint: Color) {
        if needsInput { return ("Needs you", Palette.attention) }
        if status == "error" { return ("Failed", Palette.danger) }
        if isWorking { return ("Working", Palette.text) }
        return ("Ready", Palette.secondary)
    }
}

/// Four bots in a 2×2 grid, each in its mood with a dot for its state. Spots picked in the
/// widget's settings keep their bot; the rest fill with the bots that need you first, and
/// empty spots hold a faded sleeping character.
public struct BotsTeamCard: View {
    let slots: [Bot?]
    let opens: [String: any AppIntent]

    /// `opens` (bot id → the intent that opens its conversation) makes each spot a button.
    public init(bots: [Bot], picks: [String?] = [], opens: [String: any AppIntent] = [:]) {
        slots = Self.slots(bots: bots, picks: picks)
        self.opens = opens
    }

    /// The bots in their four spots: picks first (a deleted pick falls back to automatic).
    public static func slots(bots: [Bot], picks: [String?]) -> [Bot?] {
        let byId = Dictionary(bots.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let picked = (0..<4).map { i in i < picks.count ? picks[i].flatMap { byId[$0] } : nil }
        let taken = Set(picked.compactMap { $0?.id })
        var queue = bots.filter { !$0.hidden && !taken.contains($0.id) }.sorted {
            let rank: (Bot) -> Int = { $0.needsInput ? 0 : $0.isWorking ? 1 : 2 }
            return rank($0) == rank($1) ? $0.lastAt > $1.lastAt : rank($0) < rank($1)
        }[...]
        return picked.map { $0 ?? queue.popFirst() }
    }

    public var body: some View {
        Grid(horizontalSpacing: 8, verticalSpacing: 8) {
            GridRow { tile(0); tile(1) }
            GridRow { tile(2); tile(3) }
        }
        // A new timeline entry lets each bot change its mood in place.
        .animation(Motion.activityPhase, value: slots)
    }

    @ViewBuilder private func tile(_ index: Int) -> some View {
        if let bot = slots[index] {
            // A small widget has one tap target unless it uses buttons: each spot is a
            // button that opens its bot's conversation.
            if let open = opens[bot.id] {
                Button(intent: open) { botTile(bot) }.buttonStyle(.plain)
            } else {
                botTile(bot)
            }
        } else {
            CharacterAvatar(shape: "blob", color: "gray", size: 26)
                .opacity(0.18)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Palette.bubbleAgent.opacity(0.5), in: RoundedRectangle(cornerRadius: 16))
                .accessibilityHidden(true)
        }
    }

    private func botTile(_ bot: Bot) -> some View {
        VStack(spacing: 4) {
            CharacterAvatar(bot: bot, size: 30, still: true)
            Text(widgetName(bot))
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .padding(.horizontal, 4)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16))
        .overlay(alignment: .topTrailing) {
            if bot.isWorking || bot.status == "error" {
                Circle().fill(bot.widgetState.tint).frame(width: 7, height: 7).padding(7)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(widgetName(bot)), \(L10n.tr(bot.widgetState.label))")
    }
}

public extension Bot {
    static var widgetPreview: [Bot] {
        let json = """
        [{"id":"preview-review","name":"Reviewer","avatarShape":"blob","avatarColor":"green","backend":"claude","cwd":"/","status":"needsInput","activity":"Review changes","lastAt":3},
         {"id":"preview-build","name":"Builder","avatarShape":"squircle","avatarColor":"orange","backend":"codex","cwd":"/","status":"working","activity":"Running tests","lastAt":2},
         {"id":"preview-docs","name":"Docs","avatarShape":"hex","avatarColor":"gray","backend":"claude","cwd":"/","status":"idle","lastAt":1}]
        """
        return (try? JSONDecoder().decode([Bot].self, from: Data(json.utf8))) ?? []
    }
}
