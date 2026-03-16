import Foundation
import Combine
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "CloudKitSync")

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var isSyncing = false

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .debounce(for: .seconds(5), scheduler: RunLoop.main)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
        logger.info("CloudKitSync initialized")
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        guard !isSyncing else {
            logger.debug("Sync skipped — already in progress")
            return
        }

        // Filter to only changed sessions
        let changed = sessions.filter { session in
            previousStates[session.sessionId]?.updatedAt != session.updatedAt
        }

        guard !changed.isEmpty else {
            logger.debug("Sync skipped — no changes (\(sessions.count) sessions unchanged)")
            return
        }

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
                    // Small delay between saves to avoid rate limiting
                    try? await Task.sleep(for: .seconds(1))
                } catch {
                    logger.error("Sync failed for \(session.sessionId): \(error.localizedDescription)")
                }
            }
            if syncedCount > 0 {
                logger.info("Synced \(syncedCount)/\(changed.count) sessions to CloudKit")
            }
        }
    }
}
