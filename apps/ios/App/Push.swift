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

/// Registers this phone for "needs you" / "done" alerts with the paired host.
@MainActor
final class PushRegistrar {
    static let shared = PushRegistrar()

    private var deviceToken: Data?
    private weak var model: BotStore?
    private var registrationTask: Task<Void, Never>?

    func deactivate() {
        registrationTask?.cancel()
        registrationTask = nil
        model = nil
    }

    func requestAuthorization() async -> Bool {
        let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        if granted { UIApplication.shared.registerForRemoteNotifications() }
        return granted
    }

    func didRegister(token: Data) {
        deviceToken = token
        if let model { syncDevice(with: model) }
    }

    /// Sends our ticket to the host whenever we have both a token and a pairing.
    func syncDevice(with model: BotStore) {
        self.model = model
        guard let token = deviceToken else {
            Task {
                let settings = await UNUserNotificationCenter.current().notificationSettings()
                if settings.authorizationStatus == .authorized { UIApplication.shared.registerForRemoteNotifications() }
            }
            return
        }
        registrationTask?.cancel()
        let pairing = model.storage.orderedPairing
        registrationTask = Task { [weak model] in
            do {
                let ticket = try await relayTicket(token: token, kind: "alert")
                try Task.checkCancellation()
                guard let model, self.model === model, let pairing,
                      let client = await HostClient.resolve(pairing) else { return }
                try Task.checkCancellation()
                try await client.registerDevice(ticket: ticket, relay: SharedStore.relayURL, name: UIDevice.current.name)
                log.info("device registered with host")
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
                _ = try Activity.request(attributes: BotActivityAttributes(bot: bot, link: URL(string: "codync://computers")),
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

    private nonisolated static func find(_ botId: String) -> Activity<BotActivityAttributes>? {
        Activity<BotActivityAttributes>.activities.first { $0.attributes.botId == botId }
    }

    func start(for bot: Bot, model: BotStore) {
        guard UserDefaults.standard.object(forKey: "liveActivitiesEnabled") as? Bool ?? true else { return }
        guard ActivityAuthorizationInfo().areActivitiesEnabled, Self.find(bot.id) == nil, let client = model.client else { return }
        let attributes = BotActivityAttributes(bot: bot, link: model.storage.botURL(bot.id))
        let id = UUID()
        requests[id] = Task.detached { [weak self] in
            await Self.request(attributes: attributes, client: client)
            await self?.finishedRequest(id)
        }
    }

    private func finishedRequest(_ id: UUID) { requests[id] = nil }

    private nonisolated static func request(attributes: BotActivityAttributes, client: HostClient) async {
        let state = BotActivityAttributes.ContentState(status: "working", activity: "Starting…", startedAt: .now)
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

    /// Local updates while the app is open; the host pushes them otherwise.
    func update(bot: Bot) {
        guard Self.find(bot.id) != nil else { return }
        let state = BotActivityAttributes.ContentState(
            status: bot.status,
            activity: bot.activity,
            startedAt: bot.startedAt.map(Date.init(milliseconds:))
        )
        let botId = bot.id, working = bot.isWorking
        Task.detached { await Self.apply(botId: botId, state: state, end: !working) }
    }

    private nonisolated static func apply(botId: String, state: BotActivityAttributes.ContentState, end: Bool) async {
        guard let activity = find(botId) else { return }
        if end {
            await activity.end(.init(state: state, staleDate: nil), dismissalPolicy: .after(.now + 60))
        } else {
            await activity.update(.init(state: state, staleDate: .now + 15 * 60))
        }
    }
}
