import ActivityKit
import CodyncKit
import CodyncUI
import Foundation
import UIKit
import UserNotifications
import os

private let log = Logger(subsystem: "com.pokai.Codync.ios", category: "Push")

private var apnsEnvironment: String {
    #if DEBUG
    "sandbox"
    #else
    "production"
    #endif
}

/// Turns an APNs token into a relay ticket (see relay/src/index.ts).
private func relayTicket(token: Data, kind: String) async throws -> String {
    struct Body: Encodable { var token: String; var env: String; var kind: String }
    struct Res: Decodable { var ticket: String }
    var req = URLRequest(url: URL(string: "\(SharedStore.relayURL)/register")!)
    req.httpMethod = "POST"
    req.setValue("application/json", forHTTPHeaderField: "Content-Type")
    req.httpBody = try JSONEncoder().encode(Body(token: token.map { String(format: "%02x", $0) }.joined(), env: apnsEnvironment, kind: kind))
    let (data, _) = try await URLSession.shared.data(for: req)
    return try JSONDecoder().decode(Res.self, from: data).ticket
}

/// Registers this phone for "needs you" / "done" alerts with every computer it's connected to.
/// Each ticket carries this context's push key so the computer can seal the alert text (spec §6.7).
@MainActor
final class PushRegistrar {
    static let shared = PushRegistrar()

    private var deviceToken: Data?
    private var stores: [ComputerID: WeakStore] = [:]
    private var registrations: [ComputerID: Task<Void, Never>] = [:]

    private struct WeakStore { weak var store: BotStore? }

    func deactivate() {
        registrations.values.forEach { $0.cancel() }
        registrations = [:]
        stores = [:]
    }

    func requestAuthorization() async -> Bool {
        let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        if granted { UIApplication.shared.registerForRemoteNotifications() }
        return granted
    }

    func didRegister(token: Data) {
        deviceToken = token
        for entry in stores.values { if let store = entry.store { syncDevice(with: store) } }
    }

    /// Sends our ticket to a computer whenever we have both a token and a connection to it.
    func syncDevice(with store: BotStore) {
        let id = store.computer.id
        stores[id] = WeakStore(store: store)
        guard let token = deviceToken else {
            Task {
                let settings = await UNUserNotificationCenter.current().notificationSettings()
                if settings.authorizationStatus == .authorized { UIApplication.shared.registerForRemoteNotifications() }
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

    private var requests: [UUID: Task<Void, Never>] = [:]

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
                _ = try Activity.request(attributes: BotActivityAttributes(bot: bot, computerId: "preview", link: URL(string: "codync://computers")),
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

    func start(_ ref: BotReference, bot: Bot, store: BotStore) {
        guard UserDefaults.standard.object(forKey: "liveActivitiesEnabled") as? Bool ?? true else { return }
        guard ActivityAuthorizationInfo().areActivitiesEnabled, Self.find(ref) == nil, let client = store.client else { return }
        let attributes = BotActivityAttributes(bot: bot, computerId: ref.computerId, link: store.storage.botURL(ref))
        let id = UUID()
        requests[id] = Task.detached { [weak self] in
            await Self.request(attributes: attributes, client: client)
            await self?.finishedRequest(id)
        }
    }

    private func finishedRequest(_ id: UUID) { requests[id] = nil }

    private nonisolated static func request(attributes: BotActivityAttributes, client: HostClient) async {
        // Status only: pushes never carry free text, so local updates don't either.
        let state = BotActivityAttributes.ContentState(status: "working", activity: "", startedAt: .now)
        do {
            try Task.checkCancellation()
            let activity = try Activity.request(attributes: attributes, content: .init(state: state, staleDate: .now + 15 * 60), pushType: .token)
            for await token in activity.pushTokenUpdates {
                guard !Task.isCancelled else { break }
                guard let ticket = try? await relayTicket(token: token, kind: "liveactivity") else { continue }
                guard !Task.isCancelled else { break }
                try? await client.registerActivity(botId: attributes.botId, ticket: ticket)
            }
            if Task.isCancelled { await activity.end(nil, dismissalPolicy: .immediate) }
        } catch {
            log.info("live activity not started: \(error.localizedDescription)")
        }
    }

    /// Local updates while the app is open; the computer pushes them otherwise.
    func update(_ ref: BotReference, bot: Bot) {
        guard Self.find(ref) != nil else { return }
        let state = BotActivityAttributes.ContentState(
            status: bot.status,
            activity: "",
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
