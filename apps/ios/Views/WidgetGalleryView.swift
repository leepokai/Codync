import CodyncKit
import CodyncUI
import SwiftUI
import WidgetKit

/// The installed widgets and these previews share their rendering components.
struct WidgetGalleryView: View {
    /// Pushed inside Computers & settings rather than opened as its own sheet.
    var pushed = false
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
    @AppStorage(SharedStore.usageIconStyleKey, store: UserDefaults(suiteName: SharedStore.appGroup))
    private var usageIconStyle = UsageIconStyle.character.rawValue

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
                        setupRow("Connect a computer", complete: !accounts.computers.isEmpty)
                        Rectangle().fill(Palette.border).frame(height: 0.5).padding(.leading, 42)
                        setupRow("Add a Codync widget", complete: hasWidget == true,
                                 status: hasWidget == nil ? (widgetCheckFailed ? "Unable to check" : "Checking") : nil)
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
                        if kind == "usage" { ChoicePicker(selection: $providerID, options: Self.providers) }
                    } else {
                        SegmentedChoice(selection: $kind, options: Self.kinds)
                        if kind == "usage" {
                            SegmentedChoice(selection: $providerID, options: Self.providers)
                        }
                    }
                    ChoicePicker(selection: $size, options: [("small", "Small"), ("medium", "Medium"), ("large", "Large")])
                    VStack(spacing: 18) {
                        preview(wide: size != "small")
                            .frame(width: size == "small" ? 158 : nil, height: size == "large" ? 338 : 158)
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
                    Text("Applies to Usage on iPhone and in the Mac menu bar.")
                        .font(.caption).foregroundStyle(Palette.secondary)
                }

                VStack(alignment: .leading, spacing: 12) {
                    sectionLabel("Keep tasks in view")
                    Button { page = .lockScreen } label: {
                        Label("Lock Screen widgets", systemImage: "lock.rectangle").foregroundStyle(Palette.accent)
                    }
                    .buttonStyle(PressScale())
                    Button { page = .activity } label: {
                        Label("Live Activity & Dynamic Island", systemImage: "waveform").foregroundStyle(Palette.accent)
                    }
                    .buttonStyle(PressScale())
                }
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)

                VStack(alignment: .leading, spacing: 12) {
                    sectionLabel("Add a widget")
                    WidgetSetupAnimation()
                    Text("Touch and hold the Home Screen · Edit · Add Widget · Codync")
                        .font(.caption).foregroundStyle(Palette.secondary)
                        .frame(maxWidth: .infinity, alignment: .center)
                }
                Text("Widgets show the latest report. Open Codync for live updates and approvals.")
                    .font(.caption2).foregroundStyle(Palette.tertiary)
            }
            .padding(18)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .page("Widgets", pushed: pushed)
        .navigationDestination(item: $page) { page in
            switch page {
            case .lockScreen: LockWidgetGalleryView()
            case .activity: ActivityGalleryView()
            }
        }
        .task { checkWidgets() }
        .onChange(of: scenePhase) { _, phase in if phase == .active { checkWidgets() } }
    }

    private enum Page: Hashable { case lockScreen, activity }

    private static let kinds: [(id: String, label: String)] = [("usage", "Provider usage"), ("bots", "Bots")]
    private static let providers: [(id: String, label: String)] = [("claude", "Claude"), ("codex", "Codex")]

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

private struct WidgetSetupAnimation: View {
    private enum Stage: CaseIterable { case home, choose, placed }
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Group {
            if reduceMotion {
                stageContent(.placed)
            } else {
                PhaseAnimator(Stage.allCases) { stage in
                    stageContent(stage)
                } animation: { _ in .easeInOut(duration: 0.55) }
            }
        }
    }

    private func stageContent(_ stage: Stage) -> some View {
        ZStack {
                RoundedRectangle(cornerRadius: 24).fill(Palette.bubbleAgent)
                switch stage {
                case .home:
                    VStack(spacing: 14) {
                        Text("9:41").font(.system(size: 32, weight: .medium, design: .rounded)).foregroundStyle(Palette.text)
                        HStack(spacing: 12) {
                            appIcon("message.fill", "Messages", .green)
                            appIcon("calendar", "Calendar", .red)
                            appIcon("photo.fill", "Photos", .blue)
                            appIcon("gearshape.fill", "Settings", .gray)
                        }
                        Label("Touch & hold", systemImage: "hand.tap.fill")
                            .font(.caption.weight(.medium)).foregroundStyle(Palette.secondary)
                    }
                    .transition(.opacity.combined(with: .scale(scale: 0.96)))
                case .choose:
                    VStack(spacing: 10) {
                        HStack(spacing: 8) {
                            Image(systemName: "magnifyingglass").foregroundStyle(Palette.tertiary)
                            Text("Search widgets").foregroundStyle(Palette.secondary)
                            Spacer()
                        }
                        .font(.caption).padding(10).background(Palette.surface, in: Capsule())
                        HStack(spacing: 12) {
                            CharacterAvatar(shape: "hex", color: "gray", size: 38)
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Codync").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
                                Text("Bots · Usage").font(.caption).foregroundStyle(Palette.secondary)
                            }
                            Spacer()
                            Image(systemName: "plus.circle.fill").font(.title2).foregroundStyle(Palette.accent)
                        }
                        .padding(14).background(Palette.surface, in: RoundedRectangle(cornerRadius: 18))
                    }
                    .padding(20).transition(.opacity.combined(with: .move(edge: .trailing)))
                case .placed:
                    HStack(spacing: 16) {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Codync").font(.caption.weight(.semibold)).foregroundStyle(Palette.text)
                            HStack(spacing: 6) {
                                CharacterAvatar(shape: "blob", color: "green", size: 24)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Reviewer").font(.system(size: 10, weight: .semibold))
                                    Text("Needs you").font(.system(size: 9)).foregroundStyle(Palette.warning)
                                }
                                Spacer(minLength: 0)
                            }
                            HStack(spacing: 6) {
                                CharacterAvatar(shape: "hex", color: "orange", size: 24)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Builder").font(.system(size: 10, weight: .semibold))
                                    Text("Running tests").font(.system(size: 9)).foregroundStyle(Palette.secondary)
                                }
                                Spacer(minLength: 0)
                            }
                        }
                        .padding(14).frame(width: 188, height: 142, alignment: .topLeading)
                        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
                        VStack(spacing: 10) {
                            Image(systemName: "checkmark.circle.fill").font(.system(size: 32)).foregroundStyle(Palette.added)
                            Text("Added").font(.caption.weight(.medium)).foregroundStyle(Palette.text)
                        }
                        .transition(.scale.combined(with: .opacity))
                    }
                    .transition(.opacity.combined(with: .scale(scale: 0.96)))
                }
        }
        .frame(height: 190)
        .padding(10)
        .accessibilityLabel("Widget setup preview, \(stage == .home ? "hold the Home Screen" : stage == .choose ? "choose Codync" : "widget added")")
    }

    private func appIcon(_ symbol: String, _ title: String, _ tint: Color) -> some View {
        VStack(spacing: 5) {
            Image(systemName: symbol).font(.system(size: 17, weight: .medium)).foregroundStyle(.white)
                .frame(width: 38, height: 38).background(tint, in: RoundedRectangle(cornerRadius: 11))
            Text(title).font(.system(size: 8)).foregroundStyle(Palette.secondary)
        }
    }
}
