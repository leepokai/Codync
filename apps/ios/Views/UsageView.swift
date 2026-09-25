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
                    ContentUnavailableView("No usage yet", systemImage: "chart.bar",
                                           description: Text("Your computer reads the limits from Claude Code and Codex."))
                        .padding(.top, 60)
                }
                Button("How to add a widget") { widgetHelp = true }
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(Palette.secondary)
                    .padding(.top, 4)
            }
            .padding(16)
        }
        .background(Palette.background)
        .navigationTitle("Usage")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await model.refreshUsage() }
        .sheet(isPresented: $widgetHelp) { WidgetHelp() }
    }
}

private struct ProviderCard: View {
    let provider: UsageProvider
    @Binding var collapsed: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                ProviderMascot(provider, size: 40)
                    .frame(width: 52, height: 52)
                    .background(provider.tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                Text(provider.name).font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
                Spacer()
                Button(collapsed ? "Expand" : "Collapse", systemImage: "chevron.up") {
                    withAnimation(.snappy) { collapsed.toggle() }
                }
                .labelStyle(.iconOnly)
                .rotationEffect(.degrees(collapsed ? 180 : 0))
                .foregroundStyle(Palette.tertiary)
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
                    TickBar(percent: top.percent, tint: provider.tint)
                    if let reset = top.resetsShort() {
                        Text(reset).font(.caption.monospacedDigit()).foregroundStyle(Palette.tertiary)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                    }
                }
            }

            if !collapsed {
                Divider()
                VStack(spacing: 12) {
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
        .padding(18)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
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
        .font(.body)
    }
}

/// Nowdex-style bar: a row of ticks, filled up to the percentage.
private struct TickBar: View {
    let percent: Double
    let tint: Color

    var body: some View {
        GeometryReader { geo in
            let count = max(1, Int(geo.size.width / 7))
            let filled = Int((Double(count) * min(100, percent) / 100).rounded())
            let color = percent >= 90 ? Palette.danger : tint
            HStack(spacing: 3) {
                ForEach(0..<count, id: \.self) { i in
                    Capsule().fill(i < filled ? color : color.opacity(0.18))
                }
            }
        }
        .frame(height: 24)
        .accessibilityElement()
        .accessibilityLabel("\(Int(percent.rounded())) percent used")
    }
}

private struct WidgetHelp: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Label("Touch and hold an empty spot on your Home Screen until the apps jiggle.", systemImage: "hand.tap")
                Label("Tap Edit, then Add Widget.", systemImage: "plus.square")
                Label("Search for Codync and pick Provider usage or Usage limits.", systemImage: "magnifyingglass")
                Label("Touch and hold the widget, then Edit Widget to choose Claude or Codex.", systemImage: "slider.horizontal.3")
            }
            .navigationTitle("Add a widget")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
                }
            }
        }
        .presentationDetents([.medium])
    }
}
