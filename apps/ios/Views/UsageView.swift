import CodyncKit
import CodyncUI
import SwiftUI

/// Usage limits (opened from the State tab): one card per provider, the tightest limit as a big bar, every limit listed below.
struct UsageView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var collapsed: Set<String> = []

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
                        Text(L10n.tr("No usage yet")).font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
                        Text(L10n.tr("Your computer reads plan limits from the agents your bots run and the ones you turn on."))
                            .font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 60)
                }
                if let services = model.usage.services, !services.isEmpty {
                    UsageChooser(services: services)
                }
            }
            .padding(16)
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: model.usage.providers.map(\.id))
        }
        .background(Palette.background)
        .refreshable { await model.refreshUsage() }
    }
}

/// Which agents' limits this computer lists.
private struct UsageChooser: View {
    let services: [UsageService]
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.tr("Show usage for")).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
            ForEach(services) { service in
                Toggle(isOn: binding(service)) {
                    HStack(spacing: 10) {
                        ProviderTile(provider: service.provider)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(service.name).font(.subheadline).foregroundStyle(Palette.text)
                            if service.shown, !model.usage.providers.contains(where: { $0.id == service.id }) {
                                Text(L10n.tr("No reading yet")).font(.caption).foregroundStyle(Palette.tertiary)
                            }
                        }
                    }
                }
                .toggleStyle(.codync)
            }
            Text(L10n.tr("On by default for the agents your bots run.")).font(.caption).foregroundStyle(Palette.tertiary)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
    }

    private func binding(_ service: UsageService) -> Binding<Bool> {
        Binding(get: { service.shown }, set: { shown in
            Task { await model.setUsageShown(service.id, shown: shown) }
        })
    }
}

/// An agent's mascot or logo on its tinted tile.
private struct ProviderTile: View {
    let provider: UsageProvider
    @AppStorage(SharedStore.usageIconStyleKey, store: UserDefaults(suiteName: SharedStore.appGroup))
    private var usageIconStyle = UsageIconStyle.character.rawValue

    var body: some View {
        ProviderMascot(provider, size: 22, style: UsageIconStyle(rawValue: usageIconStyle) ?? .character)
            .frame(width: 34, height: 34)
            .background(provider.tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

private struct ProviderCard: View {
    let provider: UsageProvider
    @Binding var collapsed: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                ProviderTile(provider: provider)
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
                        Text(L10n.tr(top.title))
                        Spacer()
                        Text(top.usedText).monospacedDigit()
                    }
                    .font(.subheadline)
                    .foregroundStyle(Palette.text)
                    UsageTicks(percent: top.percent, tint: provider.widgetTint, height: 15)
                    if let reset = top.resetsShort() {
                        Text(L10n.resetLine(reset)).font(.caption.monospacedDigit()).foregroundStyle(Palette.tertiary)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                    }
                }
            }

            if !collapsed {
                VStack(spacing: 8) {
                    ForEach(provider.windows) { w in
                        row(L10n.tr(w.title), w.reading)
                        if let reset = w.resetsShort() { row(L10n.tr("Resets"), L10n.resetLine(reset)) }
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
            Text(L10n.tr(label)).foregroundStyle(Palette.secondary)
            Spacer()
            Text(value).foregroundStyle(Palette.text).monospacedDigit()
        }
        .font(.footnote)
    }
}
