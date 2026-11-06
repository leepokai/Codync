import CodyncKit
import CodyncUI
import SwiftUI

/// The Settings tab: what belongs to this iPhone alone (appearance, language, notification permission,
/// analytics, Start over). Each computer's own settings (voice chat, push, connection, hidden bots) are in
/// Computer management (`ComputersView`).
struct SettingsView: View {
    @Environment(AppStore.self) private var app
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    @State private var notificationsAllowed: Bool?
    @AppStorage(Analytics.enabledKey) private var analyticsEnabled = true
    @State private var confirmStartOver = false
    @State private var aboutAnalytics = false

    var body: some View {
        CardForm {
            Text(L10n.tr("Settings"))
                .font(.largeTitle.bold())
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)

            AppearanceSettings()

            LanguageSettings()

            ThinkingDetailsSettings()

            CardSection("Notifications", footer: "Get a result summary, a request for input, or a failure notice. Notification previews follow your iOS settings.") {
                notificationsRow
            }

            CardSection(footer: "Which features you use, never your messages, code or files. Helps us spot bugs sooner and make Codync better for you.") {
                HStack(spacing: 4) {
                    Text(L10n.tr("Share usage analytics")).lineLimit(1).layoutPriority(1)
                    IconButton("About usage analytics", systemImage: "info.circle") { aboutAnalytics = true }
                    Toggle(isOn: Binding(get: { analyticsEnabled }, set: { Analytics.shared.setEnabled($0) })) { EmptyView() }
                        .toggleStyle(.codync)
                        .accessibilityLabel(L10n.tr("Share usage analytics"))
                }
            }

            CardSection {
                Button {
                    Analytics.shared.capture(.reviewLinkOpened)
                    openURL(AppStoreRelease.reviewPage)
                } label: {
                    Text(L10n.tr("Rate on the App Store"))
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.text)
                Button {
                    Analytics.shared.capture(.linkOpened(.issues))
                    openURL(URL(string: "https://github.com/leepokai/Codync/issues")!)
                } label: {
                    Text(L10n.tr("Report an issue"))
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.text)
                ValueRow("App version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                Button {
                    Analytics.shared.capture(.linkOpened(.github))
                    openURL(URL(string: "https://github.com/leepokai/Codync")!)
                } label: {
                    Text(L10n.tr("Source code"))
                }
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.text)
            }

            CardSection {
                Button { confirmStartOver = true } label: {
                    Label(L10n.tr("Start over"), systemImage: "arrow.counterclockwise")
                        .foregroundStyle(Palette.danger)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .codyncDialog("Start over?", isPresented: $confirmStartOver,
                      message: "Signs out of every account and forgets every computer on this iPhone, then shows the welcome again. Your computers keep their bots and chats; pair again to use them.") {
            [DialogAction("Start over", destructive: true) { Task { await app.startOver() } }]
        }
        .task { notificationsAllowed = await PushRegistrar.shared.isAllowed() }
        // The tab stays alive: coming back from iOS Settings reads the permission again.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { notificationsAllowed = await PushRegistrar.shared.isAllowed() } }
        }
        .codyncSheet(isPresented: $aboutAnalytics) { AnalyticsSheet() }
    }

    private var notificationsRow: some View {
        Button {
            if notificationsAllowed != nil {
                UIApplication.shared.open(URL(string: UIApplication.openNotificationSettingsURLString)!)
            } else {
                Task { notificationsAllowed = await PushRegistrar.shared.requestAuthorization(from: .settings) }
            }
        } label: {
            ValueRow("Notifications", value: notificationsAllowed == true ? "On" : notificationsAllowed == false ? "Off in Settings" : "Turn on")
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
