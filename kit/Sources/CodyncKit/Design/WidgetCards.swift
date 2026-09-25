import SwiftUI

/// Shared by the widget extension and its in-app gallery so previews use the
/// same typography, spacing and progress rendering as the installed widget.
public struct ProviderWidgetCard: View {
    public enum Layout { case small, medium }
    let provider: UsageProvider
    let layout: Layout
    let date: Date

    public init(provider: UsageProvider, layout: Layout, date: Date = .now) {
        self.provider = provider
        self.layout = layout
        self.date = date
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: layout == .small ? 6 : 8) {
            HStack(spacing: 7) {
                CharacterAvatar(shape: provider.mascotShape, tint: provider.widgetTint, size: 22)
                Text(provider.name)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                Spacer(minLength: 0)
                if layout == .medium {
                    Text("Usage")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(Palette.secondary)
                }
            }
            if layout == .small, let window = provider.tightest {
                Spacer(minLength: 0)
                VStack(alignment: .leading, spacing: 2) {
                    Text(window.title).font(.system(size: 11)).foregroundStyle(Palette.secondary).lineLimit(1)
                    HStack(alignment: .firstTextBaseline, spacing: 1) {
                        Text("\(Int(window.percent.rounded()))")
                            .font(.system(size: 32, weight: .semibold, design: .rounded))
                        Text("%").font(.system(size: 17, weight: .medium))
                    }
                    .monospacedDigit()
                    .foregroundStyle(window.percent >= 90 ? Palette.danger : Palette.text)
                    .minimumScaleFactor(0.7)
                }
                UsageTicks(percent: window.percent, tint: provider.widgetTint, height: 13)
                Text(window.resetsShort(now: date) ?? "Usage reported by your computer")
                    .font(.system(size: 9)).foregroundStyle(Palette.secondary)
                    .lineLimit(1)
            } else {
                ForEach(provider.windows.prefix(2)) { window in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(alignment: .firstTextBaseline) {
                            Text(window.title)
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(Palette.secondary)
                                .lineLimit(1)
                            Spacer(minLength: 4)
                            Text("\(Int(window.percent.rounded()))%")
                                .font(.system(size: 13, weight: .semibold))
                                .monospacedDigit()
                                .foregroundStyle(window.percent >= 90 ? Palette.danger : Palette.text)
                        }
                        UsageTicks(percent: window.percent, tint: provider.widgetTint, height: 12)
                        if let reset = window.resetsShort(now: date) {
                            Text(reset).font(.system(size: 9)).foregroundStyle(Palette.secondary)
                                .lineLimit(1).frame(maxWidth: .infinity, alignment: .trailing)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(provider.name) usage")
        .accessibilityValue(accessibleUsage)
    }

    private var accessibleUsage: String {
        let windows = layout == .small ? provider.tightest.map { [$0] } ?? [] : Array(provider.windows.prefix(2))
        return windows.map { window in
            "\(window.title), \(Int(window.percent.rounded())) percent" +
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
    let links: [String: URL]

    public init(bots: [Bot], wide: Bool, links: [String: URL] = [:]) {
        self.bots = bots.filter { !$0.hidden }
        self.wide = wide
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
    private var attention: Color { Color(light: 0x936000, dark: 0xECAF52) }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Bots").font(.system(size: 13, weight: .semibold))
                Spacer()
                Image(systemName: "bubble.left.and.bubble.right")
                    .font(.system(size: 12)).foregroundStyle(Palette.secondary)
            }
            if wide {
                HStack(alignment: .center, spacing: 16) {
                    summary.frame(width: 76, alignment: .leading)
                    Rectangle().fill(Palette.border).frame(width: 1)
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(ordered.prefix(3)) { bot in
                            if let link = links[bot.id] {
                                Link(destination: link) { row(bot) }
                            } else { row(bot) }
                        }
                        if bots.isEmpty {
                            Text("Create your first bot in Codync.")
                                .font(.system(size: 12)).foregroundStyle(Palette.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                Spacer(minLength: 0)
                summary
                HStack(spacing: -4) {
                    ForEach(ordered.prefix(3)) { bot in
                        CharacterAvatar(bot: bot, size: 23, animated: false)
                    }
                    Spacer(minLength: 0)
                    if working > 0 && needs > 0 {
                        Text("\(working) working").font(.system(size: 9)).foregroundStyle(Palette.secondary)
                    }
                }
            }
        }
        .foregroundStyle(Palette.text)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text("\(count)").font(.system(size: 36, weight: .semibold, design: .rounded))
                .monospacedDigit().foregroundStyle(needs > 0 ? attention : Palette.text)
            Text(label).font(.system(size: 12, weight: .medium)).foregroundStyle(Palette.secondary)
        }
        .accessibilityElement(children: .combine)
    }

    private func row(_ bot: Bot) -> some View {
        HStack(spacing: 7) {
            CharacterAvatar(bot: bot, size: 24, animated: false)
            VStack(alignment: .leading, spacing: 1) {
                Text(bot.name).font(.system(size: 11, weight: .semibold)).foregroundStyle(Palette.text)
                Text(bot.needsInput ? "Needs you" : bot.isWorking ? (bot.activity.isEmpty ? "Working…" : bot.activity) : "Ready")
                    .font(.system(size: 10)).foregroundStyle(bot.needsInput ? attention : Palette.secondary)
            }
            .lineLimit(1)
        }
        .accessibilityElement(children: .combine)
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
