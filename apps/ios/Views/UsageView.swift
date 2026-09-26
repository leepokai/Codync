import CodyncKit
import CodyncUI
import SwiftUI

/// The Usage tab: one card per provider, the tightest limit as a big bar, every limit listed below.
struct UsageView: View {
    @Environment(BotStore.self) private var model
    @State private var collapsed: Set<String> = []
    @State private var widgetHelp = false

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                ForEach(model.usage.providers) { provider in
                    ProviderCard(provider: provider, collapsed: Binding(
                        get: { collapsed.contains(provider.id) },
                        set: { if $0 { collapsed.insert(provider.id) } else { collapsed.remove(provider.id) } }
                    ))
                }
                if model.usage.providers.isEmpty {
                    VStack(spacing: 8) {
                        Image(systemName: "chart.bar").font(.system(size: 40)).foregroundStyle(Palette.tertiary)
                        Text("No usage yet").font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
                        Text("Your computer reads the limits from Claude Code and Codex.")
                            .font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 60)
                }
                Button("Widgets & setup") { widgetHelp = true }
                    .buttonStyle(.plain)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(Palette.secondary)
                    .padding(.top, 4)
            }
            .padding(16)
        }
        .background(Palette.background)
        .refreshable { await model.refreshUsage() }
        .codyncSheet(isPresented: $widgetHelp) {
            // Holds the pushes inside the sheet (Lock Screen widgets, Live Activity); no bar shows.
            NavigationStack { WidgetGalleryView() }
                .environment(model)
        }
    }
}

private struct ProviderCard: View {
    let provider: UsageProvider
    @Binding var collapsed: Bool
    @AppStorage(SharedStore.usageIconStyleKey, store: UserDefaults(suiteName: SharedStore.appGroup))
    private var usageIconStyle = UsageIconStyle.character.rawValue
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                ProviderMascot(provider, size: 22, style: UsageIconStyle(rawValue: usageIconStyle) ?? .character)
                    .frame(width: 34, height: 34)
                    .background(provider.tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                Text(provider.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
                Spacer()
                IconButton(collapsed ? "Expand" : "Collapse", systemImage: "chevron.up") {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { collapsed.toggle() }
                }
                .rotationEffect(.degrees(collapsed ? 180 : 0))
            }

            if let top = provider.tightest {
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(top.label)
                        Spacer()
                        Text("\(Int(top.percent.rounded()))%").monospacedDigit()
                    }
                    .font(.subheadline)
                    .foregroundStyle(Palette.text)
                    UsageTicks(percent: top.percent, tint: provider.widgetTint, height: 15)
                    if let reset = top.resetsShort() {
                        Text(reset).font(.caption.monospacedDigit()).foregroundStyle(Palette.tertiary)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                    }
                }
            }

            if !collapsed {
                VStack(spacing: 8) {
                    ForEach(provider.windows) { w in
                        row(w.label, "\(Int(w.percent.rounded()))%")
                        if let reset = w.resetsShort() { row("\(w.label) reset", reset.replacingOccurrences(of: "resets ", with: "")) }
                    }
                    row("Source", source)
                    row("Updated", Date(milliseconds: provider.updatedAt).formatted(date: .abbreviated, time: .shortened))
                }
                .transition(.opacity)
            }
        }
        .padding(14)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
    }

    private var source: String {
        switch provider.source {
        case "claude": "Claude Code"
        case "statusline": "Claude Code status line"
        case "agent": "A running bot"
        case "sessions": "Codex sessions"
        default: provider.source
        }
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).foregroundStyle(Palette.secondary)
            Spacer()
            Text(value).foregroundStyle(Palette.text).monospacedDigit()
        }
        .font(.footnote)
    }
}
