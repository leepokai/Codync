import CodyncKit
import CodyncUI
import SwiftUI
import WidgetKit

/// The State tab's Widget page. The installed widgets and these previews share their rendering components.
struct WidgetGalleryView: View {
    @Environment(AccountStore.self) private var accounts
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var page: Page?
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var kind = "usage"
    @State private var providerID = "claude"
    @State private var size = "medium"
    @State private var hasWidget: Bool?
    @State private var widgetCheckFailed = false
    /// nil until asked, false once turned off in the Settings app. Not asked during onboarding: offered here.
    @State private var notificationsAllowed: Bool?
    @AppStorage(SharedStore.usageIconStyleKey, store: UserDefaults(suiteName: SharedStore.appGroup))
    private var usageIconStyle = UsageIconStyle.character.rawValue

    /// Sample data where there is some, else what the computer last reported.
    private var provider: UsageProvider? {
        (Usage.widgetPreview.providers + SharedStore.activeContext.usage.values.flatMap(\.providers)).first { $0.id == providerID }
    }
    private var providers: [(id: String, label: String)] {
        let reported = SharedStore.activeContext.usageProviders
        var names = Dictionary(uniqueKeysWithValues: reported.map { ($0.id, $0.name) })
        // Keep both built-in providers selectable even when the shared snapshot is empty/stale.
        for provider in Usage.widgetPreview.providers where names[provider.id] == nil {
            names[provider.id] = provider.name
        }
        return names.map { (id: $0.key, label: $0.value) }.sorted { $0.label < $1.label }
    }

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
                        setupRow("Connect a computer", complete: !accounts.computers.isEmpty)
                        Rectangle().fill(Palette.border).frame(height: 0.5).padding(.leading, 42)
                        setupRow("Add a Codync widget", complete: hasWidget == true,
                                 status: hasWidget == nil ? (widgetCheckFailed ? "Unable to check" : "Checking") : nil)
                        Rectangle().fill(Palette.border).frame(height: 0.5).padding(.leading, 42)
                        Button(action: turnOnNotifications) {
                            setupRow(notificationsAllowed == false ? "Turn on notifications in Settings" : "Turn on notifications",
                                     complete: notificationsAllowed == true)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(notificationsAllowed == true)
                        .accessibilityHint("Know when a bot is done or needs you.")
                    }
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 18))
                    if widgetCheckFailed {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Couldn't check your widgets. You can still add one using the steps below.")
                                .foregroundStyle(Palette.secondary)
                            Button("Check again") { checkWidgets() }
                                .buttonStyle(.secondary)
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
                        ChoicePicker(selection: $kind, options: Self.kinds)
                    } else {
                        SegmentedChoice(selection: $kind, options: Self.kinds)
                    }
                    if kind == "usage" {
                        if typeSize.isAccessibilitySize || providers.count > 3 {
                            ChoicePicker(selection: $providerID, options: providers)
                        } else {
                            SegmentedChoice(selection: $providerID, options: providers)
                        }
                    }
                    if kind != "team" {
                        ChoicePicker(selection: $size, options: [("small", "Small"), ("medium", "Medium"), ("large", "Large")])
                    }
                    VStack(spacing: 18) {
                        preview(wide: size != "small")
                            .frame(width: size == "small" || kind == "team" ? 158 : nil, height: size == "large" && kind != "team" ? 338 : 158)
                        previewDescription
                    }
                    .padding(14)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 26))
                }

                VStack(alignment: .leading, spacing: 12) {
                    sectionLabel("Choose the look")
                    SegmentedChoice(selection: Binding(
                        get: { usageIconStyle },
                        set: { usageIconStyle = $0; WidgetCenter.shared.reloadTimelines(ofKind: "CodyncProviderUsage") }
                    ), options: [(UsageIconStyle.character.rawValue, "Character"), (UsageIconStyle.original.rawValue, "Original icon")])
                    Text("Applies to Usage in the app and in widgets.")
                        .font(.caption).foregroundStyle(Palette.secondary)
                }

                VStack(alignment: .leading, spacing: 12) {
                    sectionLabel("Keep tasks in view")
                    Button { page = .lockScreen } label: {
                        Label("Lock Screen widgets", systemImage: "lock.rectangle").foregroundStyle(Palette.accent)
                    }
                    .buttonStyle(PressScale())
                }
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)

                VStack(alignment: .leading, spacing: 12) {
                    sectionLabel("Add a widget")
                    WidgetSetupDemo()
                }
                Text("Widgets show the latest report. Open Codync for live updates and approvals.")
                    .font(.caption2).foregroundStyle(Palette.tertiary)
            }
            .padding(18)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .hidesSystemNavigationBar()
        .navigationDestination(item: $page) { page in
            switch page {
            case .lockScreen: LockWidgetGalleryView()
            }
        }
        .task {
            checkWidgets()
            await checkNotifications()
        }
        .onChange(of: scenePhase) { _, phase in
            guard phase == .active else { return }
            checkWidgets()
            Task { await checkNotifications() }
        }
    }

    private enum Page: Hashable { case lockScreen }

    private static let kinds: [(id: String, label: String)] = [("usage", "Provider usage"), ("bots", "Bots"), ("team", "Team")]

    private var previewDescription: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(kind == "team" ? "Small widget" : "\(size.capitalized) widget").font(.subheadline.weight(.medium))
            Text(kind == "usage" ? "Small shows the tightest limit. Medium shows two windows. Large adds a summary and up to four limits."
                 : kind == "team" ? "Four bots as characters with a dot for each one at work. Touch and hold the widget, then Edit Widget to choose who sits in each spot; empty spots show the bots that need you first."
                 : "See who needs you and who's still working. Large shows up to six bots.")
                .font(.caption).foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private func preview(wide: Bool) -> some View {
        Group {
            if kind == "usage", let provider {
                ProviderWidgetCard(provider: provider, layout: size == "large" ? .large : wide ? .medium : .small)
            } else if kind == "team" {
                BotsTeamCard(bots: Bot.widgetPreview)
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

    private func checkNotifications() async {
        let allowed = await PushRegistrar.shared.isAllowed()
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { notificationsAllowed = allowed }
    }

    /// Asks once; after a no, only the Settings app can turn them on.
    private func turnOnNotifications() {
        if notificationsAllowed == false {
            UIApplication.shared.open(URL(string: UIApplication.openNotificationSettingsURLString)!)
        } else {
            Task {
                _ = await PushRegistrar.shared.requestAuthorization()
                await checkNotifications()
            }
        }
    }

    private func checkWidgets() {
        let animation = Motion.reduced(Motion.layout, reduceMotion)
        withAnimation(animation) { widgetCheckFailed = false }
        WidgetCenter.shared.getCurrentConfigurations { result in
            let installed = (try? result.get())?.contains { $0.kind.hasPrefix("Codync") }
            Task { @MainActor in
                withAnimation(animation) {
                    hasWidget = installed
                    widgetCheckFailed = installed == nil
                }
            }
        }
    }
}
