import CodyncKit
import CodyncUI
import SwiftUI

/// One computer's usage (opened from the State tab): its name, what its agents have used, and behind a button
/// which agents show and in what order. A `List` for the system's reordering, in edit mode only while arranging,
/// which is what limits a drag to the handle.
struct UsageView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var arranging = false
    /// The order just dropped: on screen until the host answers, so the row doesn't jump back.
    @State private var sentOrder: [String]?

    var body: some View {
        let usage = model.usage
        let services = usage.services(in: sentOrder)
        let editing = arranging && !services.isEmpty
        List {
            Section {
                if editing {
                    ForEach(services) { service in
                        ChooserRow(service: service, hasNoReading: usage.hasNoReading(service))
                    }
                    .onMove { source, destination in
                        UsageOrder.moved(services.map(\.id), from: source, to: destination).map(send)
                    }
                } else if usage.providers.isEmpty {
                    Text(L10n.tr("No usage information yet."))
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .listRowBackground(Color.clear)
                } else {
                    // One row, not one each: a row that was on screen before edit mode keeps its place in the
                    // list under the same id, and the system then gives it no handle.
                    VStack(alignment: .leading, spacing: 28) {
                        ForEach(usage.providers) { ProviderLimits(provider: $0) }
                    }
                    .listRowInsets(EdgeInsets(top: 14, leading: 16, bottom: 14, trailing: 16))
                }
            } header: {
                heading(canArrange: !services.isEmpty, editing: editing)
            } footer: {
                if editing {
                    Text(L10n.tr("On by default for the agents your bots run."))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .listRowBackground(Palette.surface)
            .listRowSeparator(.hidden)
            .environment(\.layoutDirection, .leftToRight)
        }
        .listStyle(.insetGrouped)
        // The system keeps its reorder handle at the row's trailing edge: this makes that the left.
        .environment(\.layoutDirection, .rightToLeft)
        .scrollContentBackground(.hidden)
        .scrollIndicators(.hidden)
        .environment(\.editMode, .constant(editing ? .active : .inactive))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: usage.providers.map(\.id))
        .background(Palette.background)
        .refreshable { await model.refreshUsage() }
    }

    private func heading(canArrange: Bool, editing: Bool) -> some View {
        HStack(spacing: 6) {
            ComputerBadge(model.computer, size: 20)
            Text(model.hostName)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
            Spacer(minLength: 0)
            if canArrange {
                IconButton(editing ? "Done" : "Choose and arrange", systemImage: editing ? "checkmark" : "slider.horizontal.3") {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { arranging = !editing }
                }
                // Its glyph over the switches' edge.
                .padding(.trailing, -8)
            }
        }
        .textCase(nil)
    }

    /// Holds `ids` on screen until the answer lands, or until a refusal puts the host's order back.
    private func send(_ ids: [String]) {
        sentOrder = ids
        Task {
            await model.setUsageOrder(ids)
            // A later drop keeps its own order on screen until its own answer.
            if sentOrder == ids { sentOrder = nil }
        }
    }
}

/// An agent's name, its switch, and the system's handle beside it.
private struct ChooserRow: View {
    let service: UsageService
    let hasNoReading: Bool
    @Environment(BotStore.self) private var model

    var body: some View {
        Toggle(isOn: Binding(get: { service.shown }, set: { shown in
            Task { await model.setUsageShown(service.id, shown: shown) }
        })) {
            HStack(spacing: 8) {
                Mascot(provider: service.provider)
                Text(service.name).font(.subheadline).foregroundStyle(Palette.text).lineLimit(1)
                if hasNoReading {
                    Text(L10n.tr("No reading yet")).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(1)
                }
            }
        }
        .toggleStyle(.codync)
    }
}

/// An agent's mascot or logo, whichever the widgets' setting chose.
private struct Mascot: View {
    let provider: UsageProvider
    @AppStorage(SharedStore.usageIconStyleKey, store: UserDefaults(suiteName: SharedStore.appGroup))
    private var usageIconStyle = UsageIconStyle.character.rawValue

    var body: some View {
        ProviderMascot(provider, size: 20, style: UsageIconStyle(rawValue: usageIconStyle) ?? .character)
    }
}

private struct ProviderLimits: View {
    let provider: UsageProvider

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Mascot(provider: provider)
                Text(provider.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
            }
            ForEach(provider.windows) { WindowLimit(window: $0, tint: provider.widgetTint) }
        }
    }
}

/// A limit's name and when it resets, how much is used, and under them the bar; or the note an agent gives instead.
private struct WindowLimit: View {
    let window: UsageWindow
    let tint: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text(L10n.tr(window.title)).foregroundStyle(Palette.text).lineLimit(1).layoutPriority(window.text == nil ? 0 : 1)
                if let note = window.text {
                    Text(note).foregroundStyle(Palette.secondary).lineLimit(1).frame(maxWidth: .infinity, alignment: .trailing)
                } else {
                    Text(window.resetsShort().map(L10n.resetLine) ?? "")
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text("\(Int(window.percent.rounded()))%")
                        .fontWeight(.medium)
                        .monospacedDigit()
                        .foregroundStyle(Palette.text)
                        .layoutPriority(1)
                        .accessibilityLabel(window.usedText)
                }
            }
            .font(.subheadline)
            if window.text == nil {
                UsageTicks(percent: window.percent, tint: tint, height: 5)
            }
        }
    }
}
