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
    private var syncTask: Task<Void, Never>?

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        stateManager.$sessions
            .debounce(for: .seconds(2), scheduler: RunLoop.main)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
        logger.info("CloudKitSync initialized")
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        syncTask?.cancel()
        syncTask = Task {
            var syncedCount = 0
            for session in sessions {
                guard !Task.isCancelled else { return }
                let previous = previousStates[session.sessionId]
                do {
                    try await CloudKitManager.shared.saveIfChanged(session, previous: previous)
                    previousStates[session.sessionId] = session
                    syncedCount += 1
                } catch {
                    logger.error("CloudKit sync failed for session \(session.sessionId): \(error.localizedDescription)")
                }
            }
            if syncedCount > 0 {
                logger.debug("Synced \(syncedCount) sessions to CloudKit")
            }
            // Cleanup less frequently — only if we synced something
            if syncedCount > 0 {
                do {
                    try await CloudKitManager.shared.deleteCompleted()
                } catch {
                    logger.debug("CloudKit cleanup skipped: \(error.localizedDescription)")
                }
            }
        }
    }
}
