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
        }
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
