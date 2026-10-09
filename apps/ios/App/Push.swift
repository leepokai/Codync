import ActivityKit
import CodyncKit
import CodyncUI
import Foundation
import UIKit
import UserNotifications
import os

private let log = Logger(subsystem: "com.pokai.Codync.ios", category: "Push")

private var apnsEnvironment: String {
    Bundle.main.object(forInfoDictionaryKey: "CodyncAPNSEnvironment") as? String ?? "production"
}

/// Turns an APNs token into a relay ticket (see relay/src/index.ts).
private func relayTicket(token: Data, kind: String) async throws -> String {
    struct Body: Encodable { var token: String; var env: String; var kind: String; var bundleId: String }
    struct Res: Decodable { var ticket: String }
    var req = URLRequest(url: URL(string: "\(SharedStore.relayURL)/register")!)
    req.httpMethod = "POST"
    req.setValue("application/json", forHTTPHeaderField: "Content-Type")
    req.httpBody = try JSONEncoder().encode(Body(token: token.map { String(format: "%02x", $0) }.joined(), env: apnsEnvironment, kind: kind, bundleId: Bundle.main.bundleIdentifier ?? "com.pokai.Codync.ios"))
    req.timeoutInterval = 15
    let (data, response) = try await URLSession.shared.data(for: req)
    guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
        throw URLError(.badServerResponse)
    }
    let ticket = try JSONDecoder().decode(Res.self, from: data).ticket
    guard !ticket.isEmpty else { throw URLError(.cannotParseResponse) }
    return ticket
}

/// Registers this phone for input, completion, and failure alerts with every computer it's connected to.
/// Each ticket carries this context's push key so the computer can seal the alert text (spec §6.7).
@MainActor
final class PushRegistrar {
    static let shared = PushRegistrar()

    private var deviceToken: Data?
    private var stores: [ComputerID: WeakStore] = [:]
    private var registrations: [ComputerID: Task<Void, Never>] = [:]

    private struct WeakStore { weak var store: BotStore? }

    #if DEBUG
    /// Device acceptance checks can inspect only the explicitly selected test bot's notifications.
    /// No keys, tickets, account identifiers, or unrelated notification content are logged.
    func verifyDeliveryIfRequested() {
        guard let botId = ProcessInfo.processInfo.environment["CODYNC_PUSH_VERIFY_BOT"] else { return }
        let activities = Activity<BotActivityAttributes>.activities.filter { $0.attributes.botId == botId }.map {
            ["status": $0.content.state.status, "state": String(describing: $0.activityState)]
        }
        Task {
            let center = UNUserNotificationCenter.current()
            let settings = await center.notificationSettings()
            let delivered = await center.deliveredNotifications().filter {
                $0.request.content.userInfo["botId"] as? String == botId
            }
            let notifications = delivered.map {
                ["title": $0.request.content.title, "subtitle": $0.request.content.subtitle,
                 "body": $0.request.content.body, "category": $0.request.content.categoryIdentifier]
            }
            if ProcessInfo.processInfo.environment["CODYNC_PUSH_CLEAR_TEST_NOTIFICATIONS"] == "1" {
                center.removeDeliveredNotifications(withIdentifiers: delivered.map { $0.request.identifier })
            }
            let result: [String: Any] = [
                "notifications": notifications, "activities": activities,
                "authorization": settings.authorizationStatus.rawValue,
                "alerts": settings.alertSetting.rawValue,
                "notificationCenter": settings.notificationCenterSetting.rawValue,
            ]
            if let data = try? JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]),
               let text = String(data: data, encoding: .utf8) {
                print("CODYNC_PUSH_VERIFICATION \(text)")
            }
        }
    }
    #endif

    func deactivate() {
        registrations.values.forEach { $0.cancel() }
        registrations = [:]
        stores = [:]
    }

    /// nil until asked; false once turned off in the Settings app.
    func isAllowed() async -> Bool? {
        switch await UNUserNotificationCenter.current().notificationSettings().authorizationStatus {
        case .authorized, .provisional, .ephemeral: true
        case .denied: false
        default: nil
        }
    }

    func requestAuthorization() async -> Bool {
        let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        if granted { UIApplication.shared.registerForRemoteNotifications() }
        return granted
    }

    func didRegister(token: Data) {
        deviceToken = token
        resync()
    }

    /// The in-app switch; off asks every computer to stop alerting this phone.
    static var enabled: Bool { UserDefaults.standard.object(forKey: "notificationsEnabled") as? Bool ?? true }

    func resync() {
        for entry in stores.values { if let store = entry.store { syncDevice(with: store) } }
    }

    /// Sends our ticket to a computer whenever we have both a token and a connection to it.
    func syncDevice(with store: BotStore) {
        let id = store.computer.id
        stores[id] = WeakStore(store: store)
        LiveActivities.shared.resume(with: store)
        guard Self.enabled else {
            registrations[id]?.cancel()
            registrations[id] = Task { [weak store] in
                do { try await store?.client?.unregisterDevice() } catch {
                    log.error("device unregistration failed: \(error.localizedDescription)")
                }
            }
            return
        }
        guard let token = deviceToken else {
            Task {
                let settings = await UNUserNotificationCenter.current().notificationSettings()
                if settings.authorizationStatus == .authorized || settings.authorizationStatus == .provisional { UIApplication.shared.registerForRemoteNotifications() }
            }
            return
        }
        registrations[id]?.cancel()
        let storage = store.storage
        registrations[id] = Task { [weak store] in
            do {
                let identity = try DeviceIdentity.load(context: storage)
                let ticket = try await relayTicket(token: token, kind: "alert")
                try Task.checkCancellation()
                guard let client = store?.client else { return }
                try await client.registerDevice(ticket: ticket, relay: SharedStore.relayURL, name: UIDevice.current.name,
                                                pushKey: identity.pushKey, ctx: storage.id)
                log.info("device registered with a computer")
            } catch is CancellationError {
                return
            } catch {
                log.error("device registration failed: \(error.localizedDescription)")
            }
        }
    }
}

/// One Live Activity per bot the user delegated to from this phone.
@MainActor
final class LiveActivities {
    static let shared = LiveActivities()

    private var requests: [String: Task<Void, Never>] = [:]

    #if DEBUG
    /// Simulator-only visual verification. No host, relay, or APNs request.
    func previewIfRequested() async {
        #if targetEnvironment(simulator)
        guard let status = ProcessInfo.processInfo.environment["CODYNC_ACTIVITY_PREVIEW"] else { return }
        for activity in Activity<BotActivityAttributes>.activities where activity.attributes.botId.hasPrefix("codync-design-preview-") {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
        guard status != "stop", var bot = Bot.widgetPreview.first else { return }
        try? await Task.sleep(for: .milliseconds(500))
        for index in 0..<(status == "multiple" ? 2 : 1) {
            bot.id = "codync-design-preview-\(index)"
            bot.name = index == 0 ? "Reviewer" : "Builder"
            let state = BotActivityAttributes.ContentState(
                status: status == "multiple" ? (index == 0 ? "needsInput" : "working") : status == "stale" ? "working" : status,
                activity: status == "needsInput" || status == "multiple" ? "Review the proposed changes." : "Running the test suite.",
                startedAt: .now - 154)
            do {
                _ = try Activity.request(attributes: BotActivityAttributes(bot: bot, computerId: "preview", link: AppEnvironment.current.link("computers")),
                    content: .init(state: state, staleDate: status == "stale" ? .now - 1 : .now + 900), pushType: nil)
            } catch { log.error("Simulator activity preview: \(error.localizedDescription)") }
        }
        #endif
    }
    #endif

    func endAll() {
        requests.values.forEach { $0.cancel() }
        requests.removeAll()
        let ids = Set(Activity<BotActivityAttributes>.activities.map(\.id))
        Task.detached {
            for activity in Activity<BotActivityAttributes>.activities where ids.contains(activity.id) {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
        }
    }

    private nonisolated static func find(_ ref: BotReference) -> Activity<BotActivityAttributes>? {
        Activity<BotActivityAttributes>.activities.first { $0.attributes.botId == ref.botId && $0.attributes.computerId == ref.computerId }
    }

    /// Follows a message from this phone: the activity starts as it leaves (Sending), waits with
    /// it in the relay mailbox while the computer is offline, then follows the bot's own status.
    func sent(_ ref: BotReference, bot: Bot, progress: SendProgress, store: BotStore) {
        let status: String
        switch progress {
        case .sending: status = "sending"
        case .queued: status = "queued"
        case .delivered: status = bot.needsInput ? "needsInput" : "working"
        case .failed:
            let state = BotActivityAttributes.ContentState(status: "error", activity: "Couldn't send", startedAt: nil)
            Task.detached { await Self.apply(ref, state: state, end: true) }
            return
        case .cancelled:
            Task.detached { await Self.find(ref)?.end(nil, dismissalPolicy: .immediate) }
            return
        }
        let state = BotActivityAttributes.ContentState(status: status, activity: "", startedAt: progress == .delivered ? .now : nil)
        // A mailbox message may wait hours for its computer: that isn't a delayed update.
        let staleDate: Date? = progress == .queued ? nil : .now + 15 * 60
        if Self.find(ref) != nil {
            Task.detached { await Self.find(ref)?.update(.init(state: state, staleDate: staleDate)) }
            return
        }
        guard UserDefaults.standard.object(forKey: "liveActivitiesEnabled") as? Bool ?? true,
              ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        let attributes = BotActivityAttributes(bot: bot, computerId: ref.computerId, link: store.storage.botURL(ref))
        do {
            let activity = try Activity.request(attributes: attributes, content: .init(state: state, staleDate: staleDate), pushType: .token)
            // Offline (mailbox): `resume(with:)` registers the push token once the computer connects.
            if let client = store.client { observe(activity, client: client) }
        } catch {
            log.error("Live activity not started: \(error.localizedDescription)")
        }
    }

    /// Reattach token observers after a relaunch or connection replacement.
    func resume(with store: BotStore) {
        guard UserDefaults.standard.object(forKey: "liveActivitiesEnabled") as? Bool ?? true else { return }
        guard let client = store.client else { return }
        for activity in Activity<BotActivityAttributes>.activities
            where activity.attributes.computerId == store.computer.id {
            observe(activity, client: client)
        }
    }

    private func observe(_ activity: Activity<BotActivityAttributes>, client: HostClient) {
        requests[activity.id]?.cancel()
        requests[activity.id] = Task {
            if let token = activity.pushToken { await Self.register(token, activity: activity, client: client) }
            for await token in activity.pushTokenUpdates {
                guard !Task.isCancelled else { return }
                await Self.register(token, activity: activity, client: client)
            }
        }
    }

    private static func register(_ token: Data, activity: Activity<BotActivityAttributes>, client: HostClient) async {
        for attempt in 0..<3 {
            do {
                try Task.checkCancellation()
                guard activity.activityState == .active || activity.activityState == .stale else { return }
                let ticket = try await relayTicket(token: token, kind: "liveactivity")
                try Task.checkCancellation()
                try await client.registerActivity(botId: activity.attributes.botId, ticket: ticket)
                return
            } catch is CancellationError {
                return
            } catch {
                log.error("Live activity registration failed: \(error.localizedDescription)")
                guard attempt < 2 else { return }
                do { try await Task.sleep(for: .seconds(attempt + 1)) } catch { return }
            }
        }
    }

    /// Local updates while the app is open; the computer pushes them otherwise.
    func update(_ ref: BotReference, bot: Bot) {
        guard let current = Self.find(ref) else { return }
        // Until the computer starts on the message, an idle bot just hasn't begun yet.
        let local = ["sending", "queued"].contains(current.content.state.status)
        guard !local || bot.isWorking else { return }
        let state = BotActivityAttributes.ContentState(
            status: bot.status,
            // ACP activity is already visible in the app. Show it in the local Live Activity
            // while this phone is connected; APNs payloads continue to contain no free text.
            activity: bot.activity,
            startedAt: bot.startedAt.map(Date.init(milliseconds:))
        )
        let working = bot.isWorking
        Task.detached { await Self.apply(ref, state: state, end: !working) }
    }

    private nonisolated static func apply(_ ref: BotReference, state: BotActivityAttributes.ContentState, end: Bool) async {
        guard let activity = find(ref) else { return }
        if end {
            await activity.end(.init(state: state, staleDate: nil), dismissalPolicy: .after(.now + 60))
        } else {
            await activity.update(.init(state: state, staleDate: .now + 15 * 60))
        }
    }
}
