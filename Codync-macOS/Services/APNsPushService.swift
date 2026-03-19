import Foundation
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync", category: "APNsPush")

@MainActor
final class APNsPushService {
    static let shared = APNsPushService()

    private let workerURL = URL(string: "https://codync-push.kevin2005ha.workers.dev")!
    private let apiSecret = "f1bf65b18870dffc6c28e69d275413e9c08b33010cb652a781106aaaaf99f329"

    private var pushTokens: [String: String] = [:] // sessionId → token hex
    private var isFetchingTokens = false
    private var lastTokenFetch: Date = .distantPast

    private init() {
        logger.info("APNs push service ready (Worker relay)")
    }

    // MARK: - Public API

    func sendUpdate(session: SessionState) async {
        guard let tokenHex = pushTokens[session.sessionId] else { return }

        let contentState = Self.buildContentState(from: session)
        let payload: [String: Any] = [
            "pushToken": tokenHex,
            "event": "update",
            "contentState": contentState
        ]
        await post(payload: payload, label: "update \(session.sessionId)")
    }

    func sendEnd(sessionId: String) async {
        guard let tokenHex = pushTokens[sessionId] else { return }

        let payload: [String: Any] = [
            "pushToken": tokenHex,
            "event": "end"
        ]
        await post(payload: payload, label: "end \(sessionId)")
        pushTokens.removeValue(forKey: sessionId)
    }

    // MARK: - Push Token Sync (from CloudKit)

    func fetchPushTokens() async {
        guard !isFetchingTokens else { return }
        guard Date().timeIntervalSince(lastTokenFetch) > 30 else { return }
        isFetchingTokens = true
        lastTokenFetch = Date() // Update even on failure to avoid spamming
        defer { isFetchingTokens = false }

        let query = CKQuery(recordType: "PushToken", predicate: NSPredicate(value: true))
        do {
            let (results, _) = try await CloudKitManager.shared.database.records(
                matching: query, inZoneWith: CloudKitManager.zoneID, resultsLimit: 20
            )
            var tokens: [String: String] = [:]
            for (_, result) in results {
                guard case .success(let record) = result,
                      let sessionId = record["sessionId"] as? String,
                      let tokenHex = record["token"] as? String else { continue }
                tokens[sessionId] = tokenHex
            }
            if tokens != pushTokens {
                pushTokens = tokens
                if !tokens.isEmpty {
                    logger.info("Fetched \(tokens.count) push tokens from CloudKit")
                }
            }
        } catch let error as CKError where error.code == .unknownItem {
            // PushToken record type doesn't exist yet — iOS hasn't saved any tokens
            // This is normal on first run, don't log as error
        } catch {
            logger.warning("Failed to fetch push tokens: \(error)")
        }
    }

    // MARK: - HTTP

    private func post(payload: [String: Any], label: String) async {
        guard let body = try? JSONSerialization.data(withJSONObject: payload) else {
            logger.error("Failed to serialize payload for \(label)")
            return
        }

        var request = URLRequest(url: workerURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiSecret)", forHTTPHeaderField: "Authorization")
        request.httpBody = body

        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, http.statusCode != 200 {
                let reason = String(data: data, encoding: .utf8) ?? ""
                logger.warning("Worker \(http.statusCode) for \(label): \(reason)")
            }
        } catch {
            logger.error("Worker POST failed for \(label): \(error.localizedDescription)")
        }
    }

    // MARK: - Content State

    private static func buildContentState(from session: SessionState) -> [String: Any] {
        let liveDuration = max(session.durationSec, Int(Date().timeIntervalSince(session.startedAt)))
        var state: [String: Any] = [
            "status": session.status.rawValue,
            "model": session.model,
            "completedCount": session.completedTaskCount,
            "totalCount": session.totalTaskCount,
            "contextPct": session.contextPct,
            "costUSD": session.costUSD,
            "durationSec": liveDuration,
            "sessionStartDate": session.startedAt.timeIntervalSinceReferenceDate
        ]
        if let task = session.currentTask, !task.isEmpty {
            state["currentTask"] = task
        }
        if let tasksData = try? JSONEncoder().encode(session.truncatedTasks),
           let tasksArray = try? JSONSerialization.jsonObject(with: tasksData) {
            state["tasks"] = tasksArray
        } else {
            state["tasks"] = [] as [Any]
        }
        return state
    }
}
