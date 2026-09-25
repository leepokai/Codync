import CodyncKit
import CodyncUI
import SwiftUI
import UserNotifications
import WidgetKit

@main
struct CodyncApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var model = AppStore.shared
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .tint(Palette.accent)
                .onOpenURL { url in
                    if let p = Pairing(url: url) {
                        model.pair(p)
                    } else if url.host() == "bot" {
                        model.selection = url.lastPathComponent
                    } else if url.host() == "plugins" {
                        model.selection = nil
                        model.showPlugins = true
                    } else if url.host() == "computers" {
                        model.selection = nil
                        model.showProfile = true
                    }
                }
                .onChange(of: scenePhase, initial: true) { _, phase in
                    model.setActive(phase == .active)
                }
                #if DEBUG
                .task {
                    // Simulator/UI-test pairing: SIMCTL_CHILD_CODYNC_PAIR_URL=codync://pair?...
                    if let s = ProcessInfo.processInfo.environment["CODYNC_PAIR_URL"], let p = Pairing(string: s) {
                        model.pair(p)
                    }
                }
                #endif
        }
    }
}

/// The phone's store, wired to push registration, Live Activities and widgets.
@MainActor
enum AppStore {
    static let shared: BotStore = {
        let store = BotStore(pairing: SharedStore.pairing, clientKind: "ios", persistsPairing: true)
        store.onPaired = { PushRegistrar.shared.syncDevice(with: $0) }
        store.onBotUpdated = {
            LiveActivities.shared.update(bot: $0)
            BotsWidgetFeed.update(store.roster)
        }
        store.onSent = { LiveActivities.shared.start(for: $0, model: store) }
        store.onUsageChanged = { usage in
            SharedStore.usage = usage
            WidgetCenter.shared.reloadAllTimelines()
        }
        return store
    }()
}

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in PushRegistrar.shared.didRegister(token: deviceToken) }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let botId = response.notification.request.content.userInfo["botId"] as? String
        if let botId {
            await MainActor.run { AppStore.shared.selection = botId }
        }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        // The chat already shows it live.
        []
    }
}

/// Keeps the Bots widget's snapshot current; asks WidgetKit to redraw only when
/// something the widget shows changes (who is working / needs you / unread).
@MainActor
enum BotsWidgetFeed {
    private static var shown = ""

    static func update(_ roster: [Bot]) {
        SharedStore.bots = roster
        let signature = roster.map { "\($0.id):\($0.status):\($0.unread)" }.joined(separator: ",")
        guard signature != shown else { return }
        shown = signature
        WidgetCenter.shared.reloadTimelines(ofKind: "CodyncBots")
    }
}
