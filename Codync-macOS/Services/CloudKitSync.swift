import Foundation
import Combine
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync", category: "CloudKitSync")

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var previousSessionIds: Set<String> = []
    private var isSyncing = false
    private var quotaBackoffUntil: Date?

    private static func log(_ msg: String) {
        let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codync/sync.log")
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        let line = "[\(fmt.string(from: Date()))] \(msg)\n"
        if let h = try? FileHandle(forWritingTo: url) { h.seekToEndOfFile(); h.write(line.data(using: .utf8)!); try? h.close() }
        else { try? line.write(to: url, atomically: false, encoding: .utf8) }
    }

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager

        // Ensure custom zone exists before any saves
        Task {
            do {
                try await CloudKitManager.shared.ensureZoneExists()
                logger.info("Custom zone ready")
            } catch {
                logger.warning("Zone creation failed: \(error) — will retry on next save")
            }
        }

        stateManager.$sessions
            .throttle(for: .milliseconds(500), scheduler: RunLoop.main, latest: true)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        guard !isSyncing else { return }

        if let backoff = quotaBackoffUntil, Date() < backoff { return }
        quotaBackoffUntil = nil

        let currentIds = Set(sessions.map(\.sessionId))

        // DELETE only when session disappears from the list entirely.
        // This means the PID is dead and SessionScanner dropped it.
        // Status changes (working → idle → needsInput → completed) are always SAVED, never deleted.
        let disappearedIds = previousSessionIds.subtracting(currentIds)
        let toDelete = Array(disappearedIds)

        // SAVE any session whose state changed (exclude durationSec/updatedAt — they change every second)
        let toSave = sessions.filter { session in
            guard let prev = previousStates[session.sessionId] else { return true }
            return prev.status != session.status
                || prev.currentTask != session.currentTask
                || prev.waitingReason != session.waitingReason
                || prev.tasks != session.tasks
                || prev.model != session.model
                || prev.summary != session.summary
                || prev.costUSD != session.costUSD
                || prev.contextPct != session.contextPct
                || prev.lastEvent != session.lastEvent
        }

        guard !toDelete.isEmpty || !toSave.isEmpty else {
            previousSessionIds = currentIds
            return
        }

        isSyncing = true

        Task {
            defer {
                isSyncing = false
                previousSessionIds = currentIds
            }

            do {
                // Send end push BEFORE deleting (tokens still needed)
                for id in toDelete {
                    await APNsPushService.shared.sendEnd(sessionId: id)
                }

                if !toDelete.isEmpty {
                    Self.log("delete \(toDelete.count) sessions")
                    try await CloudKitManager.shared.deleteByIds(toDelete)
                    Self.log("deleted OK")
                    for id in toDelete {
                        previousStates.removeValue(forKey: id)
                    }
                }

                if !toSave.isEmpty {
                    // Fetch primary session once — reused for alert + overall push
                    let primary = await CloudKitManager.shared.fetchPrimarySession()

                    // Send completion alert for the locked primary session
                    // Triggers on: working → idle (task finished) or working → completed (session ended)
                    #if DEBUG
                    let primarySession = toSave.first(where: { $0.sessionId == primary.sessionId })
                    let prevStatus = primary.sessionId.flatMap { previousStates[$0]?.status }
                    Self.log("ALERT CHECK: primaryId=\(primary.sessionId ?? "nil") locked=\(primary.locked) sessionStatus=\(primarySession?.status.rawValue ?? "not-in-toSave") prevStatus=\(prevStatus?.rawValue ?? "nil")")
                    #endif
                    if let lockedPrimaryId = primary.sessionId, primary.locked,
                       let session = toSave.first(where: { $0.sessionId == lockedPrimaryId && ($0.status == .idle || $0.status == .needsInput || $0.status == .completed) }),
                       previousStates[session.sessionId]?.status == .working {
                        #if DEBUG
                        Self.log("ALERT SENDING for \(session.projectName) (\(session.status.rawValue))")
                        #endif
                        await APNsPushService.shared.sendCompletionAlert(session: session)
                    }

                    let desc = toSave.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
                    Self.log("save \(toSave.count): \(desc)")
                    try await CloudKitManager.shared.saveBatch(toSave)
                    Self.log("saved OK")
                    for session in toSave {
                        previousStates[session.sessionId] = session
                    }

                    // Push to APNs via Worker relay for Live Activity background updates
                    await APNsPushService.shared.fetchPushTokens(sessionIds: sessions.map(\.sessionId))
                    #if DEBUG
                    Self.log("APNs tokens: \(APNsPushService.shared.tokenCount)")
                    #endif
                    // Individual mode: per-session push
                    for session in toSave {
                        await APNsPushService.shared.sendUpdate(session: session)
                    }
                    // Overall mode: send all active sessions with user's primary selection
                    let allActive = sessions.filter { $0.status != .completed }
                    let primaryId = primary.sessionId
                        ?? allActive.first(where: { $0.status == .working })?.sessionId
                    // Sync primary to macOS panel sorting
                    stateManager.primarySessionId = primaryId
                    await APNsPushService.shared.sendOverallUpdate(
                        sessions: allActive,
                        primarySessionId: primaryId
                    )
                }
            } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
                let retryAfter = error.retryAfterSeconds ?? 600
                Self.log("QUOTA: backoff \(Int(retryAfter * 2))s")
                quotaBackoffUntil = Date().addingTimeInterval(retryAfter * 2)
            } catch {
                Self.log("ERROR: \(error)")
                quotaBackoffUntil = Date().addingTimeInterval(300)
            }
        }
    }
}
