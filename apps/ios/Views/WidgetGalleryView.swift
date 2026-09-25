import CodyncKit
import CodyncUI
import SwiftUI
import WidgetKit

/// The installed widgets and these previews share their rendering components.
struct WidgetGalleryView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var kind = "usage"
    @State private var providerID = "claude"
    @State private var size = "medium"
    @State private var hasWidget: Bool?
    @State private var widgetCheckFailed = false

    private var provider: UsageProvider? { Usage.widgetPreview.providers.first { $0.id == providerID } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                HStack(spacing: 14) {
                    CharacterAvatar(shape: "hex", color: "gray", size: 42)
                        .frame(width: 58, height: 58)
                        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 16))
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Codync, at a glance.")
                            .font(.title3.weight(.semibold)).tracking(-0.4)
                        Text("Your bots and usage, on every surface.")
                            .font(.footnote).foregroundStyle(Palette.secondary)
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))

                VStack(alignment: .leading, spacing: 9) {
                    sectionLabel("Get set up")
                    VStack(spacing: 0) {
                        setupRow("Connect a computer", complete: model.pairing != nil)
                        Divider().padding(.leading, 42)
                        setupRow("Add a Codync widget", complete: hasWidget == true,
                                 status: hasWidget == nil ? (widgetCheckFailed ? "Unable to check" : "Checking") : nil)
                    }
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 18))
                    if widgetCheckFailed {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Couldn't check your widgets. You can still add one using the steps below.")
                                .foregroundStyle(Palette.secondary)
                            Button("Check again") { checkWidgets() }
                        }
                        .font(.caption)
                    } else {
                        Text(hasWidget == nil ? "Checking your widgets…" : "Steps check themselves as you finish them.")
                            .font(.caption2).foregroundStyle(Palette.tertiary)
                    }
                }

                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        sectionLabel("Choose your widget")
                        Spacer()
                        Text("Sample data").font(.caption2).foregroundStyle(Palette.secondary)
                    }
                    if typeSize.isAccessibilitySize {
                        kindPicker.pickerStyle(.menu)
                        if kind == "usage" { providerPicker.pickerStyle(.menu) }
                    } else {
                        kindPicker.pickerStyle(.segmented)
                        if kind == "usage" {
                            providerPicker.pickerStyle(.segmented)
                        }
                    }
                    Picker("Size", selection: $size) {
                        Text("Small").tag("small")
                        Text("Medium").tag("medium")
                        Text("Large").tag("large")
                    }
                    .pickerStyle(.menu)
                    VStack(spacing: 18) {
                        preview(wide: size != "small")
                            .frame(width: size == "small" ? 158 : nil, height: size == "large" ? 338 : 158)
                        previewDescription
                    }
                    .padding(14)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 26))
                }

                VStack(alignment: .leading, spacing: 12) {
                    sectionLabel("More ways to stay up to date")
                    NavigationLink { LockWidgetGalleryView() } label: {
                        Label("Lock Screen widgets", systemImage: "lock.rectangle")
                    }
                    NavigationLink { ActivityGalleryView() } label: {
                        Label("Live Activity & Dynamic Island", systemImage: "waveform")
                    }
                }
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)

                guide(number: "01", title: "Add the widget", icon: "plus.square.on.square",
                      detail: "Touch and hold your Home Screen. Tap Edit, then Add Widget. Search for Codync and choose Bots or Provider usage.")
                guide(number: "02", title: "Make it yours", icon: "slider.horizontal.3",
                      detail: "Touch and hold Provider usage, then tap Edit Widget to choose Claude or Codex. Widgets follow the account selected in Codync.")
                guide(number: "03", title: "On your Lock Screen", icon: "lock",
                      detail: "Touch and hold your Lock Screen. Tap Customize, choose the Lock Screen, then tap the widget area. Select Codync to add Bots or Usage limits.")
                Text("Widgets show the last reported state. Open Codync for live updates and approvals.")
                    .font(.caption).foregroundStyle(Palette.secondary)
            }
            .padding(18)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .navigationTitle("Widgets")
        .navigationBarTitleDisplayMode(.inline)
        .task { checkWidgets() }
        .onChange(of: scenePhase) { _, phase in if phase == .active { checkWidgets() } }
    }

    private var kindPicker: some View {
        Picker("Widget", selection: $kind) {
            Text("Provider usage").tag("usage")
            Text("Bots").tag("bots")
        }
    }

    private var providerPicker: some View {
        Picker("Provider", selection: $providerID) {
            Text("Claude").tag("claude")
            Text("Codex").tag("codex")
        }
    }

    private var previewDescription: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("\(size.capitalized) widget").font(.subheadline.weight(.medium))
            Text(kind == "usage" ? "Small shows the tightest limit. Medium shows two windows. Large adds a summary and up to four limits." : "See who needs you and who's still working. Large shows up to six bots.")
                .font(.caption).foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private func preview(wide: Bool) -> some View {
        Group {
            if kind == "usage", let provider {
                ProviderWidgetCard(provider: provider, layout: size == "large" ? .large : wide ? .medium : .small)
            } else {
                BotsWidgetCard(bots: Bot.widgetPreview, wide: wide, large: size == "large")
            }
        }
        .padding(14)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(size.capitalized) widget preview")
    }

    private func sectionLabel(_ title: String) -> some View {
        Text(title).font(.caption.weight(.medium)).foregroundStyle(Palette.secondary)
            .accessibilityAddTraits(.isHeader)
    }

    private func setupRow(_ title: String, complete: Bool, status: String? = nil) -> some View {
        HStack(spacing: 10) {
            Image(systemName: complete ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(complete ? Palette.added : Palette.tertiary)
                .font(.system(size: 15))
            Text(title).font(.footnote).foregroundStyle(complete ? Palette.secondary : Palette.text)
            Spacer()
        }
        .padding(14)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(status ?? (complete ? "Complete" : "Not complete"))
    }

    private func guide(number: String, title: String, icon: String, detail: String) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            sectionLabel(title)
            HStack(alignment: .top, spacing: 14) {
                VStack(spacing: 8) {
                    Image(systemName: icon).font(.system(size: 24, weight: .light))
                    Text(number).font(.system(size: 10, weight: .medium, design: .monospaced))
                        .foregroundStyle(Palette.tertiary)
                }
                .frame(width: 42)
                Text(detail).font(.footnote).foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20))
        }
    }

    private func checkWidgets() {
        widgetCheckFailed = false
        WidgetCenter.shared.getCurrentConfigurations { result in
            let installed = (try? result.get())?.contains { $0.kind.hasPrefix("Codync") }
            Task { @MainActor in
                hasWidget = installed
                widgetCheckFailed = installed == nil
            }
        }
    }
}
