import Foundation
import Combine
import CloudKit
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKitSync")

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var isSyncing = false
    private var quotaBackoffUntil: Date?

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .throttle(for: .seconds(30), scheduler: RunLoop.main, latest: true)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
        logger.info("CloudKitSync initialized")
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        guard !isSyncing else { return }

        // Respect quota backoff
        if let backoff = quotaBackoffUntil, Date() < backoff {
            return
        }
        quotaBackoffUntil = nil

        let changed = sessions.filter { session in
            session.status != .completed
            && previousStates[session.sessionId]?.updatedAt != session.updatedAt
        }

        guard !changed.isEmpty else { return }

        isSyncing = true
        logger.info("Syncing \(changed.count) changed sessions to CloudKit...")

        Task {
            defer { isSyncing = false }

            var syncedCount = 0
            for session in changed {
                do {
                    try await CloudKitManager.shared.save(session)
                    previousStates[session.sessionId] = session
                    syncedCount += 1
                    try? await Task.sleep(for: .seconds(3))
                } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
                    let retryAfter = error.retryAfterSeconds ?? 300
                    logger.warning("CloudKit quota hit, backing off \(retryAfter)s")
                    quotaBackoffUntil = Date().addingTimeInterval(retryAfter)
                    break
                } catch {
                    logger.error("Sync failed for \(session.sessionId): \(error.localizedDescription)")
                    break
                }
            }
            if syncedCount > 0 {
                logger.info("Synced \(syncedCount)/\(changed.count) sessions to CloudKit")
            }
        }
    }
}
