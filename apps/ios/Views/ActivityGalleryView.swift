import ActivityKit
import CodyncKit
import CodyncUI
import SwiftUI

struct ActivityGalleryView: View {
    /// The presentations shown: the State tab splits Lock Screen from the Dynamic Island.
    var forms = BotActivityPreview.Form.allCases
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.openURL) private var openURL
    @AppStorage("liveActivitiesEnabled") private var enabled = true
    @State private var allowed = ActivityAuthorizationInfo().areActivitiesEnabled
    @State private var form: BotActivityPreview.Form?
    @State private var phase = BotActivityPresentation.Phase.working
    @State private var startedAt = Date.now - 154

    private var island: Bool { !forms.contains(.lockScreen) }

    private var state: BotActivityPresentation {
        let status: String = switch phase {
        case .sending: "sending"
        case .queued: "queued"
        case .working, .stale: "working"
        case .needsInput: "needsInput"
        case .completed: "idle"
        case .failed: "error"
        case .waiting: "unknown"
        }
        return .init(status: status, activity: phase == .needsInput ? "Review the proposed changes." : "Running the test suite.",
                     isStale: phase == .stale)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 10) {
                    // Recorded from the switch itself: both galleries share the setting and stay alive.
                    Toggle(isOn: Binding(get: { enabled }, set: { on in
                        enabled = on
                        Analytics.shared.capture(.settingChanged(.liveActivities, on: on))
                    })) {
                        Text(L10n.tr(island ? "Show in Dynamic Island" : "Show Live Activities"))
                    }
                        .toggleStyle(.codync)
                        .font(.subheadline.weight(.medium))
                    Text(L10n.tr(island
                         ? "The Dynamic Island shows the same Live Activity as the Lock Screen, so this switch turns off both."
                         : "Starts when you send a task from this iPhone. Follow its progress, then open the conversation when your bot needs you."))
                        .font(.footnote).foregroundStyle(Palette.secondary)
                    if !allowed {
                        Text(L10n.tr("Live Activities are turned off in iOS Settings."))
                            .font(.footnote).foregroundStyle(Palette.secondary)
                        Button(L10n.tr("Open iOS Settings")) { openURL(URL(string: UIApplication.openSettingsURLString)!) }
                            .buttonStyle(.plain)
                            .font(.footnote.weight(.medium))
                            .foregroundStyle(Palette.accent)
                    }
                }
                .padding(16).background(Palette.surface, in: RoundedRectangle(cornerRadius: 20))

                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        Text(L10n.tr("Preview")).font(.subheadline.weight(.medium))
                        Spacer()
                        Text(L10n.tr("Sample task")).font(.caption).foregroundStyle(Palette.secondary)
                    }
                    if forms.count > 1 {
                        ChoicePicker(selection: Binding(get: { form ?? forms[0] }, set: { form = $0 }), options: forms.map { ($0, $0.rawValue) })
                    }
                    ChoicePicker(selection: $phase, options: [
                        (.sending, "Sending"), (.queued, "Waiting for computer"),
                        (.working, "Working"), (.needsInput, "Needs you"), (.completed, "Done"),
                        (.failed, "Error"), (.stale, "Update delayed"),
                    ])
                    if let bot = Bot.widgetPreview.first {
                        BotActivityPreview(bot: bot, state: state, form: form ?? forms[0], startedAt: startedAt)
                            .frame(maxWidth: .infinity, minHeight: 90)
                            .padding(12)
                            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 28))
                    }
                }
                VStack(alignment: .leading, spacing: 14) {
                    ForEach(forms, id: \.self) { explanation($0) }
                }
                Text(L10n.tr("iOS chooses the presentation based on your device and other active tasks. These previews don't start a Live Activity. Approvals are handled inside Codync."))
                    .font(.footnote).foregroundStyle(Palette.secondary)
            }
            .padding(18).frame(maxWidth: 560).frame(maxWidth: .infinity)
        }
        .background(Palette.background)

        .tint(Palette.accent)
        .onChange(of: enabled) { _, value in if !value { LiveActivities.shared.endAll() } }
        .onChange(of: scenePhase) { _, value in
            if value == .active { allowed = ActivityAuthorizationInfo().areActivitiesEnabled }
        }
    }

    private func explanation(_ form: BotActivityPreview.Form) -> some View {
        let detail = switch form {
        case .lockScreen: "The bot in its mood, its current step in the state's color, the running time, a thinking orb and its progress. When the bot needs you, Review opens the request."
        case .compact: "The bot and a thinking orb beside the camera, outlined in the state's color. The orb becomes a check or warning when the task ends."
        case .minimal: "Just the orb when iOS displays multiple Live Activities."
        case .expanded: "Touch and hold the Dynamic Island for the current step and the running time, or Review when the bot needs you. Tap to open the conversation."
        }
        return VStack(alignment: .leading, spacing: 4) {
            Text(L10n.tr(form.rawValue)).font(.subheadline.weight(.medium)).accessibilityAddTraits(.isHeader)
            Text(L10n.tr(detail)).font(.footnote).foregroundStyle(Palette.secondary)
        }
    }
}

struct LockWidgetGalleryView: View {
    @State private var kind = "bots"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                ChoicePicker(selection: $kind, options: [("bots", "Bots"), ("usage", "Usage limits")])
                Text(L10n.tr("Sample data")).font(.caption).foregroundStyle(Palette.secondary)
                ForEach(AccessoryWidgetCard.Family.allCases, id: \.self) { family in
                    VStack(alignment: .leading, spacing: 10) {
                        Text(family.rawValue).font(.subheadline.weight(.medium))
                        AccessoryWidgetCard(kind: kind == "bots" ? .bots : .usage, family: family,
                                            bots: Bot.widgetPreview, usage: .widgetPreview)
                            .frame(width: family == .circular ? 64 : family == .rectangular ? 160 : nil,
                                   height: family == .inline ? 24 : 64)
                            .padding(16).foregroundStyle(.white)
                            .background(.black, in: RoundedRectangle(cornerRadius: 20))
                            .environment(\.colorScheme, .dark)
                    }
                }
                Text(L10n.tr("Touch and hold your Lock Screen → Customize → Lock Screen. Tap the widget area and choose Codync. Inline widgets go in the date row above the clock."))
                    .font(.footnote).foregroundStyle(Palette.secondary)
                Text(L10n.tr("Choose Bots for task status or Usage limits for the highest reported limit. iOS applies your Lock Screen's color and style."))
                    .font(.footnote).foregroundStyle(Palette.secondary)
            }
            .padding(18).frame(maxWidth: 560).frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .page("Lock Screen widgets", pushed: true)
    }
}
