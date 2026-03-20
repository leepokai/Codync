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
                if !toDelete.isEmpty {
                    logger.info("deleteByIds \(toDelete.count) sessions (PID dead)")
                    try await CloudKitManager.shared.deleteByIds(toDelete)
                    logger.info("Deleted \(toDelete.count) sessions")
                    for id in toDelete {
                        previousStates.removeValue(forKey: id)
                    }
                }

                if !toSave.isEmpty {
                    logger.info("saveBatch \(toSave.count) sessions")
                    try await CloudKitManager.shared.saveBatch(toSave)
                    logger.info("Saved \(toSave.count) sessions")
                    for session in toSave {
                        previousStates[session.sessionId] = session
                    }
                }
            } catch let error as CKError where error.code == .quotaExceeded || error.code == .requestRateLimited {
                let retryAfter = error.retryAfterSeconds ?? 600
                logger.warning("QUOTA: backoff \(Int(retryAfter * 2))s")
                quotaBackoffUntil = Date().addingTimeInterval(retryAfter * 2)
            } catch {
                logger.error("CloudKit sync error: \(error)")
                quotaBackoffUntil = Date().addingTimeInterval(300)
            }
        }
    }
}
