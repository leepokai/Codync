import CodyncKit
import CodyncUI
import SwiftUI
import Observation
import UserNotifications
import WidgetKit

@main
struct CodyncApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var app = AppStore.shared
    private var model: BotStore { app.model }
    @Environment(\.scenePhase) private var scenePhase
    @State private var tab = AppTab.bots

    var body: some Scene {
        WindowGroup {
            RootView(tab: $tab)
                .id(app.contextID)
                .environment(model)
                .environment(app.account)
                .tint(Palette.accent)
                .sheet(isPresented: Bindable(app.account).showSwitcher) {
                    NavigationStack { AccountSwitcherView().environment(app.account).environment(model) }
                }
                .onChange(of: app.account.userID, initial: true) { _, userID in
                    app.switchAccount(to: userID)
                    tab = .bots
                }
                .onOpenURL { url in
                    if url.scheme == "com.pokai.Codync.ios" {
                        Task { await app.account.handle(url) }
                    } else if let p = Pairing(url: url) {
                        model.pair(p)
                    } else if url.host() == "bot" {
                        guard model.storage.acceptsBotURL(url) else { return }
                        model.selection = url.lastPathComponent
                    } else if url.host() == "plugins" {
                        model.selection = nil
                        tab = .bots
                        model.showPlugins = true
                    } else if url.host() == "usage" {
                        tab = .usage
                    } else if url.host() == "computers" {
                        model.selection = nil
                        tab = .bots
                        model.showProfile = true
                    } else if url.host() == "screen" {
                        model.screenRequest = ScreenRequest()
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
                    if ProcessInfo.processInfo.environment["CODYNC_OPEN_USAGE"] != nil { tab = .usage }
                    if ProcessInfo.processInfo.environment["CODYNC_OPEN_SCREEN"] != nil {
                        try? await Task.sleep(for: .seconds(2))
                        model.screenRequest = ScreenRequest()
                    }
                }
                #endif
        }
    }
}

/// A new host mirror for each account prevents in-flight work from being
/// applied to another user's screen or storage. Cloud ownership is separate.
@MainActor
@Observable
final class AppStore {
    static let shared = AppStore()
    let account = AccountSession()
    private(set) var model: BotStore
    private(set) var contextID: String

    private init() {
        let storage = SharedStore.Context(accountID: account.userID)
        SharedStore.activeAccountID = account.userID
        contextID = storage.id
        model = Self.makeStore(storage)
    }

    func switchAccount(to userID: String?) {
        let storage = SharedStore.Context(accountID: userID)
        guard storage.id != contextID else { return }
        model.retire()
        PushRegistrar.shared.deactivate()
        LiveActivities.shared.endAll()
        SharedStore.activeAccountID = userID
        storage.bots = []
        storage.usage = nil
        contextID = storage.id
        model = Self.makeStore(storage)
        BotsWidgetFeed.reset()
        WidgetCenter.shared.reloadAllTimelines()
        model.setActive(true)
    }

    private static func makeStore(_ storage: SharedStore.Context) -> BotStore {
        let store = BotStore(pairing: storage.pairing, clientKind: "ios", persistsPairing: true, storage: storage)
        store.onPaired = { PushRegistrar.shared.syncDevice(with: $0) }
        store.onBotUpdated = { [weak store] bot in
            guard let store else { return }
            LiveActivities.shared.update(bot: bot)
            BotsWidgetFeed.update(store.roster)
        }
        store.onSent = { [weak store] bot in
            guard let store else { return }
            LiveActivities.shared.start(for: bot, model: store)
        }
        store.onUsageChanged = { usage in
            storage.usage = usage
            WidgetCenter.shared.reloadAllTimelines()
        }
        return store
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    /// Portrait, except while the screen viewer is open.
    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        OrientationLock.mask
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in PushRegistrar.shared.didRegister(token: deviceToken) }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let botId = response.notification.request.content.userInfo["botId"] as? String
        if let botId {
            await MainActor.run {
                // Legacy push payloads have no account/computer identity. Never
                // route one into a signed-in account using only a naked bot ID.
                let app = AppStore.shared
                guard app.account.userID == nil, app.model.bots[botId] != nil else { return }
                app.model.selection = botId
            }
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

    static func reset() { shown = "" }

    static func update(_ roster: [Bot]) {
        SharedStore.bots = roster
        let signature = roster.map { "\($0.id):\($0.status):\($0.unread)" }.joined(separator: ",")
        guard signature != shown else { return }
        shown = signature
        WidgetCenter.shared.reloadTimelines(ofKind: "CodyncBots")
    }
}
