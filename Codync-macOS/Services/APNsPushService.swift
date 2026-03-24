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

    private(set) var pushTokens: [String: String] = [:] // sessionId → token hex
    var tokenCount: Int { pushTokens.count }
    private var isFetchingTokens = false
    private var lastTokenFetch: Date = .distantPast
    private var lastPushTime: [String: Date] = [:]
    private let pushInterval: TimeInterval = 2 // minimum seconds between pushes per token

    private init() {
        logger.info("APNs push service ready (Worker relay)")
    }

    // MARK: - Public API

    func sendUpdate(session: SessionState) async {
        guard let tokenHex = pushTokens[session.sessionId] else { return }
        let key = session.sessionId
        if let last = lastPushTime[key], Date().timeIntervalSince(last) < pushInterval { return }
        lastPushTime[key] = Date()

        let contentState = Self.buildContentState(from: session)
        let payload: [String: Any] = [
            "pushToken": tokenHex,
            "event": "update",
            "contentState": contentState
        ]
        await post(payload: payload, label: "update \(session.sessionId)")
    }

    func sendOverallUpdate(sessions: [SessionState], primarySessionId: String?) async {
        guard let tokenHex = pushTokens["__overall__"] else { return }
        let key = "__overall__"
        if let last = lastPushTime[key], Date().timeIntervalSince(last) < pushInterval { return }
        lastPushTime[key] = Date()

        let summaries: [[String: Any]] = sessions.prefix(4).map { session in
            let liveDuration = max(session.durationSec, Int(Date().timeIntervalSince(session.startedAt)))
            var s: [String: Any] = [
                "sessionId": session.sessionId,
                "projectName": session.projectName,
                "status": session.status.rawValue,
                "model": session.model,
                "costUSD": session.costUSD,
                "durationSec": liveDuration,
                "completedCount": session.completedTaskCount,
                "totalCount": session.totalTaskCount
            ]
            if let task = session.currentTask { s["currentTask"] = task }
            return s
        }

        let contentState: [String: Any] = [
            "sessions": summaries,
            "primarySessionId": primarySessionId as Any,
            "totalCost": sessions.reduce(0) { $0 + $1.costUSD },
            "isDark": true
        ]

        let payload: [String: Any] = [
            "pushToken": tokenHex,
            "event": "update",
            "contentState": contentState
        ]
        logger.info("Sending overall update (\(sessions.count) sessions)")
        await post(payload: payload, label: "overall-update")
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

    // MARK: - Alert Push (Pro: session completion)

    func sendCompletionAlert(session: SessionState) async {
        // Fetch device push token from CloudKit
        let recordID = CKRecord.ID(
            recordName: "device-push-token",
            zoneID: CloudKitManager.zoneID
        )
        guard let record = try? await CloudKitManager.shared.database.record(for: recordID),
              let tokenHex = record["token"] as? String else {
            return
        }

        let payload: [String: Any] = [
            "pushToken": tokenHex,
            "type": "alert",
            "title": session.projectName,
            "body": "Session complete"
        ]
        // Retry up to 3 times with backoff (429 from APNs rate limit)
        for attempt in 0..<3 {
            if attempt > 0 {
                try? await Task.sleep(for: .seconds(Double(attempt) * 2))
            }
            let success = await postWithResult(payload: payload, label: "alert \(session.projectName)")
            if success { break }
        }
    }

    // MARK: - Push Token Sync (from CloudKit)

    func fetchPushTokens(sessionIds: [String] = []) async {
        guard !isFetchingTokens else { return }
        guard Date().timeIntervalSince(lastTokenFetch) > 10 else { return }
        isFetchingTokens = true
        lastTokenFetch = Date()
        defer { isFetchingTokens = false }

        // Fetch known push token records by ID (query requires indexing; direct fetch doesn't)
        var allIds = sessionIds
        if !allIds.contains("__overall__") { allIds.append("__overall__") }

        let recordIDs = allIds.map {
            CKRecord.ID(recordName: "pushtoken-\($0)", zoneID: CloudKitManager.zoneID)
        }

        do {
            let recordResults = try await CloudKitManager.shared.database.records(for: recordIDs)
            var tokens: [String: String] = [:]
            for (_, result) in recordResults {
                guard case .success(let record) = result,
                      let sessionId = record["sessionId"] as? String,
                      let tokenHex = record["token"] as? String else { continue }
                tokens[sessionId] = tokenHex
            }
            if tokens != pushTokens {
                pushTokens = tokens
                logger.info("Fetched \(tokens.count) push tokens from CloudKit")
            }
        } catch {
            logger.warning("Failed to fetch push tokens: \(error)")
        }
    }

    // MARK: - HTTP

    private func postWithResult(payload: [String: Any], label: String) async -> Bool {
        guard let body = try? JSONSerialization.data(withJSONObject: payload) else { return false }
        var request = URLRequest(url: workerURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiSecret)", forHTTPHeaderField: "Authorization")
        request.httpBody = body
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse {
                #if DEBUG
                if http.statusCode == 200 {
                    Self.log("APNs → \(label) ✓")
                } else {
                    let reason = String(data: data, encoding: .utf8) ?? ""
                    Self.log("APNs → \(label) ✗ \(http.statusCode): \(reason)")
                }
                #endif
                _ = data
                return http.statusCode == 200
            }
        } catch {
            #if DEBUG
            Self.log("APNs → \(label) ✗ \(error.localizedDescription)")
            #endif
        }
        return false
    }

    private func post(payload: [String: Any], label: String) async {
        guard let body = try? JSONSerialization.data(withJSONObject: payload) else {
            Self.log("APNs FAIL serialize \(label)")
            return
        }

        var request = URLRequest(url: workerURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiSecret)", forHTTPHeaderField: "Authorization")
        request.httpBody = body

        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse {
                #if DEBUG
                if http.statusCode == 200 {
                    Self.log("APNs → \(label) ✓")
                } else {
                    let reason = String(data: data, encoding: .utf8) ?? ""
                    Self.log("APNs → \(label) ✗ \(http.statusCode): \(reason)")
                }
                #endif
                _ = data // suppress unused warning in release
            }
        } catch {
            #if DEBUG
            Self.log("APNs → \(label) ✗ \(error.localizedDescription)")
            #endif
        }
    }

    private static func log(_ msg: String) {
        let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codync/sync.log")
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        let line = "[\(fmt.string(from: Date()))] \(msg)\n"
        if let h = try? FileHandle(forWritingTo: url) { h.seekToEndOfFile(); h.write(line.data(using: .utf8)!); try? h.close() }
        else { try? line.write(to: url, atomically: false, encoding: .utf8) }
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
