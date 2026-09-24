import CodyncKit
import SwiftUI
import UserNotifications

@main
struct CodyncApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var model = AppModel()
    @State private var router = Router.shared
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .environment(router)
                .tint(Palette.accent)
                .onOpenURL { url in
                    if let p = Pairing(url: url) {
                        model.pair(p)
                    } else if url.host() == "bot" {
                        router.open(botId: url.lastPathComponent)
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

@MainActor
@Observable
final class Router {
    static let shared = Router()
    var path: [String] = []

    func open(botId: String) {
        path = [botId]
    }
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
            await MainActor.run { Router.shared.open(botId: botId) }
        }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        // The chat already shows it live.
        []
    }
}
