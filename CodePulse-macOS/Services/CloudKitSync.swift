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
        // Don't start a new sync if one is already running (e.g. retrying)
        guard !isSyncing else {
            logger.debug("Sync skipped — already in progress")
            return
        }
        isSyncing = true

        Task {
            defer { isSyncing = false }

            var syncedCount = 0
            for session in sessions {
                let previous = previousStates[session.sessionId]
                guard session.updatedAt != previous?.updatedAt else { continue }

                do {
                    try await CloudKitManager.shared.save(session)
                    previousStates[session.sessionId] = session
                    syncedCount += 1
                } catch {
                    logger.error("Sync failed for \(session.sessionId): \(error.localizedDescription)")
                    // Don't block other sessions on one failure
                }
            }
            if syncedCount > 0 {
                logger.info("Synced \(syncedCount) sessions to CloudKit")
            }
        }
    }
}
