import AppKit
import CodyncKit
import SwiftUI

// Exports views directly with ImageRenderer. These are layout review artifacts,
// not screenshots of SpringBoard or a substitute for WidgetKit integration tests.
@main
struct WidgetDesignExport {
    @MainActor static func main() throws {
        let output = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
        for (name, scheme) in [("light", ColorScheme.light), ("dark", ColorScheme.dark)] {
            let renderer = ImageRenderer(content: WidgetBoard()
                .environment(\.colorScheme, scheme))
            renderer.scale = 2
            guard let image = renderer.cgImage else { fatalError("SwiftUI could not render \(name)") }
            let bitmap = NSBitmapImageRep(cgImage: image)
            guard let png = bitmap.representation(using: .png, properties: [:]) else {
                fatalError("Could not encode \(name)")
            }
            let url = output.appendingPathComponent("widgets-\(name).png")
            try png.write(to: url)
            print(url.path)
            try export(IconBoard().environment(\.colorScheme, scheme), to: output.appendingPathComponent("halftone-icons-\(name).png"))
            try export(ActivityBoard().environment(\.colorScheme, scheme), to: output.appendingPathComponent("activities-\(name).png"))
            try export(LargeWidgetBoard().environment(\.colorScheme, scheme), to: output.appendingPathComponent("large-widgets-\(name).png"))
        }
    }

    @MainActor private static func export<V: View>(_ content: V, to url: URL) throws {
        let renderer = ImageRenderer(content: content)
        renderer.scale = 2
        guard let image = renderer.cgImage,
              let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
            fatalError("Could not render \(url.lastPathComponent)")
        }
        try png.write(to: url)
        print(url.path)
    }
}

private struct ActivityBoard: View {
    private let statuses = ["working", "needsInput", "idle", "error", "stale"]
    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text("Codync · Live Activity & Dynamic Island").font(.title2.weight(.semibold))
            Text("Lock Screen / compact + minimal / expanded · illustrative sample task")
                .font(.caption).foregroundStyle(Palette.secondary)
            if let bot = Bot.widgetPreview.first {
                ForEach(statuses, id: \.self) { status in
                    let state = BotActivityPresentation(status: status == "stale" ? "working" : status,
                        activity: status == "needsInput" ? "Review the proposed changes." : "Running the test suite.",
                        startedAt: nil, isStale: status == "stale")
                    VStack(alignment: .leading, spacing: 8) {
                        Text(state.title).font(.headline)
                        HStack(spacing: 18) {
                            BotActivityPreview(bot: bot, state: state, form: .lockScreen).frame(width: 300)
                            VStack(spacing: 18) {
                                BotActivityPreview(bot: bot, state: state, form: .compact)
                                BotActivityPreview(bot: bot, state: state, form: .minimal)
                            }
                            BotActivityPreview(bot: bot, state: state, form: .expanded).frame(width: 300)
                        }
                    }
                }
            }
        }
        .padding(24).foregroundStyle(Palette.text).background(Palette.background)
    }
}

private struct LargeWidgetBoard: View {
    private var bots: [Bot] {
        (0..<6).map { i in
            var bot = Bot.widgetPreview[i % 3]
            bot.id = "preview-\(i)"
            bot.name += " \(i + 1)"
            return bot
        }
    }
    private var provider: UsageProvider {
        var p = Usage.widgetPreview.providers[0]
        var extra = p.windows[1]
        extra.id = "model-limit"; extra.label = "Weekly · Opus"; extra.percent = 76
        p.windows.append(extra)
        extra.id = "model-limit-2"; extra.label = "Weekly · Sonnet"; extra.percent = 42
        p.windows.append(extra)
        return p
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Large & Lock Screen widgets · sample data").font(.title2.weight(.semibold))
            HStack(spacing: 18) {
                ProviderWidgetCard(provider: provider, layout: .large)
                    .padding(16).frame(width: 338, height: 338)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 24))
                BotsWidgetCard(bots: bots, wide: true, large: true)
                    .padding(16).frame(width: 338, height: 338)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 24))
            }
            ForEach(AccessoryWidgetCard.Family.allCases, id: \.self) { family in
                HStack(spacing: 32) {
                    Text(family.rawValue).font(.caption).frame(width: 90, alignment: .leading)
                    AccessoryWidgetCard(kind: .bots, family: family, bots: bots)
                        .frame(width: family == .circular ? 64 : 210, height: 64)
                    AccessoryWidgetCard(kind: .usage, family: family, usage: .widgetPreview)
                        .frame(width: family == .circular ? 64 : 210, height: 64)
                }
            }
        }
        .padding(24).foregroundStyle(Palette.text).background(Palette.background)
    }
}

private struct WidgetBoard: View {
    private let date = Date(timeIntervalSince1970: 1_790_400_000)

    private func provider(_ index: Int, high: Bool = false) -> UsageProvider {
        var p = Usage.widgetPreview.providers[index]
        for i in p.windows.indices {
            p.windows[i].resetsAt = Int64((date.timeIntervalSince1970 + Double((i + 1) * 12_000)) * 1_000)
            if high { p.windows[i].percent = i == 0 ? 96 : 100 }
        }
        return p
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Codync · Widget layout review").font(.title2.weight(.semibold))
            Text("158 pt small · 338 × 158 pt medium · sample data")
                .font(.caption).foregroundStyle(Palette.secondary)
            providerRow("Claude", provider(0))
            providerRow("Codex", provider(1))
            providerRow("Near the limit", provider(0, high: true))
            HStack(spacing: 20) {
                card { BotsWidgetCard(bots: Bot.widgetPreview, wide: false) }.frame(width: 158)
                card { BotsWidgetCard(bots: Bot.widgetPreview, wide: true) }.frame(width: 338)
            }
            HStack(spacing: 20) {
                card { BotsWidgetCard(bots: [], wide: false) }.frame(width: 158)
                card { BotsWidgetCard(bots: [], wide: true) }.frame(width: 338)
            }
            Text("Shared SwiftUI cards. Excludes system widget chrome and refresh scheduling.")
                .font(.caption2).foregroundStyle(Palette.secondary)
        }
        .padding(24)
        .foregroundStyle(Palette.text)
        .background(Palette.background)
    }

    private func providerRow(_ title: String, _ provider: UsageProvider) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.caption.weight(.medium)).foregroundStyle(Palette.secondary)
            HStack(spacing: 20) {
                card { ProviderWidgetCard(provider: provider, layout: .small, date: date) }.frame(width: 158)
                card { ProviderWidgetCard(provider: provider, layout: .medium, date: date) }.frame(width: 338)
            }
        }
    }

    private func card<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        content().padding(14).frame(height: 158)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
    }
}

private struct IconBoard: View {
    private let sizes: [CGFloat] = [15, 20, 22, 26, 32, 64]
    private let shapes = ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"]
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Codync · Halftone icons").font(.title2.weight(.semibold))
            Text("15 / 20 / 22 / 26 / 32 / 64 pt · static reference frames")
                .font(.caption).foregroundStyle(Palette.secondary)
            ForEach(shapes, id: \.self) { shape in
                HStack(spacing: 24) {
                    Text(shape).font(.caption).frame(width: 90, alignment: .leading)
                    ForEach(sizes, id: \.self) { size in
                        CharacterAvatar(shape: shape, color: "gray", size: size).frame(width: 64, height: 64)
                    }
                }
            }
            Divider()
            ForEach(ThinkingOrb.State.allCases, id: \.self) { state in
                HStack(spacing: 24) {
                    Text(state.rawValue).font(.caption).frame(width: 90, alignment: .leading)
                    ForEach(sizes, id: \.self) { size in
                        ThinkingOrb(state: state, size: size, animated: false).frame(width: 64, height: 64)
                    }
                }
            }
            Text("Thinking Orbs · Jakub Antalik · MIT").font(.caption2).foregroundStyle(Palette.secondary)
        }
        .padding(24).foregroundStyle(Palette.text).background(Palette.background)
    }
}
