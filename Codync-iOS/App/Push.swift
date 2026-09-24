import ActivityKit
import CodyncKit
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
    private weak var model: AppModel?

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
    func syncDevice(with model: AppModel) {
        self.model = model
        guard let token = deviceToken else {
            Task {
                let settings = await UNUserNotificationCenter.current().notificationSettings()
                if settings.authorizationStatus == .authorized { UIApplication.shared.registerForRemoteNotifications() }
            }
            return
        }
        Task {
            do {
                let ticket = try await relayTicket(token: token, kind: "alert")
                guard let pairing = SharedStore.orderedPairing, let client = await HostClient.resolve(pairing) else { return }
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

    private nonisolated static func find(_ botId: String) -> Activity<BotActivityAttributes>? {
        Activity<BotActivityAttributes>.activities.first { $0.attributes.botId == botId }
    }

    func start(for bot: Bot, model: AppModel) {
        guard ActivityAuthorizationInfo().areActivitiesEnabled, Self.find(bot.id) == nil, let client = model.client else { return }
        let attributes = BotActivityAttributes(bot: bot)
        Task.detached { await Self.request(attributes: attributes, client: client) }
    }

    private nonisolated static func request(attributes: BotActivityAttributes, client: HostClient) async {
        let state = BotActivityAttributes.ContentState(status: "working", activity: "Starting…", startedAt: .now)
        do {
            let activity = try Activity.request(attributes: attributes, content: .init(state: state, staleDate: nil), pushType: .token)
            for await token in activity.pushTokenUpdates {
                guard let ticket = try? await relayTicket(token: token, kind: "liveactivity") else { continue }
                try? await client.registerActivity(botId: attributes.botId, ticket: ticket)
            }
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
            await activity.update(.init(state: state, staleDate: nil))
        }
    }
}
