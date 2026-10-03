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
    /// What the App Store has: greys out an update it doesn't have yet, reminds of newer ones.
    @State private var updates = AppUpdates()
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage("onboardingCompleted") private var onboardingCompleted = false

    var body: some Scene {
        WindowGroup {
            RootView()
                .id("\(app.contextID)#\(app.generation)")
                .environment(app)
                .environment(app.accounts)
                .environment(app.account)
                // A computer needs a newer app: its App Store page has the Update button.
                .environment(\.appUpdate, AppUpdateAction { UIApplication.shared.open(AppStoreRelease.page) })
                .environment(updates)
                .tint(Palette.accent)
                .codyncSheet(isPresented: Binding(
                    get: { (onboardingCompleted || app.account.isSignedIn) && app.account.showSwitcher },
                    set: { app.account.showSwitcher = $0 }
                )) {
                    // Holds the pushes inside the sheet (Computers & settings, Widgets); no bar shows.
                    NavigationStack { AccountSwitcherView() }
                        .environment(app)
                        .environment(app.accounts)
                        .environment(app.account)
                        .tint(Palette.accent)
                }
                .onChange(of: app.account.userID, initial: true) { _, userID in
                    app.switchAccount(to: userID)
                }
                .onOpenURL { url in app.open(url) }
                .onChange(of: scenePhase, initial: true) { _, phase in
                    // Only leaving for the background disconnects; `.inactive` (Control Center,
                    // app switcher, system prompts) comes and goes too often to drop the link.
                    app.accounts.setActive(phase != .background)
                    // Back in the app: a computer may have joined the account meanwhile.
                    if phase == .active {
                        Task { await app.accounts.refreshCloud() }
                        Task { await updates.refresh() }
                    }
                }
                #if DEBUG
                .task {
                    // Simulator/UI-test pairing: SIMCTL_CHILD_CODYNC_PAIR_URL=codync://pair?...
                    // Skips the notification prompt, which nothing can dismiss in a headless simulator.
                    if let s = ProcessInfo.processInfo.environment["CODYNC_PAIR_URL"], let p = Pairing(string: s) {
                        _ = try? await app.accounts.pair(p, deviceName: UIDevice.current.name, platform: "ios")
                    }
                    if ProcessInfo.processInfo.environment["CODYNC_OPEN_USAGE"] != nil { app.tab = .state; app.showUsage = true }
                    if ProcessInfo.processInfo.environment["CODYNC_STATE_SURFACE"] != nil { app.tab = .state }
                    // Screenshots: CODYNC_OPEN_URL=codync://bot/<botId>?scope=local&computer=<id>, without the system prompt.
                    if let s = ProcessInfo.processInfo.environment["CODYNC_OPEN_URL"], let url = URL(string: s) {
                        try? await Task.sleep(for: .seconds(1))
                        app.open(url)
                    }
                    await LiveActivities.shared.previewIfRequested()
                    if ProcessInfo.processInfo.environment["CODYNC_OPEN_SCREEN"] != nil {
                        try? await Task.sleep(for: .seconds(2))
                        app.currentStore?.screenRequest = ScreenRequest()
                    }
                }
                #endif
        }
    }
}

enum AppTab: Hashable { case bots, state }

/// One `AccountStore` per account context: switching accounts retires the old one so
/// in-flight work can't land on another user's screen or storage.
@MainActor
@Observable
final class AppStore {
    static let shared = AppStore()
    let account: AccountSession
    private(set) var accounts: AccountStore
    /// The signed-in account's cloud; nil when signed out or the build has no cloud.
    private(set) var cloud: CloudClient?
    private(set) var contextID: String
    /// Bumped by Start over so the whole UI is rebuilt, even though the local context keeps its id.
    private(set) var generation = 0
    var tab = AppTab.bots
    var showComputers = false
    /// The usage limits sheet over the State tab.
    var showUsage = false
    /// The computer whose marketplace is open.
    var marketplace: ComputerID?

    private init() {
        let session = AccountSession()
        let storage = SharedStore.Context(accountID: session.userID)
        SharedStore.activeAccountID = session.userID
        account = session
        contextID = storage.id
        (accounts, cloud) = Self.makeAccounts(storage, session: session)
    }

    var storage: SharedStore.Context { accounts.storage }

    /// The computer the Usage tab, widgets and "open screen" links act on: the last active one.
    var currentStore: BotStore? {
        accounts.storage.lastComputerId.flatMap(accounts.store(for:)) ?? accounts.computers.first.flatMap { accounts.store(for: $0.id) }
    }

    func switchAccount(to userID: String?) {
        let storage = SharedStore.Context(accountID: userID)
        guard storage.id != contextID else { return }
        accounts.retire()
        PushRegistrar.shared.deactivate()
        LiveActivities.shared.endAll()
        SharedStore.activeAccountID = userID
        storage.bots = []
        contextID = storage.id
        tab = .bots
        showComputers = false
        marketplace = nil
        (accounts, cloud) = Self.makeAccounts(storage, session: account)
        BotsWidgetFeed.reset()
        WidgetCenter.shared.reloadAllTimelines()
    }

    /// Signing out forgets this account on this iPhone: its device keys, computers and caches (spec §3.2).
    func signOut() async {
        guard let userID = account.userID else { return }
        await account.signOut()
        guard account.userID != userID, !account.accounts.contains(where: { $0.id == userID }) else { return }
        // Retire its stores first: a retiring store saves its cache one last time.
        switchAccount(to: account.userID)
        SharedStore.Context(accountID: userID).erase()
    }

    /// Settings → Start over: signs out of every account and forgets every computer, key and cache
    /// on this iPhone, then shows the welcome again. The computers keep their bots and chats.
    func startOver() async {
        let userIDs = account.accounts.map(\.id)
        await account.signOutAll()
        accounts.retire()
        PushRegistrar.shared.deactivate()
        LiveActivities.shared.endAll()
        for id in userIDs { SharedStore.Context(accountID: id).erase() }
        let storage = SharedStore.Context(accountID: nil)
        storage.erase()
        storage.bots = []
        SharedStore.activeAccountID = nil
        UserDefaults.standard.set(false, forKey: "onboardingCompleted")
        showComputers = false
        account.showSwitcher = false
        marketplace = nil
        tab = .bots
        contextID = storage.id
        (accounts, cloud) = Self.makeAccounts(storage, session: account)
        generation += 1
        BotsWidgetFeed.reset()
        WidgetCenter.shared.reloadAllTimelines()
    }

    func pair(_ pairing: Pairing) async throws -> Computer {
        let computer = try await accounts.pair(pairing, deviceName: UIDevice.current.name, platform: "ios")
        Task { _ = await PushRegistrar.shared.requestAuthorization() }
        return computer
    }

    func open(_ url: URL) {
        if url.scheme == "com.pokai.Codync.ios" {
            Task { await account.handle(url) }
            return
        }
        switch url.host() {
        case "pair":
            // Scanned with the Camera app: pair right away.
            let accounts = accounts
            Task {
                do { _ = try await pair(try Pairing.parse(url)) } catch { accounts.lastError = error.localizedDescription }
            }
        case "bot":
            guard let ref = storage.reference(from: url), accounts.store(for: ref.computerId) != nil else { return }
            tab = .bots
            accounts.selection = ref
        case "plugins":
            accounts.selection = nil
            tab = .bots
            marketplace = currentStore?.computer.id
        case "usage":
            accounts.selection = nil
            tab = .state
            showUsage = true
        case "computers":
            accounts.selection = nil
            tab = .bots
            showComputers = true
        case "screen":
            currentStore?.screenRequest = ScreenRequest()
        default:
            break
        }
    }

    private static func makeAccounts(_ storage: SharedStore.Context, session: AccountSession) -> (AccountStore, CloudClient?) {
        let identity = storage.accountID == nil ? nil : try? DeviceIdentity.load(context: storage)
        let cloud = session.cloudClient(for: storage.accountID, identity: identity)
        let accounts = AccountStore(storage: storage, clientKind: "ios", cloud: cloud)
        accounts.asksForAccess = true
        accounts.onConnected = { store in
            PushRegistrar.shared.syncDevice(with: store)
            store.onUsageChanged = { _, _ in WidgetCenter.shared.reloadTimelines(ofKind: "CodyncUsage") }
        }
        accounts.onBotUpdated = { ref, bot in LiveActivities.shared.update(ref, bot: bot) }
        accounts.onRosterChanged = { roster in
            BotsWidgetFeed.update(roster, storage: storage)
            if roster.isEmpty {
                LiveActivities.shared.endAll()
                WidgetCenter.shared.reloadAllTimelines()
            }
        }
        accounts.onSent = { [weak accounts] ref, bot in
            guard let store = accounts?.store(for: ref.computerId) else { return }
            LiveActivities.shared.start(ref, bot: bot, store: store)
        }
        if let cloud {
            // Signed in: make this iPhone known to the account, then list its computers.
            Task { [weak accounts] in
                do {
                    try await cloud.registerDevice(name: UIDevice.current.name, platform: "ios")
                } catch let error as CloudError where !error.isTransient {
                    accounts?.lastError = error.localizedDescription
                } catch {
                    // Transient (network, busy cloud, token not ready): the next launch registers again.
                }
                await accounts?.refreshCloud()
            }
        }
        return (accounts, cloud)
    }

    /// Takes back this iPhone's account access to a computer (its grant), and forgets the computer here.
    /// Also the way out when an earlier install was approved but this one never checked the code.
    func revokeAccess(_ id: ComputerID) async {
        guard let cloud else { return }
        let accounts = accounts
        do {
            let identity = try DeviceIdentity.load(context: accounts.storage)
            if let device = try await cloud.devices().first(where: { $0.deviceKey == identity.publicKey }) {
                for grant in try await cloud.grants(id) where grant.deviceId == device.deviceId {
                    try await cloud.revokeGrant(id, grantId: grant.grantId)
                }
            }
            accounts.forget(id)
            await accounts.refreshCloud()
        } catch {
            accounts.lastError = error.localizedDescription
        }
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        let open = UNNotificationAction(identifier: "openConversation", title: "Open conversation", options: [.foreground])
        let review = UNNotificationAction(identifier: "reviewRequest", title: "Review request", options: [.foreground])
        UNUserNotificationCenter.current().setNotificationCategories([
            UNNotificationCategory(identifier: "done", actions: [open], intentIdentifiers: []),
            UNNotificationCategory(identifier: "needsInput", actions: [review], intentIdentifiers: []),
            UNNotificationCategory(identifier: "failed", actions: [open], intentIdentifiers: []),
        ])
        #if DEBUG
        PushRegistrar.shared.verifyDeliveryIfRequested()
        #endif
        return true
    }

    /// Portrait, except while the screen viewer is open.
    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        OrientationLock.mask
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in PushRegistrar.shared.didRegister(token: deviceToken) }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
                                          withCompletionHandler completionHandler: @escaping @Sendable () -> Void) {
        let info = response.notification.request.content.userInfo
        let ctx = info["ctx"] as? String
        let computerId = info["computerId"] as? String
        let botId = info["botId"] as? String
        // UIKit saves application state from this completion. The async delegate
        // bridge can complete off the main thread even after MainActor.run.
        Task { @MainActor in
            defer { completionHandler() }
            guard let ctx, let computerId, let botId else { return }
            // Only open it in the account it was sent to, on a computer that account still has.
            let accounts = AppStore.shared.accounts
            guard ctx == accounts.storage.id, accounts.store(for: computerId) != nil else { return }
            AppStore.shared.tab = .bots
            accounts.selection = BotReference(accountId: accounts.accountId, computerId: computerId, botId: botId)
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
    private struct Snapshot: Equatable {
        let computerId: ComputerID
        let id: String
        let name: String
        let shape: String
        let color: String
        let status: String
        let activity: String
        let message: String?
        let lastAt: Int64

        init(_ bot: Bot, computerId: ComputerID) {
            self.computerId = computerId
            id = bot.id; name = bot.name; shape = bot.avatarShape; color = bot.avatarColor
            status = bot.status; activity = bot.activity; message = bot.lastMessage; lastAt = bot.lastAt
        }
    }
    private static var shown: [Snapshot]?
    private static var scope: String?

    static func reset() { shown = nil; scope = nil }

    static func update(_ roster: [RosterItem], storage: SharedStore.Context) {
        guard storage.id == SharedStore.activeContext.id else { return }
        storage.bots = roster.map { BotSnapshot(computerId: $0.ref.computerId, bot: $0.bot) }
        let currentScope = storage.id
        let signature = roster.filter { !$0.bot.hidden }.map { Snapshot($0.bot, computerId: $0.ref.computerId) }
        guard signature != shown || scope != currentScope else { return }
        shown = signature
        scope = currentScope
        WidgetCenter.shared.reloadTimelines(ofKind: "CodyncBots")
    }
}
