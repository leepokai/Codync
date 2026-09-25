import ActivityKit
import CodyncKit
import SwiftUI

struct ActivityGalleryView: View {
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage("liveActivitiesEnabled") private var enabled = true
    @State private var allowed = ActivityAuthorizationInfo().areActivitiesEnabled
    @State private var form = BotActivityPreview.Form.lockScreen
    @State private var phase = BotActivityPresentation.Phase.working
    @State private var start = Date.now - 154

    private var state: BotActivityPresentation {
        let status: String = switch phase {
        case .working, .stale: "working"
        case .needsInput: "needsInput"
        case .completed: "idle"
        case .failed: "error"
        case .waiting: "unknown"
        }
        return .init(status: status, activity: phase == .needsInput ? "Review the proposed changes." : "Running the test suite.",
                     startedAt: start, isStale: phase == .stale)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 10) {
                    Toggle("Show Live Activities", isOn: $enabled)
                        .font(.subheadline.weight(.medium))
                    Text("Starts when you send a task from this iPhone. Follow its progress, then open the conversation when your bot needs you.")
                        .font(.footnote).foregroundStyle(Palette.secondary)
                    if !allowed {
                        Text("Live Activities are turned off in iOS Settings.")
                            .font(.footnote).foregroundStyle(Palette.secondary)
                        Link("Open iOS Settings", destination: URL(string: UIApplication.openSettingsURLString)!)
                            .font(.footnote.weight(.medium))
                    }
                }
                .padding(16).background(Palette.surface, in: RoundedRectangle(cornerRadius: 20))

                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        Text("Preview").font(.subheadline.weight(.medium))
                        Spacer()
                        Text("Sample task").font(.caption).foregroundStyle(Palette.secondary)
                    }
                    Picker("Presentation", selection: $form) {
                        ForEach(BotActivityPreview.Form.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                    }.pickerStyle(.menu)
                    Picker("Task state", selection: $phase) {
                        Text("Working").tag(BotActivityPresentation.Phase.working)
                        Text("Needs you").tag(BotActivityPresentation.Phase.needsInput)
                        Text("Done").tag(BotActivityPresentation.Phase.completed)
                        Text("Error").tag(BotActivityPresentation.Phase.failed)
                        Text("Update delayed").tag(BotActivityPresentation.Phase.stale)
                    }.pickerStyle(.menu)
                    if let bot = Bot.widgetPreview.first {
                        BotActivityPreview(bot: bot, state: state, form: form)
                            .frame(maxWidth: .infinity, minHeight: 90)
                            .padding(12)
                            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 28))
                    }
                }
                VStack(alignment: .leading, spacing: 14) {
                    explanation("Lock Screen", "A task card with the bot, current step and elapsed time. Tap it to return to the conversation.")
                    explanation("Compact", "The bot and timer appear beside the camera. The timer becomes a status icon when the task needs attention or ends.")
                    explanation("Minimal", "A small status icon when iOS displays multiple Live Activities.")
                    explanation("Expanded", "Touch and hold the Dynamic Island for the current step and a shortcut to the conversation.")
                }
                Text("iOS chooses the presentation based on your device and other active tasks. These previews don't start a Live Activity. Approvals are handled inside Codync.")
                    .font(.footnote).foregroundStyle(Palette.secondary)
            }
            .padding(18).frame(maxWidth: 560).frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .navigationTitle("Live Activity")
        .navigationBarTitleDisplayMode(.inline)
        .tint(Palette.accent)
        .onChange(of: enabled) { _, value in if !value { LiveActivities.shared.endAll() } }
        .onChange(of: scenePhase) { _, value in
            if value == .active { allowed = ActivityAuthorizationInfo().areActivitiesEnabled }
        }
    }

    private func explanation(_ title: String, _ detail: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.subheadline.weight(.medium)).accessibilityAddTraits(.isHeader)
            Text(detail).font(.footnote).foregroundStyle(Palette.secondary)
        }
    }
}

struct LockWidgetGalleryView: View {
    @State private var kind = "bots"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                Picker("Widget", selection: $kind) {
                    Text("Bots").tag("bots")
                    Text("Usage limits").tag("usage")
                }.pickerStyle(.menu)
                Text("Sample data").font(.caption).foregroundStyle(Palette.secondary)
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
                Text("Touch and hold your Lock Screen → Customize → Lock Screen. Tap the widget area and choose Codync. Inline widgets go in the date row above the clock.")
                    .font(.footnote).foregroundStyle(Palette.secondary)
                Text("Choose Bots for task status or Usage limits for the highest reported limit. iOS applies your Lock Screen's color and style.")
                    .font(.footnote).foregroundStyle(Palette.secondary)
            }
            .padding(18).frame(maxWidth: 560).frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .navigationTitle("Lock Screen widgets")
        .navigationBarTitleDisplayMode(.inline)
    }
}
